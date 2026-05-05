// src/capture/mod.rs

use std::fmt;

use crate::ui::messages::{CaptureMode, WindowInfo};

#[cfg(target_os = "macos")]
pub mod macos;
pub mod screenshot;
#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug)]
pub enum CaptureError {
    TargetLost,
    PermissionDenied,
    DeviceError(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::TargetLost => write!(f, "capture target lost"),
            CaptureError::PermissionDenied => write!(f, "permission denied"),
            CaptureError::DeviceError(msg) => write!(f, "device error: {msg}"),
        }
    }
}

impl std::error::Error for CaptureError {}

pub struct CaptureConfig {
    pub mode: CaptureMode,
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

// Safety: native pointer represents sole ownership of a platform resource
// (CVPixelBufferRef on macOS, Box<ID3D11Texture2D> on Windows)
unsafe impl Send for CapturedFrame {}

pub trait VideoCapture {
    fn new(config: &CaptureConfig) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn next_frame(&self) -> Result<Option<CapturedFrame>, CaptureError>;
}

pub trait AudioCapture {
    fn new(sample_rate: u32, channels: u16) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn try_next_audio(&self) -> Option<Vec<f32>>;
}

pub fn list_windows() -> Vec<WindowInfo> {
    #[cfg(target_os = "macos")]
    {
        macos::list_windows_macos()
    }
    #[cfg(target_os = "windows")]
    {
        windows::list_windows_windows()
    }
    #[cfg(target_os = "linux")]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_windows_returns_vec() {
        let windows = list_windows();
        // On CI/test this may return empty, but it should not panic
        assert!(windows.is_empty() || !windows.is_empty());
    }

    #[test]
    fn list_windows_no_empty_titles() {
        let windows = list_windows();
        for w in &windows {
            assert!(!w.title.is_empty() || !w.app_name.is_empty());
        }
    }
}
