use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ControlMessage {
    #[serde(rename = "disconnect")]
    Disconnect { reason: DisconnectReason },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "bitrate_adjust")]
    BitrateAdjust { bitrate: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisconnectReason {
    #[serde(rename = "sender_initiated")]
    SenderInitiated,
    #[serde(rename = "receiver_initiated")]
    ReceiverInitiated,
}

impl ControlMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("ControlMessage serialization cannot fail")
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_roundtrip() {
        let msg = ControlMessage::Disconnect {
            reason: DisconnectReason::SenderInitiated,
        };
        let bytes = msg.to_bytes();
        let parsed = ControlMessage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ping_pong_roundtrip() {
        let ping = ControlMessage::Ping;
        let pong = ControlMessage::Pong;
        assert_eq!(ControlMessage::from_bytes(&ping.to_bytes()).unwrap(), ping);
        assert_eq!(ControlMessage::from_bytes(&pong.to_bytes()).unwrap(), pong);
    }

    #[test]
    fn bitrate_adjust_roundtrip() {
        let msg = ControlMessage::BitrateAdjust {
            bitrate: 15_000_000,
        };
        let bytes = msg.to_bytes();
        let parsed = ControlMessage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn disconnect_json_format() {
        let msg = ControlMessage::Disconnect {
            reason: DisconnectReason::ReceiverInitiated,
        };
        let json = String::from_utf8(msg.to_bytes()).unwrap();
        assert!(json.contains("\"type\":\"disconnect\""));
        assert!(json.contains("\"reason\":\"receiver_initiated\""));
    }
}
