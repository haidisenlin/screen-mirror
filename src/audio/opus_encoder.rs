use anyhow::Result;
use audiopus::{Application, Channels, SampleRate, coder::Encoder};

use super::AudioConfig;

pub struct OpusEncoder {
    encoder: Encoder,
    frame_size: usize,
    channels: u16,
    encode_buf: Vec<u8>,
}

impl OpusEncoder {
    pub fn new(config: &AudioConfig) -> Result<Self> {
        let sample_rate = match config.sample_rate {
            8000 => SampleRate::Hz8000,
            12000 => SampleRate::Hz12000,
            16000 => SampleRate::Hz16000,
            24000 => SampleRate::Hz24000,
            48000 => SampleRate::Hz48000,
            _ => anyhow::bail!("unsupported sample rate: {}", config.sample_rate),
        };

        let channels = match config.channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => anyhow::bail!("unsupported channels: {}", config.channels),
        };

        let mut encoder = Encoder::new(sample_rate, channels, Application::LowDelay)?;
        encoder.set_bitrate(audiopus::Bitrate::BitsPerSecond(config.bitrate as i32))?;
        encoder.set_dtx(true)?;
        encoder.set_inband_fec(false)?;

        let frame_size = (config.sample_rate * config.frame_duration_ms / 1000) as usize;

        Ok(Self {
            encoder,
            frame_size,
            channels: config.channels,
            encode_buf: vec![0u8; 4000],
        })
    }

    pub fn encode(&mut self, pcm: &[f32]) -> Result<&[u8]> {
        let len = self.encoder.encode_float(pcm, &mut self.encode_buf)?;
        Ok(&self.encode_buf[..len])
    }

    pub fn frame_size_samples(&self) -> usize {
        self.frame_size * self.channels as usize
    }
}
