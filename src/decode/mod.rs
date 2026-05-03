#[cfg(target_os = "macos")]
pub mod videotoolbox;

pub struct DecoderConfig {
    pub width: u32,
    pub height: u32,
}

pub struct DecodedFrame {
    pub pixel_buffer: *mut std::ffi::c_void, // CVPixelBufferRef — caller must CFRelease
    pub timestamp: u64,
}
