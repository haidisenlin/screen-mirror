// Metal renderer — displays CVPixelBuffers in a winit window via CAMetalLayer.
//
// Framework linking is declared here; the linker picks them up automatically
// because this module is compiled only on macOS (see mod.rs cfg gate).

#[allow(clippy::duplicated_attributes)]
#[link(name = "Metal", kind = "framework")]
#[link(name = "QuartzCore", kind = "framework")]
#[link(name = "CoreVideo", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLOrigin, MTLPixelFormat, MTLSize, MTLTexture,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

// ── Opaque types ──────────────────────────────────────────────────────────────

type CFAllocatorRef = *mut c_void;
type CFDictionaryRef = *mut c_void;
type CVImageBufferRef = *mut c_void;
type CVMetalTextureCacheRef = *mut c_void;
type CVMetalTextureRef = *mut c_void;
type CVReturn = i32;

// ── CoreVideo / CoreFoundation FFI ────────────────────────────────────────────
//
// We call the C functions directly rather than using the `core-video` crate,
// which depends on the `metal` crate (a different binding layer from objc2-metal).

unsafe extern "C" {
    static kCFAllocatorDefault: CFAllocatorRef;

    // CVMetalTextureCache
    fn CVMetalTextureCacheCreate(
        allocator: CFAllocatorRef,
        cache_attributes: CFDictionaryRef,
        metal_device: *mut c_void, // id<MTLDevice>
        texture_attributes: CFDictionaryRef,
        cache_out: *mut CVMetalTextureCacheRef,
    ) -> CVReturn;

    fn CVMetalTextureCacheCreateTextureFromImage(
        allocator: CFAllocatorRef,
        texture_cache: CVMetalTextureCacheRef,
        source_image: CVImageBufferRef,
        texture_attributes: CFDictionaryRef,
        pixel_format: usize, // MTLPixelFormat (NSUInteger)
        width: usize,
        height: usize,
        plane_index: usize,
        texture_out: *mut CVMetalTextureRef,
    ) -> CVReturn;

    fn CVMetalTextureGetTexture(image: CVMetalTextureRef) -> *mut c_void; // id<MTLTexture>

    fn CFRelease(cf: *mut c_void);

    // CVPixelBuffer geometry
    fn CVPixelBufferGetWidth(pixel_buffer: CVImageBufferRef) -> usize;
    fn CVPixelBufferGetHeight(pixel_buffer: CVImageBufferRef) -> usize;
}

// ── AppKit FFI — attach a CALayer to an NSView ────────────────────────────────
//
// On ARM64, objc_msgSend is NOT variadic — it uses the standard calling
// convention. Declaring it as `...` causes Rust to use the C variadic ABI,
// which passes arguments differently on aarch64 and corrupts pointer args.
// We use typed function pointer casts instead.

unsafe extern "C" {
    fn objc_msgSend(receiver: *mut AnyObject, sel: *const c_void, ...) -> *mut c_void;
    fn sel_registerName(name: *const u8) -> *const c_void;
}

unsafe fn msg_send_set_layer(obj: *mut AnyObject, layer: *mut c_void) {
    let sel = unsafe { sel_registerName(c"setLayer:".as_ptr().cast()) };
    let func: unsafe extern "C" fn(*mut AnyObject, *const c_void, *mut c_void) =
        unsafe { std::mem::transmute(objc_msgSend as *const c_void) };
    unsafe { func(obj, sel, layer) };
}

unsafe fn msg_send_set_wants_layer(obj: *mut AnyObject, val: bool) {
    let sel = unsafe { sel_registerName(c"setWantsLayer:".as_ptr().cast()) };
    let func: unsafe extern "C" fn(*mut AnyObject, *const c_void, i8) =
        unsafe { std::mem::transmute(objc_msgSend as *const c_void) };
    unsafe { func(obj, sel, val as i8) };
}

// ── MetalRenderer ─────────────────────────────────────────────────────────────

/// Renders CVPixelBuffers to a winit window via Metal + CAMetalLayer.
pub struct MetalRenderer {
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    metal_layer: Retained<CAMetalLayer>,
    texture_cache: CVMetalTextureCacheRef,
}

// SAFETY: CAMetalLayer and texture_cache are only accessed from the thread
// that drives the render loop. The caller must ensure single-threaded access.
unsafe impl Send for MetalRenderer {}

impl MetalRenderer {
    /// Create a new renderer attached to the given winit window.
    ///
    /// Must be called from the main thread (AppKit requirement).
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        // ── Get NSView pointer from the winit window handle ───────────────────
        let ns_view: *mut AnyObject = {
            let handle = window
                .window_handle()
                .map_err(|e| anyhow::anyhow!("window_handle error: {e}"))?;
            match handle.as_raw() {
                RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as *mut AnyObject,
                _ => anyhow::bail!("non-AppKit window handle on macOS"),
            }
        };

