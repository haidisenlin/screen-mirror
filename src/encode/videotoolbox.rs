// VideoToolbox H.264 encoder — raw FFI against macOS frameworks.
//
// Framework linking is declared here; the linker picks them up automatically
// because this module is compiled only on macOS (see mod.rs cfg gate).

#[allow(clippy::duplicated_attributes)]
#[link(name = "VideoToolbox", kind = "framework")]
#[link(name = "CoreMedia", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {}

use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, SyncSender};

use super::{EncodedPacket, EncoderConfig};

// ── Opaque CF / CM / CV / VT types ──────────────────────────────────────────

type CFAllocatorRef = *mut c_void;
type CFDictionaryRef = *mut c_void;
type CFStringRef = *mut c_void;
type CFNumberRef = *mut c_void;
type CFBooleanRef = *mut c_void;
type CFTypeRef = *mut c_void;

type CMFormatDescriptionRef = *mut c_void;
type CMSampleBufferRef = *mut c_void;
type CMBlockBufferRef = *mut c_void;

type CVImageBufferRef = *mut c_void;

type VTCompressionSessionRef = *mut c_void;
type VTEncodeInfoFlags = u32;
type OSStatus = i32;

// ── CMTime — repr(C) matching the ABI layout ─────────────────────────────────
//
// typedef struct {
//     CMTimeValue  value;      // int64_t
//     CMTimeScale  timescale;  // int32_t
//     CMTimeFlags  flags;      // uint32_t
//     CMTimeEpoch  epoch;      // int64_t
// } CMTime;
// Total: 8 + 4 + 4 + 8 = 24 bytes.

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

impl CMTime {
    fn from_ns(ns: u64) -> Self {
        CMTime {
            value: ns as i64,
            timescale: 1_000_000_000i32, // nanosecond resolution
            flags: 1,                    // kCMTimeFlags_Valid
            epoch: 0,
        }
    }

    fn invalid() -> Self {
        CMTime {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        }
    }
}

// ── Constants ────────────────────────────────────────────────────────────────

const K_CM_VIDEO_CODEC_TYPE_H264: u32 = 0x6176_6331; // 'avc1'

// CFNumber types
const K_CF_NUMBER_SINT32_TYPE: i32 = 3;

// ── FFI declarations ─────────────────────────────────────────────────────────

unsafe extern "C" {
    // CoreFoundation
    static kCFAllocatorDefault: CFAllocatorRef;
    static kCFBooleanTrue: CFBooleanRef;
    static kCFBooleanFalse: CFBooleanRef;

    #[allow(dead_code)]
    fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    fn CFBooleanGetValue(boolean: CFBooleanRef) -> bool;

    // CoreFoundation string keys exposed by VideoToolbox / CoreMedia
    static kVTCompressionPropertyKey_RealTime: CFStringRef;
    static kVTCompressionPropertyKey_ProfileLevel: CFStringRef;
    static kVTCompressionPropertyKey_AverageBitRate: CFStringRef;
    static kVTCompressionPropertyKey_MaxKeyFrameInterval: CFStringRef;
    static kVTCompressionPropertyKey_AllowFrameReordering: CFStringRef;
    static kVTCompressionPropertyKey_ExpectedFrameRate: CFStringRef;
    static kVTProfileLevel_H264_High_AutoLevel: CFStringRef;

    static kCMSampleAttachmentKey_NotSync: CFStringRef;

    // VideoToolbox
    fn VTCompressionSessionCreate(
        allocator: CFAllocatorRef,
        width: i32,
        height: i32,
        codec_type: u32,
        encoder_specification: CFDictionaryRef,
        source_image_buffer_attributes: CFDictionaryRef,
        compressed_data_allocator: CFAllocatorRef,
        output_callback: VTCompressionOutputCallback,
        output_callback_ref_con: *mut c_void,
        compression_session_out: *mut VTCompressionSessionRef,
    ) -> OSStatus;

    fn VTCompressionSessionEncodeFrame(
        session: VTCompressionSessionRef,
        image_buffer: CVImageBufferRef,
        presentation_time_stamp: CMTime,
        duration: CMTime,
        frame_properties: CFDictionaryRef,
        source_frame_ref_con: *mut c_void,
        info_flags_out: *mut VTEncodeInfoFlags,
    ) -> OSStatus;

    fn VTSessionSetProperty(
        session: VTCompressionSessionRef,
        property_key: CFStringRef,
        property_value: CFTypeRef,
    ) -> OSStatus;

    fn VTCompressionSessionCompleteFrames(
        session: VTCompressionSessionRef,
        complete_until_presentation_time_stamp: CMTime,
    ) -> OSStatus;

    fn VTCompressionSessionInvalidate(session: VTCompressionSessionRef);

    // CoreMedia — sample buffer
    fn CMSampleBufferGetDataBuffer(sbuf: CMSampleBufferRef) -> CMBlockBufferRef;
    fn CMSampleBufferGetFormatDescription(sbuf: CMSampleBufferRef) -> CMFormatDescriptionRef;
    fn CMSampleBufferGetSampleAttachmentsArray(
        sbuf: CMSampleBufferRef,
        create_if_necessary: bool,
    ) -> *mut c_void; // CFArrayRef

    // CoreMedia — block buffer
    fn CMBlockBufferGetDataPointer(
        the_buffer: CMBlockBufferRef,
        offset: usize,
        length_at_offset_out: *mut usize,
        total_length_out: *mut usize,
        data_pointer_out: *mut *mut u8,
    ) -> OSStatus;

    fn CMBlockBufferGetDataLength(the_buffer: CMBlockBufferRef) -> usize;

    // CoreMedia — format description (H.264 parameter sets)
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        video_desc: CMFormatDescriptionRef,
        parameter_set_index: usize,
        parameter_set_pointer_out: *mut *const u8,
        parameter_set_size_out: *mut usize,
        parameter_set_count_out: *mut usize,
        nal_unit_header_length_out: *mut i32,
    ) -> OSStatus;

    // CoreFoundation array (for attachments)
    fn CFArrayGetCount(array: *mut c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *mut c_void, idx: isize) -> CFTypeRef;
}

