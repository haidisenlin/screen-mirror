use std::sync::mpsc::{self, Receiver, SyncSender};

use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Media::MediaFoundation::{
    IMFMediaBuffer, IMFMediaType, IMFSample, IMFTransform, MFCreateDXGISurfaceBuffer,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFStartup, MFTEnumEx,
    MFMediaType_Video, MFVideoFormat_H264, MFVideoFormat_NV12, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_REGISTER_TYPE_INFO,
    MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::core::Interface;

use super::{EncodedPacket, EncoderConfig};

pub struct MfEncoder {
    transform: IMFTransform,
    tx: SyncSender<EncodedPacket>,
    rx: Receiver<EncodedPacket>,
    width: u32,
    height: u32,
}

unsafe impl Send for MfEncoder {}

impl MfEncoder {
    pub fn new(config: &EncoderConfig) -> anyhow::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok();
            MFStartup(0x00020070, 0)?;

            let input_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_NV12,
            };
            let output_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_H264,
            };

            let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;

            let mut activates = std::ptr::null_mut();
            let mut count = 0u32;

            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                flags,
                Some(&input_type),
                Some(&output_type),
                &mut activates,
                &mut count,
            )?;

            if count == 0 {
                anyhow::bail!("no hardware H.264 MFT found");
            }

            let activate_array = std::slice::from_raw_parts(activates, count as usize);
            let activate = activate_array[0]
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("null IMFActivate"))?;

            let transform: IMFTransform = activate.ActivateObject()?;

            // Take ownership of each IMFActivate (triggers Release via Drop), then free the array
            for i in 0..count as usize {
                let _ = std::ptr::read(activates.add(i));
            }
            CoTaskMemFree(Some(activates as *const _ as *const std::ffi::c_void));

            // Configure output type (H.264)
            let output_media_type: IMFMediaType = MFCreateMediaType()?;
            output_media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output_media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            output_media_type.SetUINT64(
                &MF_MT_FRAME_SIZE,
                ((config.width as u64) << 32) | config.height as u64,
            )?;
            output_media_type.SetUINT64(
                &MF_MT_FRAME_RATE,
                ((config.fps as u64) << 32) | 1,
            )?;
            output_media_type.SetUINT32(&MF_MT_AVG_BITRATE, config.bitrate)?;
            output_media_type.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?; // Progressive

            transform.SetOutputType(0, &output_media_type, 0)?;

            // Configure input type (NV12)
            let input_media_type: IMFMediaType = MFCreateMediaType()?;
            input_media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input_media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            input_media_type.SetUINT64(
                &MF_MT_FRAME_SIZE,
                ((config.width as u64) << 32) | config.height as u64,
            )?;
            input_media_type.SetUINT64(
                &MF_MT_FRAME_RATE,
                ((config.fps as u64) << 32) | 1,
            )?;

            transform.SetInputType(0, &input_media_type, 0)?;

            transform.ProcessMessage(
                windows::Win32::Media::MediaFoundation::MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
                0,
            )?;
            transform.ProcessMessage(
                windows::Win32::Media::MediaFoundation::MFT_MESSAGE_NOTIFY_START_OF_STREAM,
                0,
            )?;

            let (tx, rx) = mpsc::sync_channel(16);

            tracing::info!(
                "Media Foundation: HW H.264 encoder initialized {}x{}",
                config.width, config.height
            );

            Ok(Self { transform, tx, rx, width: config.width, height: config.height })
        }
    }

    pub fn encode_nv12(&mut self, nv12_texture: &ID3D11Texture2D, timestamp_ns: u64) -> anyhow::Result<()> {
        unsafe {
            let buffer: IMFMediaBuffer = MFCreateDXGISurfaceBuffer(
                &ID3D11Texture2D::IID,
                nv12_texture,
                0,     // subresource index
                false, // bottom-up (false for top-down)
            )?;

            let sample: IMFSample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timestamp_ns as i64 / 100)?; // 100ns units

            self.transform.ProcessInput(0, &sample, 0)?;
            self.drain_output(timestamp_ns)?;

            Ok(())
        }
    }

    unsafe fn drain_output(&self, timestamp: u64) -> anyhow::Result<()> {
        loop {
            let mut output_buffer =
                windows::Win32::Media::MediaFoundation::MFT_OUTPUT_DATA_BUFFER::default();
            let output_sample: IMFSample = MFCreateSample()?;
            let output_buf: IMFMediaBuffer = MFCreateMemoryBuffer(1024 * 1024)?;
            output_sample.AddBuffer(&output_buf)?;
            output_buffer.pSample = std::mem::ManuallyDrop::new(Some(output_sample));

            let mut status = 0u32;
            let hr = self.transform.ProcessOutput(0, &mut [output_buffer], &mut status);

            if hr.is_err() {
                break;
            }

            if let Some(sample) = std::mem::ManuallyDrop::into_inner(output_buffer.pSample) {
                let buf: IMFMediaBuffer = sample.ConvertToContiguousBuffer()?;
                let mut ptr = std::ptr::null_mut();
                let mut length = 0u32;
                buf.Lock(&mut ptr, None, Some(&mut length))?;

                let data =
                    std::slice::from_raw_parts(ptr as *const u8, length as usize).to_vec();
                buf.Unlock()?;

                let nal_units = parse_annex_b_nals(&data);
                let is_keyframe =
                    nal_units.iter().any(|n| !n.is_empty() && (n[0] & 0x1F) == 5);

                let packet = EncodedPacket {
                    data,
                    is_keyframe,
                    timestamp,
                    nal_units,
                };

                let _ = self.tx.try_send(packet);
            }
        }
        Ok(())
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        self.rx.try_recv().ok()
    }
}

fn parse_annex_b_nals(data: &[u8]) -> Vec<Vec<u8>> {
    let mut nals = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let start = if i + 3 < data.len() && data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                i + 3
            } else if i + 4 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                i + 4
            } else {
                i += 1;
                continue;
            }
        } else {
            i += 1;
            continue;
        };

        let mut end = start;
        while end + 2 < data.len() {
            if data[end] == 0
                && data[end + 1] == 0
                && (data[end + 2] == 1
                    || (end + 3 < data.len() && data[end + 2] == 0 && data[end + 3] == 1))
            {
                break;
            }
            end += 1;
        }
        if end + 2 >= data.len() {
            end = data.len();
        }

        if end > start {
            nals.push(data[start..end].to_vec());
        }
        i = end;
    }

    nals
}
