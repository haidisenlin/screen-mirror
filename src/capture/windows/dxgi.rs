use crate::capture::{CaptureConfig, CapturedFrame, VideoCapture};

pub struct DxgiCapture {
    width: u32,
    height: u32,
}

impl VideoCapture for DxgiCapture {
    fn new(_config: &CaptureConfig) -> anyhow::Result<Self> {
        todo!("DXGI Desktop Duplication implementation")
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn next_frame(&self) -> Option<CapturedFrame> {
        todo!("DXGI AcquireNextFrame implementation")
    }
}