// ── Output callback type ──────────────────────────────────────────────────────

type VTCompressionOutputCallback = unsafe extern "C" fn(
    output_callback_ref_con: *mut c_void,
    source_frame_ref_con: *mut c_void,
    status: OSStatus,
    info_flags: VTEncodeInfoFlags,
    sample_buffer: CMSampleBufferRef,
);

// ── Safe wrappers around CF helpers ──────────────────────────────────────────

/// Wrap an i32 as a CFNumber. Returns an owned ref (must be CFRelease'd).
unsafe fn cf_number_from_i32(v: i32) -> CFNumberRef {
    unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            K_CF_NUMBER_SINT32_TYPE,
            &v as *const i32 as *const c_void,
        )
    }
}

// ── NAL unit extraction ───────────────────────────────────────────────────────

/// Read parameter sets (SPS + PPS) from the format description.
///
/// Returns a list of raw NAL byte sequences (without start codes).
unsafe fn extract_parameter_sets(fmt: CMFormatDescriptionRef) -> (Vec<Vec<u8>>, i32) {
    let mut out = Vec::new();
    let mut count: usize = 0;
    let mut nal_len: i32 = 4;

    let status = unsafe {
        CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            fmt,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut count,
            &mut nal_len,
        )
    };
    if status != 0 || count == 0 {
        return (out, nal_len);
    }

    for i in 0..count {
        let mut ptr: *const u8 = std::ptr::null();
        let mut size: usize = 0;
        let status = unsafe {
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                fmt,
                i,
                &mut ptr,
                &mut size,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status == 0 && !ptr.is_null() && size > 0 {
            out.push(unsafe { std::slice::from_raw_parts(ptr, size).to_vec() });
        }
    }
    (out, nal_len)
}

/// Parse AVCC-style block buffer into individual NAL units.
///
/// Each NAL unit is prefixed by a big-endian length field of `nal_header_len` bytes.
fn parse_avcc_nal_units(data: &[u8], nal_header_len: usize) -> Vec<Vec<u8>> {
    let mut nals = Vec::new();
    let mut offset = 0usize;

    while offset + nal_header_len <= data.len() {
        let mut len: usize = 0;
        for i in 0..nal_header_len {
            len = (len << 8) | (data[offset + i] as usize);
        }
        offset += nal_header_len;

        if offset + len > data.len() {
            break;
        }
        nals.push(data[offset..offset + len].to_vec());
        offset += len;
    }
    nals
}

// ── Output callback (called by VideoToolbox on an internal thread) ────────────

