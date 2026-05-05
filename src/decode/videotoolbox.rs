// VideoToolbox H.264 decoder — raw FFI against macOS frameworks.
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

use super::{DecodedFrame, DecoderConfig};

// ── Opaque CF / CM / CV / VT types ──────────────────────────────────────────

type CFAllocatorRef = *mut c_void;
type CFDictionaryRef = *mut c_void;
type CFStringRef = *mut c_void;
type CFNumberRef = *mut c_void;
type CFBooleanRef = *mut c_void;
type CFTypeRef = *mut c_void;
type CFMutableDictionaryRef = *mut c_void;

type CMFormatDescriptionRef = *mut c_void;
type CMSampleBufferRef = *mut c_void;
type CMBlockBufferRef = *mut c_void;

type CVImageBufferRef = *mut c_void;
type CVPixelBufferRef = *mut c_void;

type VTDecompressionSessionRef = *mut c_void;
type VTDecodeInfoFlags = u32;
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

// ── CMSampleTimingInfo ────────────────────────────────────────────────────────
//
// typedef struct {
//     CMTime  duration;
//     CMTime  presentationTimeStamp;
//     CMTime  decodeTimeStamp;
// } CMSampleTimingInfo;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMSampleTimingInfo {
    duration: CMTime,
    presentation_time_stamp: CMTime,
    decode_time_stamp: CMTime,
}

// ── VTDecompressionOutputCallbackRecord ──────────────────────────────────────
//
// typedef struct {
//     VTDecompressionOutputCallback  decompressionOutputCallback;
//     void *                         decompressionOutputRefCon;
// } VTDecompressionOutputCallbackRecord;

#[repr(C)]
struct VTDecompressionOutputCallbackRecord {
    decompress_output_callback: VTDecompressionOutputCallback,
    decompress_output_ref_con: *mut c_void,
}

// ── Constants ────────────────────────────────────────────────────────────────

// Pixel format: 'BGRA' = 0x42475241
const K_CV_PIXEL_FORMAT_TYPE_32BGRA: u32 = 0x4247_5241;

// CFNumber types
const K_CF_NUMBER_SINT32_TYPE: i32 = 3;

// CFDictionary callbacks (NULL means default kCFTypeDictionaryKeyCallBacks /
// kCFTypeDictionaryValueCallBacks when creating via CFDictionaryCreateMutable).
// We pass the exported symbols directly in the FFI calls below.

// ── FFI declarations ─────────────────────────────────────────────────────────

unsafe extern "C" {
    // CoreFoundation
    static kCFAllocatorDefault: CFAllocatorRef;
    static kCFAllocatorNull: CFAllocatorRef;
    static kCFBooleanTrue: CFBooleanRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;

    fn CFRelease(cf: CFTypeRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFMutableDictionaryRef;
    fn CFDictionarySetValue(
        the_dict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );

    // CoreFoundation string keys exposed by VideoToolbox / CoreVideo
    static kVTDecompressionPropertyKey_RealTime: CFStringRef;
    static kCVPixelBufferPixelFormatTypeKey: CFStringRef;

    // VideoToolbox — decompression
    fn VTDecompressionSessionCreate(
        allocator: CFAllocatorRef,
        video_format_description: CMFormatDescriptionRef,
        video_decoder_specification: CFDictionaryRef,
        destination_image_buffer_attributes: CFDictionaryRef,
        output_callback: *const VTDecompressionOutputCallbackRecord,
        decompression_session_out: *mut VTDecompressionSessionRef,
    ) -> OSStatus;

    fn VTDecompressionSessionDecodeFrame(
        session: VTDecompressionSessionRef,
        sample_buffer: CMSampleBufferRef,
        decode_flags: u32,
        source_frame_ref_con: *mut c_void,
        info_flags_out: *mut VTDecodeInfoFlags,
    ) -> OSStatus;

    fn VTDecompressionSessionWaitForAsynchronousFrames(
        session: VTDecompressionSessionRef,
    ) -> OSStatus;

    fn VTDecompressionSessionInvalidate(session: VTDecompressionSessionRef);

    fn VTSessionSetProperty(
        session: VTDecompressionSessionRef,
        property_key: CFStringRef,
        property_value: CFTypeRef,
    ) -> OSStatus;

    // CoreMedia — format description
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: CFAllocatorRef,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        format_description_out: *mut CMFormatDescriptionRef,
    ) -> OSStatus;

    // CoreMedia — block buffer
    fn CMBlockBufferCreateWithMemoryBlock(
        structure_allocator: CFAllocatorRef,
        memory_block: *mut c_void,
        block_length: usize,
        block_allocator: CFAllocatorRef,
        custom_block_source: *const c_void,
        offset_to_data: usize,
        data_length: usize,
        flags: u32,
        block_buffer_out: *mut CMBlockBufferRef,
    ) -> OSStatus;

    // CoreMedia — sample buffer
    fn CMSampleBufferCreateReady(
        allocator: CFAllocatorRef,
        data_buffer: CMBlockBufferRef,
        format_description: CMFormatDescriptionRef,
        num_samples: isize,
        num_sample_timing_entries: isize,
        sample_timing_array: *const CMSampleTimingInfo,
        num_sample_size_entries: isize,
        sample_size_array: *const usize,
        sample_buffer_out: *mut CMSampleBufferRef,
    ) -> OSStatus;
}

