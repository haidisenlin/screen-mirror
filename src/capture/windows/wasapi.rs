use crate::capture::AudioCapture;

pub struct WasapiCapture;

impl AudioCapture for WasapiCapture {
    fn new(_sample_rate: u32, _channels: u16) -> anyhow::Result<Self> {
        todo!("WASAPI Loopback implementation")
    }

    fn try_next_audio(&self) -> Option<Vec<f32>> {
        todo!("WASAPI capture implementation")
    }
}
