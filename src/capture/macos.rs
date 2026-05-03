use std::sync::mpsc::{self, Receiver, SyncSender};

use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::prelude::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutputTrait,
    SCStreamOutputType,
};

use super::CaptureConfig;

struct FrameSender(SyncSender<CMSampleBuffer>);

impl SCStreamOutputTrait for FrameSender {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type == SCStreamOutputType::Screen {
            // Drop the frame silently if the channel is full — the consumer is behind.
            let _ = self.0.try_send(sample);
        }
    }
}

/// Owns a live `SCStream` and provides frames one at a time via `next_frame`.
pub struct MacOsCapture {
    stream: SCStream,
    rx: Receiver<CMSampleBuffer>,
}

impl MacOsCapture {
    /// Start capturing the primary display with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if no display is found, or if the stream fails to start.
    pub fn new(config: &CaptureConfig) -> anyhow::Result<Self> {
        let content = SCShareableContent::get()?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no display found"))?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        let sc_config = SCStreamConfiguration::new()
            .with_width(config.width)
            .with_height(config.height)
            .with_fps(config.fps);

        let (tx, rx) = mpsc::sync_channel(2);

        let mut stream = SCStream::new(&filter, &sc_config);
        stream
            .add_output_handler(FrameSender(tx), SCStreamOutputType::Screen)
            .ok_or_else(|| anyhow::anyhow!("failed to add output handler"))?;

        stream.start_capture()?;

        Ok(Self { stream, rx })
    }

    /// Block until the next frame arrives and return it.
    ///
    /// Returns `None` when the capture stream has stopped.
    pub fn next_frame(&self) -> Option<CMSampleBuffer> {
        self.rx.recv().ok()
    }
}

impl Drop for MacOsCapture {
    fn drop(&mut self) {
        // Best-effort stop; ignore errors on teardown.
        let _ = self.stream.stop_capture();
    }
}