unsafe extern "C" fn compression_output_callback(
    output_callback_ref_con: *mut c_void,
    source_frame_ref_con: *mut c_void,
    status: OSStatus,
    _info_flags: VTEncodeInfoFlags,
    sample_buffer: CMSampleBufferRef,
) {
    if status != 0 || sample_buffer.is_null() {
        return;
    }

    let tx = unsafe { &*(output_callback_ref_con as *const SyncSender<EncodedPacket>) };
    let timestamp = source_frame_ref_con as u64;

    if let Some(mut p) = unsafe { extract_encoded_packet(sample_buffer) } {
        p.timestamp = timestamp;
        let _ = tx.try_send(p);
    }
}

/// Extract an `EncodedPacket` from a CMSampleBuffer produced by VideoToolbox.
unsafe fn extract_encoded_packet(sbuf: CMSampleBufferRef) -> Option<EncodedPacket> {
    // ── Is it a keyframe? ────────────────────────────────────────────────────
    // kCMSampleAttachmentKey_NotSync: present+true means NOT a sync frame.
    let is_keyframe = unsafe {
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sbuf, false);
        if attachments.is_null() || CFArrayGetCount(attachments) == 0 {
            true // no attachments → sync frame (keyframe)
        } else {
            let dict = CFArrayGetValueAtIndex(attachments, 0) as CFDictionaryRef;
            let not_sync =
                CFDictionaryGetValue(dict, kCMSampleAttachmentKey_NotSync as *const c_void);
            if not_sync.is_null() {
                true // key absent → sync frame
            } else {
                // Value is kCFBooleanFalse when it IS a sync frame (NotSync=false).
                !CFBooleanGetValue(not_sync as CFBooleanRef)
            }
        }
    };

    // ── Format description → parameter sets (SPS/PPS) on keyframes ──────────
    let mut nal_units: Vec<Vec<u8>> = Vec::new();
    let mut nal_header_len: i32 = 4;

    let fmt = unsafe { CMSampleBufferGetFormatDescription(sbuf) };
    if !fmt.is_null() {
        let (param_sets, hdr_len) = unsafe { extract_parameter_sets(fmt) };
        nal_header_len = hdr_len;
        if is_keyframe {
            nal_units.extend(param_sets);
        }
    }

    // ── Block buffer → encoded bytes ─────────────────────────────────────────
    let block_buf = unsafe { CMSampleBufferGetDataBuffer(sbuf) };
    if block_buf.is_null() {
        return None;
    }

    let total_len = unsafe { CMBlockBufferGetDataLength(block_buf) };
    if total_len == 0 {
        return None;
    }

    let mut data_ptr: *mut u8 = std::ptr::null_mut();
    let status = unsafe {
        CMBlockBufferGetDataPointer(
            block_buf,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut data_ptr,
        )
    };
    if status != 0 || data_ptr.is_null() {
        return None;
    }

    let raw = unsafe { std::slice::from_raw_parts(data_ptr, total_len) };

    let slice_nals = parse_avcc_nal_units(raw, nal_header_len as usize);
    nal_units.extend(slice_nals);

    Some(EncodedPacket {
        data: raw.to_vec(),
        is_keyframe,
        timestamp: 0, // TODO: propagate pts from source_frame_ref_con if needed
        nal_units,
    })
}

// ── VTEncoder ────────────────────────────────────────────────────────────────

/// H.264 encoder backed by VideoToolbox.
pub struct VTEncoder {
    session: VTCompressionSessionRef,
    rx: Receiver<EncodedPacket>,
    /// Kept alive so the pointer in the session refcon stays valid.
    _tx: Box<SyncSender<EncodedPacket>>,
    fps: u32,
}

// SAFETY: VTCompressionSessionRef is a CF opaque pointer managed by
// VideoToolbox. We never share the session across threads concurrently;
// encode calls are serialised by the caller.
unsafe impl Send for VTEncoder {}

impl VTEncoder {
    /// Create a new encoder for the given configuration.
    pub fn new(config: &EncoderConfig) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<EncodedPacket>(16);
        let tx_box = Box::new(tx);
        let refcon = &*tx_box as *const SyncSender<EncodedPacket> as *mut c_void;

        let mut session: VTCompressionSessionRef = std::ptr::null_mut();

        let status = unsafe {
            VTCompressionSessionCreate(
                kCFAllocatorDefault,
                config.width as i32,
                config.height as i32,
                K_CM_VIDEO_CODEC_TYPE_H264,
                std::ptr::null_mut(), // encoder specification
                std::ptr::null_mut(), // source image buffer attributes
                std::ptr::null_mut(), // compressed data allocator
                compression_output_callback,
                refcon,
                &mut session,
            )
        };

