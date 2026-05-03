// src/capture/macos.rs

use std::sync::mpsc::{self, Receiver, SyncSender};

use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::prelude::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutputTrait,
    SCStreamOutputType,
};

use super::{CaptureConfig, CapturedFrame, NativeFrame, VideoCapture};

type CGDisplayModeRef = *mut std::ffi::c_void;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayCopyDisplayMode(display: u32) -> CGDisplayModeRef;
    fn CGDisplayModeGetPixelWidth(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayModeRelease(mode: CGDisplayModeRef);
}

pub fn native_resolution() -> (u32, u32) {
    let display_id = unsafe { CGMainDisplayID() };
    physical_display_size(display_id).unwrap_or((1920, 1080))
}

fn physical_display_size(display_id: u32) -> Option<(u32, u32)> {
    unsafe {
        let mode = CGDisplayCopyDisplayMode(display_id);
        if mode.is_null() {
            return None;
        }
        let w = CGDisplayModeGetPixelWidth(mode) as u32;
        let h = CGDisplayModeGetPixelHeight(mode) as u32;
        CGDisplayModeRelease(mode);
        if w > 0 && h > 0 { Some((w, h)) } else { None }
    }
}

struct VideoHandler {
    tx: SyncSender<CMSampleBuffer>,
}

impl SCStreamOutputTrait for VideoHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, _of_type: SCStreamOutputType) {
        let _ = self.tx.try_send(sample);
    }
}

struct AudioHandler {
    tx: SyncSender<CMSampleBuffer>,
}

impl SCStreamOutputTrait for AudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, _of_type: SCStreamOutputType) {
        let _ = self.tx.try_send(sample);
    }
}

pub struct MacOsCapture {
    _stream: SCStream,
    video_rx: Receiver<CMSampleBuffer>,
    audio_rx: Receiver<CMSampleBuffer>,
    width: u32,
    height: u32,
}

impl VideoCapture for MacOsCapture {
    fn new(config: &CaptureConfig) -> anyhow::Result<Self> {
        let content = SCShareableContent::get()?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no display found"))?;

        let (cap_w, cap_h) = if config.width == 0 || config.height == 0 {
            let display_id = display.display_id();
            physical_display_size(display_id)
                .or_else(|| {
                    let fallback_id = unsafe { CGMainDisplayID() };
                    physical_display_size(fallback_id)
                })
                .unwrap_or((1920, 1080))
        } else {
            (config.width, config.height)
        };

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        let sc_config = SCStreamConfiguration::new()
            .with_width(cap_w)
            .with_height(cap_h)
            .with_fps(config.fps)
            .with_captures_audio(true)
            .with_sample_rate(48000)
            .with_channel_count(2);

        let (video_tx, video_rx) = mpsc::sync_channel(2);
        let (audio_tx, audio_rx) = mpsc::sync_channel(8);

        let mut stream = SCStream::new(&filter, &sc_config);
        stream
            .add_output_handler(VideoHandler { tx: video_tx }, SCStreamOutputType::Screen)
            .ok_or_else(|| anyhow::anyhow!("failed to add video output handler"))?;
        stream
            .add_output_handler(AudioHandler { tx: audio_tx }, SCStreamOutputType::Audio)
            .ok_or_else(|| anyhow::anyhow!("failed to add audio output handler"))?;
        stream.start_capture()?;

        Ok(Self {
            _stream: stream,
            video_rx,
            audio_rx,
            width: cap_w,
            height: cap_h,
        })
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn next_frame(&self) -> Option<CapturedFrame> {
        loop {
            let sample = self.video_rx.recv().ok()?;
            let pixel_buffer = sample.image_buffer()?;
            let timestamp_ns = sample.display_time().unwrap_or_else(|| {
                let t = sample.presentation_timestamp();
                (t.value as u64 * 1_000_000_000) / t.timescale as u64
            });
            return Some(CapturedFrame {
                native: pixel_buffer.as_ptr() as NativeFrame,
                timestamp_ns,
            });
        }
    }
}

impl MacOsCapture {
    pub fn try_next_audio(&self) -> Option<Vec<f32>> {
        let sample = self.audio_rx.try_recv().ok()?;
        extract_audio_pcm(&sample)
    }
}

fn extract_audio_pcm(sample: &CMSampleBuffer) -> Option<Vec<f32>> {
    let buffer_list = sample.audio_buffer_list()?;
    let mut pcm = Vec::new();
    for buf in buffer_list.iter() {
        let data = buf.data();
        if data.len() >= 4 {
            let floats: &[f32] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };
            pcm.extend_from_slice(floats);
        }
    }
    if pcm.is_empty() { None } else { Some(pcm) }
}
