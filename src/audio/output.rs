use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

use crate::transport::jitter::AudioJitterBuffer;

// CoreAudio types
type AudioUnit = *mut c_void;
type OSStatus = i32;

// Constants
const K_AUDIO_UNIT_TYPE_OUTPUT: u32 = u32::from_be_bytes(*b"auou");
const K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"def ");
const K_AUDIO_UNIT_MANUFACTURER_APPLE: u32 = u32::from_be_bytes(*b"appl");
const K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK: u32 = 23;
const K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT: u32 = 8;
const K_AUDIO_FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const K_AUDIO_FORMAT_FLAG_IS_FLOAT: u32 = 1;
const K_AUDIO_FORMAT_FLAG_IS_PACKED: u32 = 8;
const K_AUDIO_UNIT_SCOPE_INPUT: u32 = 1;

const SAMPLE_RATE: f64 = 48000.0;
const CHANNELS: u32 = 2;
const BYTES_PER_SAMPLE: u32 = 4; // f32
const JITTER_FRAME_SAMPLES: usize = 960; // 10ms * 48kHz * 2 channels

#[repr(C)]
struct AudioComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AURenderCallbackStruct {
    input_proc: unsafe extern "C" fn(
        *mut c_void,
        *mut u32,
        *const AudioTimeStamp,
        u32,
        u32,
        *mut AudioBufferList,
    ) -> OSStatus,
    input_proc_ref_con: *mut c_void,
}

