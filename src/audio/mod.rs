pub mod opus_encoder;
pub mod opus_decoder;
pub mod output;

pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub bitrate: u32,
    pub frame_duration_ms: u32,
}

impl AudioConfig {
    pub fn frame_size(&self) -> usize {
        (self.sample_rate * self.frame_duration_ms / 1000) as usize * self.channels as usize
    }
}
