use std::sync::mpsc::{self, Receiver, SyncSender};

use super::{EncodedPacket, EncoderConfig};
use crate::capture::CapturedFrame;

#[repr(C)]
pub struct SoftwareFrameData {
    pub y: *const u8,
    pub u: *const u8,
    pub v: *const u8,
    pub stride: u32,
    pub timestamp_ns: u64,
}

pub struct X264Encoder {
    width: u32,
    height: u32,
    tx: SyncSender<EncodedPacket>,
    rx: Receiver<EncodedPacket>,
}

impl X264Encoder {
    pub fn new(config: &EncoderConfig) -> anyhow::Result<Self> {
        // openh264 encoder initialization will go here once we verify
        // the crate works on Windows. For now, this always succeeds as
        // the software fallback — actual encoding uses a simple passthrough
        // that will be fleshed out with real openh264 calls on Windows.
        let (tx, rx) = mpsc::sync_channel(16);

        tracing::info!(
            "x264/software: initialized {}x{} @ {}fps, {}bps",
            config.width, config.height, config.fps, config.bitrate
        );

        Ok(Self {
            width: config.width,
            height: config.height,
            tx,
            rx,
        })
    }

    pub fn encode(&mut self, frame: &CapturedFrame) -> anyhow::Result<()> {
        // The caller (VideoEncoder) provides frame.native pointing to SoftwareFrameData
        // after doing GPU→CPU readback of the NV12 texture.
        let frame_data = unsafe { &*(frame.native as *const SoftwareFrameData) };

        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4; // each U and V plane is quarter size

        let y_plane = unsafe { std::slice::from_raw_parts(frame_data.y, y_size) };
        let _u_plane = unsafe { std::slice::from_raw_parts(frame_data.u, uv_size) };
        let _v_plane = unsafe { std::slice::from_raw_parts(frame_data.v, uv_size) };

        // TODO: Feed YUV planes to openh264 encoder when crate is integrated.
        // For now, emit a placeholder keyframe so the pipeline doesn't stall.
        let _ = y_plane;

        let packet = EncodedPacket {
            data: Vec::new(),
            is_keyframe: true,
            timestamp: frame_data.timestamp_ns,
            nal_units: Vec::new(),
        };

        let _ = self.tx.try_send(packet);
        Ok(())
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        self.rx.try_recv().ok()
    }
}
