// src/encode/mod.rs

#[cfg(target_os = "macos")]
pub mod videotoolbox;
#[cfg(target_os = "windows")]
pub mod nvenc;
#[cfg(target_os = "windows")]
pub mod media_foundation;
#[cfg(target_os = "windows")]
pub mod software;
#[cfg(target_os = "windows")]
pub mod convert;

use crate::capture::CapturedFrame;

pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}

pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub timestamp: u64,
    pub nal_units: Vec<Vec<u8>>,
}

pub struct VideoEncoder {
    #[cfg(target_os = "macos")]
    inner: videotoolbox::VTEncoder,
    #[cfg(target_os = "windows")]
    inner: WindowsEncoder,
}

#[cfg(target_os = "windows")]
enum WindowsEncoder {
    Nvenc(nvenc::NvencEncoder),
    MediaFoundation(media_foundation::MfEncoder),
    Software(software::X264Encoder),
}

impl VideoEncoder {
    pub fn new(config: &EncoderConfig) -> anyhow::Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let inner = videotoolbox::VTEncoder::new(config)?;
            Ok(Self { inner })
        }
        #[cfg(target_os = "windows")]
        {
            Self::probe_windows_encoder(config)
        }
    }

    pub fn encode(&mut self, frame: &CapturedFrame) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.inner.encode_frame(frame)
        }
        #[cfg(target_os = "windows")]
        {
            match &mut self.inner {
                WindowsEncoder::Nvenc(enc) => enc.encode(frame),
                WindowsEncoder::MediaFoundation(enc) => enc.encode(frame),
                WindowsEncoder::Software(enc) => enc.encode(frame),
            }
        }
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        #[cfg(target_os = "macos")]
        {
            self.inner.next_encoded()
        }
        #[cfg(target_os = "windows")]
        {
            match &self.inner {
                WindowsEncoder::Nvenc(enc) => enc.next_encoded(),
                WindowsEncoder::MediaFoundation(enc) => enc.next_encoded(),
                WindowsEncoder::Software(enc) => enc.next_encoded(),
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn probe_windows_encoder(config: &EncoderConfig) -> anyhow::Result<Self> {
        if let Ok(enc) = nvenc::NvencEncoder::new(config) {
            tracing::info!("encoder: using NVENC");
            return Ok(Self { inner: WindowsEncoder::Nvenc(enc) });
        }
        if let Ok(enc) = media_foundation::MfEncoder::new(config) {
            tracing::info!("encoder: using Media Foundation HW");
            return Ok(Self { inner: WindowsEncoder::MediaFoundation(enc) });
        }
        let enc = software::X264Encoder::new(config)?;
        tracing::info!("encoder: using x264 software");
        Ok(Self { inner: WindowsEncoder::Software(enc) })
    }
}
