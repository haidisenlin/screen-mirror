// src/capture/mod.rs

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub struct CaptureConfig {
    pub fps: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "macos")]
pub type NativeFrame = *mut std::ffi::c_void; // CVPixelBufferRef

#[cfg(target_os = "windows")]
pub type NativeFrame = *mut std::ffi::c_void; // ID3D11Texture2D*

pub struct CapturedFrame {
    pub native: NativeFrame,
    pub timestamp_ns: u64,
}

pub trait VideoCapture {
    fn new(config: &CaptureConfig) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn next_frame(&self) -> Option<CapturedFrame>;
}

pub trait AudioCapture {
    fn new(sample_rate: u32, channels: u16) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn try_next_audio(&self) -> Option<Vec<f32>>;
}