        if status != 0 {
            anyhow::bail!("VTCompressionSessionCreate failed: {}", status);
        }

        let encoder = VTEncoder {
            session,
            rx,
            _tx: tx_box,
            fps: config.fps,
        };

        unsafe { encoder.configure(config)? };

        Ok(encoder)
    }

    unsafe fn configure(&self, config: &EncoderConfig) -> anyhow::Result<()> {
        // Real-time encoding
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_RealTime,
                kCFBooleanTrue as CFTypeRef,
            )
        };
        if status != 0 {
            anyhow::bail!("set RealTime failed: {}", status);
        }

        // H.264 High Profile (better compression at high resolutions)
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_ProfileLevel,
                kVTProfileLevel_H264_High_AutoLevel as CFTypeRef,
            )
        };
        if status != 0 {
            anyhow::bail!("set ProfileLevel failed: {}", status);
        }

        // Expected frame rate hint (non-fatal if unsupported)
        let fps_num = unsafe { cf_number_from_i32(config.fps as i32) };
        let _ = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_ExpectedFrameRate,
                fps_num as CFTypeRef,
            )
        };
        unsafe { CFRelease(fps_num as CFTypeRef) };

        // CBR average bitrate (bits per second)
        let bitrate = config.bitrate as i32;
        let br_num = unsafe { cf_number_from_i32(bitrate) };
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_AverageBitRate,
                br_num as CFTypeRef,
            )
        };
        unsafe { CFRelease(br_num as CFTypeRef) };
        if status != 0 {
            anyhow::bail!("set AverageBitRate failed: {}", status);
        }

        // Max keyframe interval = fps * 2 (2-second GOP)
        let kf_interval = (config.fps * 2) as i32;
        let kf_num = unsafe { cf_number_from_i32(kf_interval) };
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_MaxKeyFrameInterval,
                kf_num as CFTypeRef,
            )
        };
        unsafe { CFRelease(kf_num as CFTypeRef) };
        if status != 0 {
            anyhow::bail!("set MaxKeyFrameInterval failed: {}", status);
        }

        // Disable frame reordering (B-frames) for low-latency streaming
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_AllowFrameReordering,
                kCFBooleanFalse as CFTypeRef,
            )
        };
        if status != 0 {
            anyhow::bail!("set AllowFrameReordering failed: {}", status);
        }

        Ok(())
    }

    /// Submit a CVPixelBuffer for encoding.
    ///
    /// `pixel_buffer` must be a valid `CVPixelBufferRef` cast to `*mut c_void`.
    /// `timestamp` is in nanoseconds.
    pub fn encode_pixel_buffer(
        &mut self,
        pixel_buffer: *mut c_void,
        timestamp: u64,
    ) -> anyhow::Result<()> {
        let pts = CMTime::from_ns(timestamp);
        let frame_ns = 1_000_000_000u64 / self.fps as u64;
        let dur = CMTime::from_ns(frame_ns);
        let mut flags: VTEncodeInfoFlags = 0;

        let status = unsafe {
            VTCompressionSessionEncodeFrame(
                self.session,
                pixel_buffer as CVImageBufferRef,
                pts,
                dur,
                std::ptr::null_mut(),              // frame properties
                timestamp as usize as *mut c_void, // pass timestamp through refcon
                &mut flags,
            )
        };

        if status != 0 {
            anyhow::bail!("VTCompressionSessionEncodeFrame failed: {}", status);
        }

        Ok(())
    }

    /// Drain any pending encoded packets from the output channel.
    ///
    /// Returns `None` when no packet is immediately available.
    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        self.rx.try_recv().ok()
    }

    /// Encode a CapturedFrame (cross-platform API).
    pub fn encode_frame(&mut self, frame: &crate::capture::CapturedFrame) -> anyhow::Result<()> {
        self.encode_pixel_buffer(frame.native, frame.timestamp_ns)
    }
}

impl Drop for VTEncoder {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe {
                // Flush remaining frames.
                let _ = VTCompressionSessionCompleteFrames(self.session, CMTime::invalid());
                VTCompressionSessionInvalidate(self.session);
                CFRelease(self.session);
            }
            self.session = std::ptr::null_mut();
        }
    }
}
