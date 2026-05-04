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
    VerifyPin { pin: String },
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
    PinMatched { device_name: String, addr: SocketAddr },
    PinNotFound,
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

    #[test]
    fn stream_stats_clone() {
        let stats = StreamStats {
            resolution_w: 1920,
            resolution_h: 1080,
            fps: 60.0,
            bitrate_bps: 10_000_000,
            latency_ms: 3.5,
            packet_loss_pct: 0.1,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.resolution_w, 1920);
        assert_eq!(cloned.resolution_h, 1080);
        assert_eq!(cloned.fps, 60.0);
        assert_eq!(cloned.bitrate_bps, 10_000_000);
        assert_eq!(cloned.latency_ms, 3.5);
        assert_eq!(cloned.packet_loss_pct, 0.1);
    }

    #[test]
    fn capture_mode_is_debug() {
        let mode = CaptureMode::FullScreen;
        let debug = format!("{:?}", mode);
        assert_eq!(debug, "FullScreen");
    }

    #[test]
    fn all_backend_events_debug() {
        let events: Vec<BackendEvent> = vec![
            BackendEvent::DevicesUpdated(vec![]),
            BackendEvent::PairingSuccess,
            BackendEvent::PairingFailed("err".to_string()),
            BackendEvent::PairingTimeout,
            BackendEvent::PinMatched {
                device_name: "test".to_string(),
                addr: "127.0.0.1:9000".parse().unwrap(),
            },
            BackendEvent::PinNotFound,
            BackendEvent::StreamingStarted,
            BackendEvent::StatsUpdate(StreamStats {
                resolution_w: 0,
                resolution_h: 0,
                fps: 0.0,
                bitrate_bps: 0,
                latency_ms: 0.0,
                packet_loss_pct: 0.0,
            }),
            BackendEvent::Disconnected(String::new()),
        ];
        for event in events {
            let _ = format!("{:?}", event);
        }
    }

    #[test]
    fn all_ui_commands_debug() {
        let commands: Vec<UiCommand> = vec![
            UiCommand::Connect {
                addr: "127.0.0.1:9000".parse().unwrap(),
                pin: "123456".to_string(),
            },
            UiCommand::VerifyPin {
                pin: "123456".to_string(),
            },
            UiCommand::StartStreaming {
                mode: CaptureMode::FullScreen,
            },
            UiCommand::Pause,
            UiCommand::Resume,
            UiCommand::Disconnect,
        ];
        for cmd in commands {
            let _ = format!("{:?}", cmd);
        }
    }

    #[test]
    fn command_channel_bidirectional() {
        let (cmd_tx, cmd_rx) = mpsc::channel::<UiCommand>();
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>();

        cmd_tx
            .send(UiCommand::Connect {
                addr: "192.168.1.100:9000".parse().unwrap(),
                pin: "654321".to_string(),
            })
            .unwrap();
        evt_tx.send(BackendEvent::PairingSuccess).unwrap();

        let cmd = cmd_rx.recv().unwrap();
        let evt = evt_rx.recv().unwrap();

        assert!(matches!(cmd, UiCommand::Connect { .. }));
        assert!(matches!(evt, BackendEvent::PairingSuccess));
    }
}
