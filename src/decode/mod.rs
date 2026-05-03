#[cfg(target_os = "macos")]
pub mod videotoolbox;

pub struct DecoderConfig {
    pub width: u32,
    pub height: u32,
}

pub struct DecodedFrame {
    pub pixel_buffer: *mut std::ffi::c_void, // CVPixelBufferRef
    pub timestamp: u64,
}

impl Drop for DecodedFrame {
    fn drop(&mut self) {
        if !self.pixel_buffer.is_null() {
            unsafe {
                unsafe extern "C" {
                    fn CFRelease(cf: *mut std::ffi::c_void);
                }
                CFRelease(self.pixel_buffer);
            }
        }
    }
}
