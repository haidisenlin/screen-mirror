use super::{EncoderConfig, EncodedPacket};
use crate::capture::CapturedFrame;

pub struct X264Encoder;

impl X264Encoder {
    pub fn new(_config: &EncoderConfig) -> anyhow::Result<Self> {
        todo!("x264 encoder implementation")
    }

    pub fn encode(&mut self, _frame: &CapturedFrame) -> anyhow::Result<()> {
        todo!()
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        todo!()
    }
}
