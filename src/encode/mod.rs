#[cfg(target_os = "macos")]
pub mod videotoolbox;

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
