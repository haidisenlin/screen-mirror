use std::net::SocketAddr;

use crate::discovery::browser::DiscoveredReceiver;

#[derive(Debug, Clone)]
pub enum CaptureMode {
    FullScreen,
}

#[derive(Debug, Clone)]
pub struct StreamStats {
    pub resolution_w: u32,
    pub resolution_h: u32,
    pub fps: f32,
    pub bitrate_bps: u64,
    pub latency_ms: f32,
    pub packet_loss_pct: f32,
}

#[derive(Debug)]
pub enum UiCommand {
    Connect { addr: SocketAddr, pin: String },
    StartStreaming { mode: CaptureMode },
    Pause,
    Resume,
    Disconnect,
}

#[derive(Debug)]
pub enum BackendEvent {
    DevicesUpdated(Vec<DiscoveredReceiver>),
    PairingSuccess,
    PairingFailed(String),
    PairingTimeout,
    StreamingStarted,
    StatsUpdate(StreamStats),
    Disconnected(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn commands_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<UiCommand>();
    }

    #[test]
    fn events_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<BackendEvent>();
    }

    #[test]
    fn channel_roundtrip() {
        let (tx, rx) = mpsc::channel::<BackendEvent>();
        tx.send(BackendEvent::PairingSuccess).unwrap();
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, BackendEvent::PairingSuccess));
    }
}
