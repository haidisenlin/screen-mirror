// src/encode/mod.rs

#[cfg(target_os = "windows")]
pub mod convert;
#[cfg(target_os = "windows")]
pub mod media_foundation;
#[cfg(target_os = "windows")]
pub mod nvenc;
#[cfg(target_os = "windows")]
pub mod software;
#[cfg(target_os = "macos")]
pub mod videotoolbox;

use crate::capture::CapturedFrame;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};

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
    #[cfg(target_os = "windows")]
    converter: convert::BgraToNv12Converter,
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
            anyhow::bail!(
                "on Windows, use VideoEncoder::new_with_device() to provide the D3D11 device"
            )
        }
    }

    #[cfg(target_os = "windows")]
    pub fn new_with_device(config: &EncoderConfig, device: &ID3D11Device) -> anyhow::Result<Self> {
        Self::probe_windows_encoder(config, device)
    }

    pub fn encode(&mut self, frame: CapturedFrame) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.inner.encode_frame(&frame)
        }
        #[cfg(target_os = "windows")]
        {
            // Take ownership immediately so the texture is freed on any exit path
            let bgra_box = unsafe { Box::from_raw(frame.native as *mut ID3D11Texture2D) };
            let nv12_texture = self.converter.convert(&bgra_box)?;

            match &mut self.inner {
                WindowsEncoder::Nvenc(enc) => enc.encode_nv12(nv12_texture, frame.timestamp_ns),
                WindowsEncoder::MediaFoundation(enc) => {
                    enc.encode_nv12(nv12_texture, frame.timestamp_ns)
                }
                WindowsEncoder::Software(enc) => enc.encode_nv12(nv12_texture, frame.timestamp_ns),
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
    fn probe_windows_encoder(
        config: &EncoderConfig,
        device: &ID3D11Device,
    ) -> anyhow::Result<Self> {
        let converter = convert::BgraToNv12Converter::new(device, config.width, config.height)?;

        let inner = if let Ok(enc) = nvenc::NvencEncoder::new(config) {
            tracing::info!("encoder: using NVENC");
            WindowsEncoder::Nvenc(enc)
        } else if let Ok(enc) = media_foundation::MfEncoder::new(config) {
            tracing::info!("encoder: using Media Foundation HW");
            WindowsEncoder::MediaFoundation(enc)
        } else {
            let enc = software::X264Encoder::new(config)?;
            tracing::info!("encoder: using software (openh264)");
            WindowsEncoder::Software(enc)
        };

        Ok(Self { inner, converter })
    }
}
