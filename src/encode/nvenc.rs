use super::{EncoderConfig, EncodedPacket};
use crate::capture::CapturedFrame;

pub struct NvencEncoder;

impl NvencEncoder {
    pub fn new(_config: &EncoderConfig) -> anyhow::Result<Self> {
        anyhow::bail!("NVENC not available")
    }

    pub fn encode(&mut self, _frame: &CapturedFrame) -> anyhow::Result<()> {
        todo!()
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        todo!()
    }
}