        // ── Create MTLDevice ──────────────────────────────────────────────────
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| anyhow::anyhow!("MTLCreateSystemDefaultDevice returned nil"))?;

        // ── Create CAMetalLayer and configure it ──────────────────────────────
        let scale_factor = window.scale_factor();
        let inner = window.inner_size();
        let logical_w = inner.width as f64 / scale_factor;
        let logical_h = inner.height as f64 / scale_factor;

        let metal_layer = CAMetalLayer::new();
        metal_layer.setDevice(Some(&device));
        metal_layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        metal_layer.setFramebufferOnly(false);
        metal_layer.setContentsScale(scale_factor);
        // Explicit layer hosting doesn't auto-size the layer.
        metal_layer.setFrame(objc2_core_foundation::CGRect {
            origin: objc2_core_foundation::CGPoint { x: 0.0, y: 0.0 },
            size: objc2_core_foundation::CGSize {
                width: logical_w,
                height: logical_h,
            },
        });

        // Attach layer to the NSView.
        unsafe {
            msg_send_set_wants_layer(ns_view, true);
            let layer_ptr = Retained::as_ptr(&metal_layer) as *mut c_void;
            msg_send_set_layer(ns_view, layer_ptr);
        }

        // ── Create command queue ──────────────────────────────────────────────
        let command_queue = device
            .newCommandQueue()
            .ok_or_else(|| anyhow::anyhow!("newCommandQueue returned nil"))?;

        // ── Create CVMetalTextureCache ────────────────────────────────────────
        let device_ptr = Retained::as_ptr(&device) as *mut c_void;
        let mut texture_cache: CVMetalTextureCacheRef = std::ptr::null_mut();
        let status = unsafe {
            CVMetalTextureCacheCreate(
                kCFAllocatorDefault,
                std::ptr::null_mut(),
                device_ptr,
                std::ptr::null_mut(),
                &mut texture_cache,
            )
        };
        if status != 0 {
            anyhow::bail!("CVMetalTextureCacheCreate failed: {}", status);
        }

        Ok(MetalRenderer {
            _device: device,
            command_queue,
            metal_layer,
            texture_cache,
        })
    }

    /// # Safety
    ///
    /// `pixel_buffer` must be a valid `CVPixelBufferRef` in BGRA8 format.
    pub unsafe fn render_pixel_buffer(&mut self, pixel_buffer: *mut c_void) -> anyhow::Result<()> {
        let width = unsafe { CVPixelBufferGetWidth(pixel_buffer) };
        let height = unsafe { CVPixelBufferGetHeight(pixel_buffer) };
        if width == 0 || height == 0 {
            anyhow::bail!("pixel buffer has zero dimension");
        }

        // Ensure the drawable matches the source video dimensions so the
        // blit fills the entire drawable. CAMetalLayer handles upscaling to
        // fill the view.
        let cur = self.metal_layer.drawableSize();
        if cur.width as usize != width || cur.height as usize != height {
            self.metal_layer
                .setDrawableSize(objc2_core_foundation::CGSize {
                    width: width as f64,
                    height: height as f64,
                });
        }

        // ── Wrap the CVPixelBuffer as an MTLTexture via the texture cache ─────
        let mut cv_texture: CVMetalTextureRef = std::ptr::null_mut();
        let status = unsafe {
            CVMetalTextureCacheCreateTextureFromImage(
                kCFAllocatorDefault,
                self.texture_cache,
                pixel_buffer,
                std::ptr::null_mut(),
                MTLPixelFormat::BGRA8Unorm.0,
                width,
                height,
                0, // plane index
                &mut cv_texture,
            )
        };
        if status != 0 || cv_texture.is_null() {
            anyhow::bail!(
                "CVMetalTextureCacheCreateTextureFromImage failed: {}",
                status
            );
        }

        // Get the underlying id<MTLTexture> from the CVMetalTexture wrapper.
        let src_texture_ptr = unsafe { CVMetalTextureGetTexture(cv_texture) };
        if src_texture_ptr.is_null() {
            unsafe { CFRelease(cv_texture) };
            anyhow::bail!("CVMetalTextureGetTexture returned nil");
        }

        // SAFETY: CVMetalTextureGetTexture returns an id<MTLTexture> which
        // follows objc2's ProtocolObject layout for the MTLTexture protocol.
        let src_texture: &ProtocolObject<dyn MTLTexture> =
            unsafe { &*(src_texture_ptr as *const ProtocolObject<dyn MTLTexture>) };

        // ── Get the next drawable from the CAMetalLayer ───────────────────────
        let drawable = match self.metal_layer.nextDrawable() {
            Some(d) => d,
            None => {
                unsafe { CFRelease(cv_texture) };
                anyhow::bail!("nextDrawable returned nil");
            }
        };

        let dst_texture: Retained<ProtocolObject<dyn MTLTexture>> = drawable.texture();

        // ── Build a command buffer and blit the texture ───────────────────────
        let cmd_buf = self
            .command_queue
            .commandBuffer()
            .ok_or_else(|| anyhow::anyhow!("commandBuffer returned nil"))?;

        let blit = cmd_buf
            .blitCommandEncoder()
            .ok_or_else(|| anyhow::anyhow!("blitCommandEncoder returned nil"))?;

        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let size = MTLSize {
            width,
            height,
            depth: 1,
        };
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                src_texture,
                0,
                0,
                origin,
                size,
                &dst_texture,
                0,
                0,
                origin,
            );
        }
        blit.endEncoding();

        // Present and commit.
        cmd_buf.presentDrawable(ProtocolObject::from_ref(&*drawable));
        cmd_buf.commit();

        // Release the CVMetalTexture wrapper (not the underlying MTLTexture).
        unsafe { CFRelease(cv_texture) };

        Ok(())
    }
}

impl Drop for MetalRenderer {
    fn drop(&mut self) {
        if !self.texture_cache.is_null() {
            unsafe { CFRelease(self.texture_cache) };
            self.texture_cache = std::ptr::null_mut();
        }
    }
}
