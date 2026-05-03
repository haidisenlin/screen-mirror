use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Offer {
    pub video: OfferVideo,
    pub audio: OfferAudio,
    pub transport: OfferTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfferVideo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfferAudio {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bitrate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfferTransport {
    pub udp_port: u16,
    pub fec_group_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Answer {
    pub video: AnswerVideo,
    pub audio: AnswerAudio,
    pub transport: AnswerTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerVideo {
    pub codec: String,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerAudio {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerTransport {
    pub udp_port: u16,
}

#[derive(Debug, Clone)]
pub struct NegotiatedParams {
    pub video_width: u32,
    pub video_height: u32,
    pub video_fps: u32,
    pub video_bitrate: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub audio_bitrate: u32,
    pub sender_udp_port: u16,
    pub receiver_udp_port: u16,
    pub fec_group_size: usize,
}

impl NegotiatedParams {
    pub fn resolve(offer: &Offer, answer: &Answer) -> Self {
        Self {
            video_width: offer.video.width.min(answer.video.max_width),
            video_height: offer.video.height.min(answer.video.max_height),
            video_fps: offer.video.fps.min(answer.video.max_fps),
            video_bitrate: offer.video.bitrate,
            audio_sample_rate: offer.audio.sample_rate.min(answer.audio.sample_rate),
            audio_channels: offer.audio.channels.min(answer.audio.channels),
            audio_bitrate: offer.audio.bitrate,
            sender_udp_port: offer.transport.udp_port,
            receiver_udp_port: answer.transport.udp_port,
            fec_group_size: offer.transport.fec_group_size,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NegotiateMessage {
    #[serde(rename = "offer")]
    Offer(Offer),
    #[serde(rename = "answer")]
    Answer(Answer),
}

impl NegotiateMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("NegotiateMessage serialization cannot fail")
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_offer() -> Offer {
        Offer {
            video: OfferVideo {
                codec: "h264".to_string(),
                width: 2560,
                height: 1440,
                fps: 60,
                bitrate: 20_000_000,
            },
            audio: OfferAudio {
                codec: "opus".to_string(),
                sample_rate: 48000,
                channels: 2,
                bitrate: 128_000,
            },
            transport: OfferTransport {
                udp_port: 5004,
                fec_group_size: 6,
            },
        }
    }

    fn sample_answer() -> Answer {
        Answer {
            video: AnswerVideo {
                codec: "h264".to_string(),
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
            },
            audio: AnswerAudio {
                codec: "opus".to_string(),
                sample_rate: 48000,
                channels: 2,
            },
            transport: AnswerTransport { udp_port: 5004 },
        }
    }

    #[test]
    fn offer_roundtrip() {
        let msg = NegotiateMessage::Offer(sample_offer());
        let bytes = msg.to_bytes();
        let parsed = NegotiateMessage::from_bytes(&bytes).unwrap();
        match parsed {
            NegotiateMessage::Offer(o) => assert_eq!(o, sample_offer()),
            _ => panic!("expected Offer"),
        }
    }

    #[test]
    fn answer_roundtrip() {
        let msg = NegotiateMessage::Answer(sample_answer());
        let bytes = msg.to_bytes();
        let parsed = NegotiateMessage::from_bytes(&bytes).unwrap();
        match parsed {
            NegotiateMessage::Answer(a) => assert_eq!(a, sample_answer()),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn resolve_takes_min() {
        let offer = sample_offer();
        let answer = sample_answer();
        let params = NegotiatedParams::resolve(&offer, &answer);
        assert_eq!(params.video_width, 1920);
        assert_eq!(params.video_height, 1080);
        assert_eq!(params.video_fps, 60);
    }
}
