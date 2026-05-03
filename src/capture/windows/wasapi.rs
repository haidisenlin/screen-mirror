use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::core::Interface;

use crate::capture::AudioCapture;

pub struct WasapiCapture {
    rx: Receiver<Vec<f32>>,
}

impl AudioCapture for WasapiCapture {
    fn new(sample_rate: u32, channels: u16) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(64);

        thread::Builder::new()
            .name("wasapi-loopback".into())
            .spawn(move || {
                if let Err(e) = wasapi_loop(tx, sample_rate, channels) {
                    tracing::error!("WASAPI thread error: {e}");
                }
            })?;

        Ok(Self { rx })
    }

    fn try_next_audio(&self) -> Option<Vec<f32>> {
        match self.rx.try_recv() {
            Ok(pcm) => Some(pcm),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }
}

fn wasapi_loop(
    tx: mpsc::SyncSender<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)?;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

        let stream_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

        let block_align = channels * 4; // f32 = 4 bytes
        let format = windows::Win32::Media::Audio::WAVEFORMATEX {
            wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
            nChannels: channels,
            nSamplesPerSec: sample_rate,
            nAvgBytesPerSec: sample_rate * block_align as u32,
            nBlockAlign: block_align,
            wBitsPerSample: 32,
            cbSize: 0,
        };

        let buffer_duration = 200_000; // 20ms in 100ns units

        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            buffer_duration,
            0,
            &format,
            None,
        )?;

        let capture_client: IAudioCaptureClient = audio_client.GetService()?;
        audio_client.Start()?;

        tracing::info!("WASAPI: loopback started, {}Hz {}ch f32", sample_rate, channels);

        loop {
            thread::sleep(Duration::from_millis(5));

            loop {
                let mut buffer_ptr = std::ptr::null_mut();
                let mut frames_available = 0u32;
                let mut flags = 0u32;

                let hr = capture_client.GetBuffer(
                    &mut buffer_ptr,
                    &mut frames_available,
                    &mut flags,
                    None,
                    None,
                );

                if hr.is_err() || frames_available == 0 {
                    break;
                }

                let sample_count = frames_available as usize * channels as usize;

                let pcm = if flags & 0x2 != 0 {
                    // AUDCLNT_BUFFERFLAGS_SILENT
                    vec![0.0f32; sample_count]
                } else {
                    let slice = std::slice::from_raw_parts(
                        buffer_ptr as *const f32,
                        sample_count,
                    );
                    slice.to_vec()
                };

                capture_client.ReleaseBuffer(frames_available)?;

                if tx.try_send(pcm).is_err() {
                    // Receiver full — drop this chunk
                }
            }
        }
    }
}