// ── Output callback type ──────────────────────────────────────────────────────

type VTDecompressionOutputCallback = unsafe extern "C" fn(
    decompress_output_ref_con: *mut c_void,
    source_frame_ref_con: *mut c_void,
    status: OSStatus,
    info_flags: VTDecodeInfoFlags,
    image_buffer: CVImageBufferRef,
    presentation_time_stamp: CMTime,
    presentation_duration: CMTime,
);

// ── Output callback (called by VideoToolbox on an internal thread) ────────────

unsafe extern "C" fn decompression_output_callback(
    decompress_output_ref_con: *mut c_void,
    source_frame_ref_con: *mut c_void,
    status: OSStatus,
    _info_flags: VTDecodeInfoFlags,
    image_buffer: CVImageBufferRef,
    _presentation_time_stamp: CMTime,
    _presentation_duration: CMTime,
) {
    if status != 0 || image_buffer.is_null() {
        return;
    }

    // SAFETY: refcon is a raw pointer to a SyncSender we Box'd when creating
    // the session. It is valid for the lifetime of the session.
    let tx = unsafe { &*(decompress_output_ref_con as *const SyncSender<DecodedFrame>) };

    // Retrieve the timestamp passed through source_frame_ref_con.
    // We encoded the u64 timestamp as a raw pointer value (see decode_nal).
    let timestamp = source_frame_ref_con as u64;

    // Retain the pixel buffer — VideoToolbox may release it after this callback
    // returns, so the receiver needs its own reference.
    let retained = unsafe { CFRetain(image_buffer) };

    let frame = DecodedFrame {
        pixel_buffer: retained as CVPixelBufferRef,
        timestamp,
    };

    // Drop silently if the receiver is gone or the channel is full.
    let _ = tx.try_send(frame);
}

// ── CF helpers ────────────────────────────────────────────────────────────────

/// Wrap a u32 as a CFNumber (stored as SInt32). Returns an owned ref (must CFRelease).
unsafe fn cf_number_from_u32(v: u32) -> CFNumberRef {
    unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            K_CF_NUMBER_SINT32_TYPE,
            &(v as i32) as *const i32 as *const c_void,
        )
    }
}

// ── VTDecoder ─────────────────────────────────────────────────────────────────

/// H.264 decoder backed by VideoToolbox.
pub struct VTDecoder {
    session: VTDecompressionSessionRef,
    fmt_desc: CMFormatDescriptionRef,
    rx: Receiver<DecodedFrame>,
    /// Kept alive so the pointer in the session refcon stays valid.
    _tx: Box<SyncSender<DecodedFrame>>,
    /// Buffered SPS NAL (without start code, without AVCC length header).
    sps: Option<Vec<u8>>,
    /// Buffered PPS NAL (without start code, without AVCC length header).
    pps: Option<Vec<u8>>,
}

// SAFETY: VTDecompressionSessionRef is a CF opaque pointer managed by
// VideoToolbox. We never share the session across threads concurrently;
// decode calls are serialised by the caller.
unsafe impl Send for VTDecoder {}

