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

        let rgba = bgra_to_rgba(raw, width, height, bytes_per_row);

        CFRelease(data);
        CGImageRelease(image);

        Ok(Screenshot {
            width,
            height,
            rgba,
        })
    }
}

pub(crate) fn bgra_to_rgba(bgra: &[u8], width: u32, height: u32, bytes_per_row: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        let row_start = y * bytes_per_row;
        for x in 0..width as usize {
            let offset = row_start + x * 4;
            let b = bgra[offset];
            let g = bgra[offset + 1];
            let r = bgra[offset + 2];
            let a = bgra[offset + 3];
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }
    rgba
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

    #[test]
    fn bgra_to_rgba_single_pixel() {
        // BGRA: B=10, G=20, R=30, A=255
        let bgra = vec![10, 20, 30, 255];
        let rgba = bgra_to_rgba(&bgra, 1, 1, 4);
        assert_eq!(rgba, vec![30, 20, 10, 255]);
    }

    #[test]
    fn bgra_to_rgba_with_stride_padding() {
        // 2x1 image with bytes_per_row=16 (8 bytes of pixels + 8 bytes padding)
        let mut bgra = vec![0u8; 16];
        // Pixel (0,0): B=1, G=2, R=3, A=4
        bgra[0] = 1;
        bgra[1] = 2;
        bgra[2] = 3;
        bgra[3] = 4;
        // Pixel (1,0): B=5, G=6, R=7, A=8
        bgra[4] = 5;
        bgra[5] = 6;
        bgra[6] = 7;
        bgra[7] = 8;
        // Bytes 8..15 are stride padding

        let rgba = bgra_to_rgba(&bgra, 2, 1, 16);
        assert_eq!(rgba, vec![3, 2, 1, 4, 7, 6, 5, 8]);
    }

    #[test]
    fn bgra_to_rgba_multi_row() {
        // 2x2 image, no padding (bytes_per_row = 8)
        let bgra = vec![
            // Row 0: pixel(0,0) BGRA, pixel(1,0) BGRA
            100, 150, 200, 255, 50, 60, 70, 128, // Row 1: pixel(0,1) BGRA, pixel(1,1) BGRA
            10, 20, 30, 255, 0, 0, 0, 0,
        ];
        let rgba = bgra_to_rgba(&bgra, 2, 2, 8);
        assert_eq!(
            rgba,
            vec![
                200, 150, 100, 255, 70, 60, 50, 128, // Row 0 in RGBA
                30, 20, 10, 255, 0, 0, 0, 0, // Row 1 in RGBA
            ]
        );
    }
}
