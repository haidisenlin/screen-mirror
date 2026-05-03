use super::{EncoderConfig, EncodedPacket};
use crate::capture::CapturedFrame;

pub struct MfEncoder;

impl MfEncoder {
    pub fn new(_config: &EncoderConfig) -> anyhow::Result<Self> {
        anyhow::bail!("Media Foundation not available")
    }

    pub fn encode(&mut self, _frame: &CapturedFrame) -> anyhow::Result<()> {
        todo!()
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        todo!()
    }
}
