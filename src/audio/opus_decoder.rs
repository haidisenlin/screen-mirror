use std::convert::TryFrom;

use anyhow::Result;
use audiopus::{Channels, MutSignals, SampleRate, coder::Decoder, packet::Packet};

use super::AudioConfig;

pub struct OpusDecoder {
    decoder: Decoder,
    frame_size: usize,
    channels: u16,
    decode_buf: Vec<f32>,
}

impl OpusDecoder {
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

        let decoder = Decoder::new(sample_rate, channels)?;
        let frame_size = (config.sample_rate * config.frame_duration_ms / 1000) as usize
            * config.channels as usize;

        Ok(Self {
            decoder,
            frame_size,
            channels: config.channels,
            decode_buf: vec![0.0f32; frame_size],
        })
    }

    pub fn decode(&mut self, opus_data: &[u8]) -> Result<&[f32]> {
        let packet = Packet::try_from(opus_data)?;
        let output = MutSignals::try_from(&mut *self.decode_buf)?;
        let samples = self.decoder.decode_float(Some(packet), output, false)?;
        Ok(&self.decode_buf[..samples * self.channels as usize])
    }

    pub fn decode_plc(&mut self) -> Result<&[f32]> {
        let output = MutSignals::try_from(&mut *self.decode_buf)?;
        let samples = self.decoder.decode_float(None, output, false)?;
        Ok(&self.decode_buf[..samples * self.channels as usize])
    }

    pub fn frame_size_samples(&self) -> usize {
        self.frame_size
    }
}
