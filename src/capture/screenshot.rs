// src/capture/screenshot.rs

use anyhow::Result;

pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn take_screenshot() -> Result<Screenshot> {
    #[cfg(target_os = "macos")]
    {
        take_screenshot_macos()
    }
    #[cfg(target_os = "windows")]
    {
        take_screenshot_windows()
    }
    #[cfg(target_os = "linux")]
    {
        anyhow::bail!("screenshot not supported on Linux")
    }
}

#[cfg(target_os = "macos")]
fn take_screenshot_macos() -> Result<Screenshot> {
    type CGImageRef = *mut std::ffi::c_void;
    type CFDataRef = *mut std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayCreateImage(display_id: u32) -> CGImageRef;
        fn CGImageGetWidth(image: CGImageRef) -> usize;
        fn CGImageGetHeight(image: CGImageRef) -> usize;
        fn CGImageGetBytesPerRow(image: CGImageRef) -> usize;
        fn CGImageGetDataProvider(image: CGImageRef) -> *mut std::ffi::c_void;
        fn CGImageRelease(image: CGImageRef);
        fn CGDataProviderCopyData(provider: *mut std::ffi::c_void) -> CFDataRef;
        fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        fn CFDataGetLength(data: CFDataRef) -> isize;
        fn CFRelease(cf: *mut std::ffi::c_void);
    }

    unsafe {
        let display_id = CGMainDisplayID();
        let image = CGDisplayCreateImage(display_id);
        if image.is_null() {
            anyhow::bail!("CGDisplayCreateImage returned null");
        }

        let width = CGImageGetWidth(image) as u32;
        let height = CGImageGetHeight(image) as u32;
        let bytes_per_row = CGImageGetBytesPerRow(image);

        let provider = CGImageGetDataProvider(image);
        if provider.is_null() {
            CGImageRelease(image);
            anyhow::bail!("CGImageGetDataProvider returned null");
        }

        let data = CGDataProviderCopyData(provider);
        if data.is_null() {
            CGImageRelease(image);
            anyhow::bail!("CGDataProviderCopyData returned null");
        }

        let ptr = CFDataGetBytePtr(data);
        let len = CFDataGetLength(data) as usize;
        let raw = std::slice::from_raw_parts(ptr, len);

        // Convert BGRA to RGBA, handling stride (bytes_per_row may include padding)
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let row_start = y * bytes_per_row;
            for x in 0..width as usize {
                let offset = row_start + x * 4;
                let b = raw[offset];
                let g = raw[offset + 1];
                let r = raw[offset + 2];
                let a = raw[offset + 3];
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        }

        CFRelease(data);
        CGImageRelease(image);

        Ok(Screenshot { width, height, rgba })
    }
}

#[cfg(target_os = "windows")]
fn take_screenshot_windows() -> Result<Screenshot> {
    anyhow::bail!("Windows screenshot not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_screenshot_does_not_panic() {
        let _ = take_screenshot();
    }
}