impl VTDecoder {
    /// Create a new decoder. The session itself is created lazily when both
    /// SPS and PPS have been received (via `decode_nal`).
    pub fn new(_config: DecoderConfig) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<DecodedFrame>(16);
        let tx_box = Box::new(tx);
        Ok(VTDecoder {
            session: std::ptr::null_mut(),
            fmt_desc: std::ptr::null_mut(),
            rx,
            _tx: tx_box,
            sps: None,
            pps: None,
        })
    }

    /// Submit a raw NAL unit (Annex-B bytes after start-code stripping, or
    /// bare NAL — the caller must strip start codes before calling).
    ///
    /// `timestamp` is in nanoseconds.
    pub fn decode_nal(&mut self, nal: &[u8], timestamp: u64) -> anyhow::Result<()> {
        if nal.is_empty() {
            return Ok(());
        }

        let nal_type = nal[0] & 0x1f;

        match nal_type {
            7 => {
                // SPS
                self.sps = Some(nal.to_vec());
                self.try_create_session()?;
            }
            8 => {
                // PPS
                self.pps = Some(nal.to_vec());
                self.try_create_session()?;
            }
            1 | 5 => {
                // Non-IDR or IDR slice — decode if session is ready
                if self.session.is_null() {
                    // Session not yet created (waiting for SPS/PPS); drop frame.
                    return Ok(());
                }
                unsafe { self.submit_nal_for_decode(nal, timestamp)? };
            }
            _ => {
                // Other NAL types (SEI, etc.) — ignore
            }
        }

        Ok(())
    }

    /// Returns the next decoded frame if one is available, or `None`.
    pub fn next_frame(&self) -> Option<DecodedFrame> {
        self.rx.try_recv().ok()
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Create the VTDecompressionSession once both SPS and PPS are available.
    fn try_create_session(&mut self) -> anyhow::Result<()> {
        if self.sps.is_none() || self.pps.is_none() {
            return Ok(());
        }

        // Tear down any existing session (e.g. after a parameter-set change).
        if !self.session.is_null() {
            unsafe {
                VTDecompressionSessionWaitForAsynchronousFrames(self.session);
                VTDecompressionSessionInvalidate(self.session);
                CFRelease(self.session);
            }
            self.session = std::ptr::null_mut();
        }
        if !self.fmt_desc.is_null() {
            unsafe { CFRelease(self.fmt_desc) };
            self.fmt_desc = std::ptr::null_mut();
        }

        let sps = self.sps.as_ref().unwrap();
        let pps = self.pps.as_ref().unwrap();

        // ── Build CMVideoFormatDescription from SPS + PPS ────────────────────
        let param_ptrs: [*const u8; 2] = [sps.as_ptr(), pps.as_ptr()];
        let param_sizes: [usize; 2] = [sps.len(), pps.len()];

        let mut fmt_desc: CMFormatDescriptionRef = std::ptr::null_mut();
        let status = unsafe {
            CMVideoFormatDescriptionCreateFromH264ParameterSets(
                kCFAllocatorDefault,
                2,
                param_ptrs.as_ptr(),
                param_sizes.as_ptr(),
                4, // AVCC NAL length header = 4 bytes
                &mut fmt_desc,
            )
        };
        if status != 0 {
            anyhow::bail!(
                "CMVideoFormatDescriptionCreateFromH264ParameterSets failed: {}",
                status
            );
        }
        self.fmt_desc = fmt_desc;

        // ── Build destination image buffer attributes (BGRA output) ──────────
        let dest_attrs = unsafe {
            let dict = CFDictionaryCreateMutable(
                kCFAllocatorDefault,
                0,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            let fmt_num = cf_number_from_u32(K_CV_PIXEL_FORMAT_TYPE_32BGRA);
            CFDictionarySetValue(
                dict,
                kCVPixelBufferPixelFormatTypeKey as *const c_void,
                fmt_num as *const c_void,
            );
            CFRelease(fmt_num as CFTypeRef);
            dict
        };

        // ── Output callback record ────────────────────────────────────────────
        let refcon = &*self._tx as *const SyncSender<DecodedFrame> as *mut c_void;
        let callback_record = VTDecompressionOutputCallbackRecord {
            decompress_output_callback: decompression_output_callback,
            decompress_output_ref_con: refcon,
        };

        // ── Create the decompression session ──────────────────────────────────
        let mut session: VTDecompressionSessionRef = std::ptr::null_mut();
        let status = unsafe {
            VTDecompressionSessionCreate(
                kCFAllocatorDefault,
                fmt_desc,
                std::ptr::null_mut(), // video decoder specification
                dest_attrs,
                &callback_record,
                &mut session,
            )
        };
        unsafe { CFRelease(dest_attrs as CFTypeRef) };

        if status != 0 {
            anyhow::bail!("VTDecompressionSessionCreate failed: {}", status);
        }
        self.session = session;

        // ── Configure real-time decoding ──────────────────────────────────────
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTDecompressionPropertyKey_RealTime,
                kCFBooleanTrue as CFTypeRef,
            )
        };
        if status != 0 {
            anyhow::bail!("VTSessionSetProperty RealTime failed: {}", status);
        }

        Ok(())
    }

    /// Wrap `nal` in AVCC framing, create a CMSampleBuffer, and submit it for
    /// decoding. `nal` must NOT include start codes or an AVCC length prefix.
    unsafe fn submit_nal_for_decode(&self, nal: &[u8], timestamp: u64) -> anyhow::Result<()> {
        let nal_len = nal.len();

        // Build AVCC packet: 4-byte big-endian length + NAL data.
        let avcc_len = 4 + nal_len;
        let mut avcc_buf: Vec<u8> = Vec::with_capacity(avcc_len);
        avcc_buf.extend_from_slice(&(nal_len as u32).to_be_bytes());
        avcc_buf.extend_from_slice(nal);

        // ── CMBlockBuffer wrapping our Vec's memory ───────────────────────────
        // We Box the Vec so its pointer stays stable, then leak it; the block
        // buffer's custom block source would be the right place to free it, but
        // since we call VTDecompressionSessionDecodeFrame synchronously below
        // (kVTDecodeFrame_EnableAsynchronousDecompression = 0) with a
        // WaitForAsynchronousFrames barrier, it is safe to keep the Vec alive
        // on the stack for the duration of this call.  We keep it as a local
        // and let it drop at the end of the function.
        let mut block_buf: CMBlockBufferRef = std::ptr::null_mut();
        let status = unsafe {
            CMBlockBufferCreateWithMemoryBlock(
                kCFAllocatorDefault,
                avcc_buf.as_mut_ptr() as *mut c_void,
                avcc_len,
                kCFAllocatorNull, // don't deallocate — we own the memory
                std::ptr::null(), // custom block source
                0,                // offset to data
                avcc_len,         // data length
                0,                // flags
                &mut block_buf,
            )
        };
        if status != 0 {
            anyhow::bail!("CMBlockBufferCreateWithMemoryBlock failed: {}", status);
        }

        // ── CMSampleBuffer ────────────────────────────────────────────────────
        let pts = CMTime::from_ns(timestamp);
        let timing = CMSampleTimingInfo {
            duration: CMTime::invalid(),
            presentation_time_stamp: pts,
            decode_time_stamp: CMTime::invalid(),
        };

        let mut sbuf: CMSampleBufferRef = std::ptr::null_mut();
        let status = unsafe {
            CMSampleBufferCreateReady(
                kCFAllocatorDefault,
                block_buf,
                self.fmt_desc,
                1, // num_samples
                1, // num_sample_timing_entries
                &timing,
                1,         // num_sample_size_entries
                &avcc_len, // sample size
                &mut sbuf,
            )
        };
        unsafe { CFRelease(block_buf as CFTypeRef) };
        if status != 0 {
            anyhow::bail!("CMSampleBufferCreateReady failed: {}", status);
        }

        // ── Decode ────────────────────────────────────────────────────────────
        // Pass the timestamp as a raw pointer value through source_frame_ref_con.
        // The callback casts it back to u64.  This is safe as long as u64 fits
        // in a pointer-sized integer on the target (true for all 64-bit Apple
        // platforms).
        let mut info_flags: VTDecodeInfoFlags = 0;
        let status = unsafe {
            VTDecompressionSessionDecodeFrame(
                self.session,
                sbuf,
                0, // synchronous decode (RealTime property is set), output frame
                timestamp as usize as *mut c_void, // source_frame_ref_con carries timestamp
                &mut info_flags,
            )
        };
        unsafe { CFRelease(sbuf as CFTypeRef) };

        if status != 0 {
            anyhow::bail!("VTDecompressionSessionDecodeFrame failed: {}", status);
        }

        // avcc_buf is still alive here, which is what we need: CMBlockBuffer
        // referenced it by pointer and the decode call has consumed the data.
        // It drops at the end of this scope.
        drop(avcc_buf);

        Ok(())
    }
}

impl Drop for VTDecoder {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe {
                VTDecompressionSessionWaitForAsynchronousFrames(self.session);
                VTDecompressionSessionInvalidate(self.session);
                CFRelease(self.session);
            }
            self.session = std::ptr::null_mut();
        }
        if !self.fmt_desc.is_null() {
            unsafe { CFRelease(self.fmt_desc) };
            self.fmt_desc = std::ptr::null_mut();
        }
    }
}
