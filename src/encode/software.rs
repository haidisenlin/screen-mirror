use std::sync::mpsc::{self, Receiver, SyncSender};

use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

use super::{EncodedPacket, EncoderConfig};

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

    pub fn encode_nv12(&mut self, nv12_texture: &ID3D11Texture2D, timestamp_ns: u64) -> anyhow::Result<()> {
        unsafe {
            // Create a staging texture for CPU readback
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            nv12_texture.GetDesc(&mut desc);

            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: self.width,
                Height: self.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: windows::Win32::Graphics::Direct3D11::D3D11_BIND_FLAG(0),
                CPUAccessFlags: D3D11_CPU_ACCESS_READ,
                MiscFlags: windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_FLAG(0),
            };

            let device: ID3D11Device = {
                let mut dev = None;
                nv12_texture.GetDevice(&mut dev);
                dev.ok_or_else(|| anyhow::anyhow!("failed to get device from texture"))?
            };

            let staging = device.CreateTexture2D(&staging_desc, None)?;

            let mut ctx = None;
            device.GetImmediateContext(&mut ctx);
            let context = ctx.ok_or_else(|| anyhow::anyhow!("failed to get device context"))?;

            context.CopyResource(&staging, nv12_texture);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(
                &staging,
                0,
                windows::Win32::Graphics::Direct3D11::D3D11_MAP_READ,
                0,
                Some(&mut mapped),
            )?;

            // TODO: Feed mapped NV12 data to openh264 encoder when crate is integrated.
            // For now, just unmap and emit a placeholder keyframe.

            context.Unmap(&staging, 0);

            let packet = EncodedPacket {
                data: Vec::new(),
                is_keyframe: true,
                timestamp: timestamp_ns,
                nal_units: Vec::new(),
            };

            let _ = self.tx.try_send(packet);
            Ok(())
        }
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        self.rx.try_recv().ok()
    }
}