#[repr(C)]
struct AudioTimeStamp {
    _sample_time: f64,
    _host_time: u64,
    _rate_scalar: f64,
    _word_clock_time: u64,
    _smpte_time: [u8; 24],
    _flags: u32,
    _reserved: u32,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioComponentFindNext(
        component: *mut c_void,
        desc: *const AudioComponentDescription,
    ) -> *mut c_void;
    fn AudioComponentInstanceNew(component: *mut c_void, out: *mut AudioUnit) -> OSStatus;
    fn AudioUnitInitialize(unit: AudioUnit) -> OSStatus;
    fn AudioOutputUnitStart(unit: AudioUnit) -> OSStatus;
    fn AudioOutputUnitStop(unit: AudioUnit) -> OSStatus;
    fn AudioComponentInstanceDispose(unit: AudioUnit) -> OSStatus;
    fn AudioUnitSetProperty(
        unit: AudioUnit,
        prop_id: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> OSStatus;
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {}

/// State shared between the render callback and AudioOutput.
struct CallbackState {
    jitter_buffer: Arc<Mutex<AudioJitterBuffer>>,
    samples_played: Arc<AtomicU64>,
    /// Intermediate buffer to accumulate jitter frames for the callback.
    frame_buf: Vec<f32>,
    /// Leftover samples from a partially consumed jitter frame.
    residual: Vec<f32>,
    residual_offset: usize,
}

unsafe extern "C" fn render_callback(
    in_ref_con: *mut c_void,
    _io_action_flags: *mut u32,
    _in_time_stamp: *const AudioTimeStamp,
    _in_bus_number: u32,
    in_number_frames: u32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let state = unsafe { &mut *(in_ref_con as *mut CallbackState) };
    let needed_samples = in_number_frames as usize * CHANNELS as usize;

    let buf_list = unsafe { &mut *io_data };
    let buffer = &mut buf_list.buffers[0];
    let out_ptr = buffer.data as *mut f32;
    let out_slice = unsafe { std::slice::from_raw_parts_mut(out_ptr, needed_samples) };

    let mut written = 0;

    // First, drain any residual from a previous partial pull.
    if state.residual_offset < state.residual.len() {
        let avail = state.residual.len() - state.residual_offset;
        let to_copy = avail.min(needed_samples - written);
        out_slice[written..written + to_copy].copy_from_slice(
            &state.residual[state.residual_offset..state.residual_offset + to_copy],
        );
        state.residual_offset += to_copy;
        written += to_copy;
    }

    // Pull jitter frames until we have enough samples.
    while written < needed_samples {
        state.frame_buf.resize(JITTER_FRAME_SAMPLES, 0.0);
        let got_frame = {
            if let Ok(mut jb) = state.jitter_buffer.lock() {
                jb.pull_frame(&mut state.frame_buf)
            } else {
                false
            }
        };

        if !got_frame {
            // Underrun: fill remainder with silence.
            out_slice[written..].fill(0.0);
            break;
        }

        let remaining = needed_samples - written;
        if JITTER_FRAME_SAMPLES <= remaining {
            out_slice[written..written + JITTER_FRAME_SAMPLES]
                .copy_from_slice(&state.frame_buf[..JITTER_FRAME_SAMPLES]);
            written += JITTER_FRAME_SAMPLES;
        } else {
            out_slice[written..written + remaining].copy_from_slice(&state.frame_buf[..remaining]);
            written += remaining;
            // Store leftover in residual.
            state.residual.clear();
            state
                .residual
                .extend_from_slice(&state.frame_buf[remaining..JITTER_FRAME_SAMPLES]);
            state.residual_offset = 0;
        }
    }

    state
        .samples_played
        .fetch_add(in_number_frames as u64, Ordering::Relaxed);

    0 // noErr
}

pub struct AudioOutput {
    unit: AudioUnit,
    jitter_buffer: Arc<Mutex<AudioJitterBuffer>>,
    samples_played: Arc<AtomicU64>,
    /// Prevent the callback state from being freed while CoreAudio holds a pointer.
    _callback_state: *mut CallbackState,
}

// AudioUnit is thread-safe via CoreAudio's internal synchronization.
unsafe impl Send for AudioOutput {}
unsafe impl Sync for AudioOutput {}

impl AudioOutput {
    pub fn new(jitter_buffer: Arc<Mutex<AudioJitterBuffer>>) -> Result<Self> {
        let samples_played = Arc::new(AtomicU64::new(0));

        let desc = AudioComponentDescription {
            component_type: K_AUDIO_UNIT_TYPE_OUTPUT,
            component_sub_type: K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT,
            component_manufacturer: K_AUDIO_UNIT_MANUFACTURER_APPLE,
            component_flags: 0,
            component_flags_mask: 0,
        };

        let component = unsafe { AudioComponentFindNext(ptr::null_mut(), &desc) };
        if component.is_null() {
            bail!("CoreAudio: could not find default output AudioComponent");
        }

        let mut unit: AudioUnit = ptr::null_mut();
        let status = unsafe { AudioComponentInstanceNew(component, &mut unit) };
        if status != 0 {
            bail!("CoreAudio: AudioComponentInstanceNew failed ({})", status);
        }

        // Set stream format.
        let asbd = AudioStreamBasicDescription {
            sample_rate: SAMPLE_RATE,
            format_id: K_AUDIO_FORMAT_LINEAR_PCM,
            format_flags: K_AUDIO_FORMAT_FLAG_IS_FLOAT | K_AUDIO_FORMAT_FLAG_IS_PACKED,
            bytes_per_packet: CHANNELS * BYTES_PER_SAMPLE,
            frames_per_packet: 1,
            bytes_per_frame: CHANNELS * BYTES_PER_SAMPLE,
            channels_per_frame: CHANNELS,
            bits_per_channel: 32,
            reserved: 0,
        };

        let status = unsafe {
            AudioUnitSetProperty(
                unit,
                K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT,
                K_AUDIO_UNIT_SCOPE_INPUT,
                0,
                &asbd as *const _ as *const c_void,
                size_of::<AudioStreamBasicDescription>() as u32,
            )
        };
        if status != 0 {
            unsafe { AudioComponentInstanceDispose(unit) };
            bail!("CoreAudio: set stream format failed ({})", status);
        }

        // Create callback state on the heap with a stable address.
        let callback_state = Box::into_raw(Box::new(CallbackState {
            jitter_buffer: Arc::clone(&jitter_buffer),
            samples_played: Arc::clone(&samples_played),
            frame_buf: vec![0.0; JITTER_FRAME_SAMPLES],
            residual: Vec::new(),
            residual_offset: 0,
        }));

        let cb_struct = AURenderCallbackStruct {
            input_proc: render_callback,
            input_proc_ref_con: callback_state as *mut c_void,
        };

        let status = unsafe {
            AudioUnitSetProperty(
                unit,
                K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK,
                K_AUDIO_UNIT_SCOPE_INPUT,
                0,
                &cb_struct as *const _ as *const c_void,
                size_of::<AURenderCallbackStruct>() as u32,
            )
        };
        if status != 0 {
            unsafe {
                let _ = Box::from_raw(callback_state);
                AudioComponentInstanceDispose(unit);
            }
            bail!("CoreAudio: set render callback failed ({})", status);
        }

        let status = unsafe { AudioUnitInitialize(unit) };
        if status != 0 {
            unsafe {
                let _ = Box::from_raw(callback_state);
                AudioComponentInstanceDispose(unit);
            }
            bail!("CoreAudio: AudioUnitInitialize failed ({})", status);
        }

        let status = unsafe { AudioOutputUnitStart(unit) };
        if status != 0 {
            unsafe {
                let _ = Box::from_raw(callback_state);
                AudioComponentInstanceDispose(unit);
            }
            bail!("CoreAudio: AudioOutputUnitStart failed ({})", status);
        }

        Ok(Self {
            unit,
            jitter_buffer,
            samples_played,
            _callback_state: callback_state,
        })
    }

    pub fn jitter_buffer(&self) -> &Arc<Mutex<AudioJitterBuffer>> {
        &self.jitter_buffer
    }

    pub fn samples_played(&self) -> u64 {
        self.samples_played.load(Ordering::Relaxed)
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        unsafe {
            AudioOutputUnitStop(self.unit);
            AudioComponentInstanceDispose(self.unit);
            let _ = Box::from_raw(self._callback_state);
        }
    }
}
