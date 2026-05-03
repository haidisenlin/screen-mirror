#[cfg(target_os = "macos")]
pub mod macos;

pub struct CaptureConfig {
    pub fps: u32,
    pub width: u32,
    pub height: u32,
}
