use std::sync::mpsc::{self, Receiver, SyncSender};

use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::prelude::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutputTrait,
    SCStreamOutputType,
};

use super::CaptureConfig;

unsafe extern "C" {
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGMainDisplayID() -> u32;
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

/// Owns a live `SCStream` and provides frames one at a time via `next_frame`.
pub struct MacOsCapture {
    stream: SCStream,
    video_rx: Receiver<CMSampleBuffer>,
    audio_rx: Option<Receiver<CMSampleBuffer>>,
    width: u32,
    height: u32,
}

impl MacOsCapture {
    /// Start capturing the primary display.
    ///
    /// If `config.width` and `config.height` are 0, captures at the display's
    /// native pixel resolution (Retina-aware).
    pub fn new(config: &CaptureConfig) -> anyhow::Result<Self> {
        let content = SCShareableContent::get()?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no display found"))?;

        let (cap_w, cap_h) = if config.width == 0 || config.height == 0 {
            let display_id = display.display_id();
            let pw = unsafe { CGDisplayPixelsWide(display_id) } as u32;
            let ph = unsafe { CGDisplayPixelsHigh(display_id) } as u32;
            if pw == 0 || ph == 0 {
                let fallback_id = unsafe { CGMainDisplayID() };
                let pw = unsafe { CGDisplayPixelsWide(fallback_id) } as u32;
                let ph = unsafe { CGDisplayPixelsHigh(fallback_id) } as u32;
                (pw.max(1920), ph.max(1080))
            } else {
                (pw, ph)
            }
        } else {
            (config.width, config.height)
        };

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        let mut sc_config = SCStreamConfiguration::new()
            .with_width(cap_w)
            .with_height(cap_h)
            .with_fps(config.fps);

        if config.capture_audio {
            sc_config = sc_config
                .with_captures_audio(true)
                .with_sample_rate(48000)
                .with_channel_count(2);
        }

        let (video_tx, video_rx) = mpsc::sync_channel(2);

        let audio_rx = if config.capture_audio {
            let (tx, rx) = mpsc::sync_channel(8);
            let mut stream = SCStream::new(&filter, &sc_config);
            stream
                .add_output_handler(VideoHandler { tx: video_tx }, SCStreamOutputType::Screen)
                .ok_or_else(|| anyhow::anyhow!("failed to add video output handler"))?;
            stream
                .add_output_handler(AudioHandler { tx }, SCStreamOutputType::Audio)
                .ok_or_else(|| anyhow::anyhow!("failed to add audio output handler"))?;
            stream.start_capture()?;
            return Ok(Self {
                stream,
                video_rx,
                audio_rx: Some(rx),
                width: cap_w,
                height: cap_h,
            });
        } else {
            None
        };

        let mut stream = SCStream::new(&filter, &sc_config);
        stream
            .add_output_handler(VideoHandler { tx: video_tx }, SCStreamOutputType::Screen)
            .ok_or_else(|| anyhow::anyhow!("failed to add video output handler"))?;

        stream.start_capture()?;

        Ok(Self {
            stream,
            video_rx,
            audio_rx,
            width: cap_w,
            height: cap_h,
        })
    }

    /// The actual capture width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The actual capture height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Block until the next frame arrives and return it.
    pub fn next_frame(&self) -> Option<CMSampleBuffer> {
        self.video_rx.recv().ok()
    }

    /// Block until the next audio sample arrives and return it.
    /// Returns `None` immediately if audio capture is disabled.
    pub fn next_audio(&self) -> Option<CMSampleBuffer> {
        self.audio_rx.as_ref()?.recv().ok()
    }

    /// Non-blocking attempt to get the next audio sample.
    /// Returns `None` if no sample is available or audio capture is disabled.
    pub fn try_next_audio(&self) -> Option<CMSampleBuffer> {
        self.audio_rx.as_ref()?.try_recv().ok()
    }
}

impl Drop for MacOsCapture {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
    }
}
