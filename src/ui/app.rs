use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportCommand};
use tray_icon::TrayIconEvent;

use crate::ui::messages::{BackendEvent, StreamStats, UiCommand};
use crate::ui::theme::{PANEL_MAX_HEIGHT, PANEL_WIDTH};
use crate::ui::tray::{AppTray, TrayState, calculate_position};
use crate::ui::views::idle::{IdleAction, IdleViewState};
use crate::ui::views::mode::ModeAction;
use crate::ui::views::paused::PausedAction;
use crate::ui::views::streaming::StreamingAction;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub enum AppState {
    Idle,
    Connecting {
        device_name: String,
        started_at: Instant,
    },
    ModeSelect {
        device_name: String,
    },
    Streaming {
        device_name: String,
        stats: StreamStats,
    },
    Paused {
        device_name: String,
    },
}

/// State machine core — no OS resources; safe to construct in tests.
struct AppCore {
    state: AppState,
    visible: bool,
    idle_view: IdleViewState,
    cmd_tx: mpsc::Sender<UiCommand>,
    event_rx: mpsc::Receiver<BackendEvent>,
}

impl AppCore {
    fn new(cmd_tx: mpsc::Sender<UiCommand>, event_rx: mpsc::Receiver<BackendEvent>) -> Self {
        Self {
            state: AppState::Idle,
            visible: false,
            idle_view: IdleViewState {
                devices: Vec::new(),
                selected_device: None,
                pin_input: String::new(),
                error: None,
                connecting: false,
                connecting_device: None,
            },
            cmd_tx,
            event_rx,
        }
    }

    fn tray_state(&self) -> TrayState {
        match &self.state {
            AppState::Streaming { .. } => TrayState::Streaming,
            AppState::Paused { .. } => TrayState::Paused,
            _ => TrayState::Idle,
        }
    }

    fn process_backend_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                BackendEvent::DevicesUpdated(devices) => {
                    self.idle_view.devices = devices;
                }
                BackendEvent::PairingSuccess => {
                    if let AppState::Connecting { device_name, .. } =
                        std::mem::replace(&mut self.state, AppState::Idle)
                    {
                        self.idle_view.connecting = false;
                        self.idle_view.connecting_device = None;
                        self.idle_view.error = None;
                        self.state = AppState::ModeSelect { device_name };
                    }
                }
                BackendEvent::PairingFailed(reason) => {
                    self.idle_view.connecting = false;
                    self.idle_view.connecting_device = None;
                    self.idle_view.error = Some(reason);
                    self.state = AppState::Idle;
                }
                BackendEvent::PairingTimeout => {
                    self.idle_view.connecting = false;
                    self.idle_view.connecting_device = None;
                    self.idle_view.error = Some("连接超时".to_string());
                    self.state = AppState::Idle;
                }
                BackendEvent::StreamingStarted => {
                    if let AppState::ModeSelect { device_name } =
                        std::mem::replace(&mut self.state, AppState::Idle)
                    {
                        self.state = AppState::Streaming {
                            device_name,
                            stats: StreamStats {
                                resolution_w: 0,
                                resolution_h: 0,
                                fps: 0.0,
                                bitrate_bps: 0,
                                latency_ms: 0.0,
                                packet_loss_pct: 0.0,
                            },
                        };
                    }
                }
                BackendEvent::StatsUpdate(stats) => {
                    if let AppState::Streaming { stats: ref mut s, .. } = self.state {
                        *s = stats;
                    }
                }
                BackendEvent::Disconnected(reason) => {
                    self.idle_view.error = if reason.is_empty() { None } else { Some(reason) };
                    self.idle_view.connecting = false;
                    self.idle_view.connecting_device = None;
                    self.state = AppState::Idle;
                }
            }
        }
    }

    fn check_connecting_timeout(&mut self) {
        if let AppState::Connecting { started_at, .. } = &self.state
            && started_at.elapsed() >= CONNECT_TIMEOUT
        {
            self.idle_view.connecting = false;
            self.idle_view.connecting_device = None;
            self.idle_view.error = Some("连接超时".to_string());
            self.state = AppState::Idle;
        }
    }

    fn render_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(PANEL_WIDTH);
        ui.set_max_width(PANEL_WIDTH);
        ui.set_max_height(PANEL_MAX_HEIGHT);

        match &mut self.state {
            AppState::Idle | AppState::Connecting { .. } => {
                let action = crate::ui::views::idle::render(ui, &mut self.idle_view);
                if let IdleAction::Connect { device_index, pin } = action
                    && let Some(device) = self.idle_view.devices.get(device_index)
                {
                    let addr = device.addr;
                    let name = device.name.clone();
                    self.idle_view.connecting = true;
                    self.idle_view.connecting_device = Some(name.clone());
                    self.idle_view.error = None;
                    self.state = AppState::Connecting {
                        device_name: name,
                        started_at: Instant::now(),
                    };
                    let _ = self.cmd_tx.send(UiCommand::Connect { addr, pin });
                }
            }
            AppState::ModeSelect { device_name } => {
                let device_name = device_name.clone();
                let action = crate::ui::views::mode::render(ui, &device_name);
                if let ModeAction::Start(mode) = action {
                    let _ = self.cmd_tx.send(UiCommand::StartStreaming { mode });
                }
            }
            AppState::Streaming { device_name, stats } => {
                let device_name = device_name.clone();
                let action = crate::ui::views::streaming::render(ui, &device_name, stats);
                match action {
                    StreamingAction::Pause => {
                        let _ = self.cmd_tx.send(UiCommand::Pause);
                        self.state = AppState::Paused { device_name };
                    }
                    StreamingAction::Disconnect => {
                        let _ = self.cmd_tx.send(UiCommand::Disconnect);
                    }
                    StreamingAction::None => {}
                }
            }
            AppState::Paused { device_name } => {
                let device_name = device_name.clone();
                let action = crate::ui::views::paused::render(ui, &device_name);
                match action {
                    PausedAction::Resume => {
                        let _ = self.cmd_tx.send(UiCommand::Resume);
                        self.state = AppState::Streaming {
                            device_name,
                            stats: StreamStats {
                                resolution_w: 0,
                                resolution_h: 0,
                                fps: 0.0,
                                bitrate_bps: 0,
                                latency_ms: 0.0,
                                packet_loss_pct: 0.0,
                            },
                        };
                    }
                    PausedAction::Disconnect => {
                        let _ = self.cmd_tx.send(UiCommand::Disconnect);
                    }
                    PausedAction::None => {}
                }
            }
        }
    }
}

pub struct App {
    tray: AppTray,
    core: AppCore,
}

impl App {
    fn new(cmd_tx: mpsc::Sender<UiCommand>, event_rx: mpsc::Receiver<BackendEvent>) -> Self {
        Self {
            tray: AppTray::new(),
            core: AppCore::new(cmd_tx, event_rx),
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check tray click events — toggle panel visibility
        if let Ok(TrayIconEvent::Click { rect, .. }) = TrayIconEvent::receiver().try_recv() {
            self.core.visible = !self.core.visible;
            if self.core.visible {
                let pos = calculate_position(rect);
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            ctx.send_viewport_cmd(ViewportCommand::Visible(self.core.visible));
        }

        // Hide on focus loss
        if let Some(false) = ctx.input(|i| i.viewport().focused)
            && self.core.visible
        {
            self.core.visible = false;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        self.core.process_backend_events();
        self.core.check_connecting_timeout();

        // Update tray icon color based on state
        self.tray.set_state(self.core.tray_state());

        // Request repaint periodically during active states
        match &self.core.state {
            AppState::Streaming { .. } | AppState::Connecting { .. } => {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.core.visible {
            return;
        }
        self.core.render_ui(ui);
    }
}

pub fn run(
    cmd_tx: mpsc::Sender<UiCommand>,
    event_rx: mpsc::Receiver<BackendEvent>,
) -> eframe::Result<()> {
    let app = App::new(cmd_tx, event_rx);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_visible(false)
            .with_inner_size([PANEL_WIDTH, PANEL_MAX_HEIGHT]),
        ..Default::default()
    };

    eframe::run_native(
        "screen-mirror",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn make_core() -> (AppCore, mpsc::Sender<BackendEvent>, mpsc::Receiver<UiCommand>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let core = AppCore::new(cmd_tx, evt_rx);
        (core, evt_tx, cmd_rx)
    }

    fn dummy_device(name: &str) -> crate::discovery::browser::DiscoveredReceiver {
        crate::discovery::browser::DiscoveredReceiver {
            name: name.to_string(),
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000),
        }
    }

    #[test]
    fn initial_state_is_idle() {
        let (core, _evt_tx, _cmd_rx) = make_core();
        assert!(matches!(core.state, AppState::Idle));
    }

    #[test]
    fn tray_state_idle() {
        let (core, _evt_tx, _cmd_rx) = make_core();
        assert_eq!(core.tray_state(), TrayState::Idle);
    }

    #[test]
    fn tray_state_streaming() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Streaming {
            device_name: "TV".to_string(),
            stats: StreamStats {
                resolution_w: 1920,
                resolution_h: 1080,
                fps: 30.0,
                bitrate_bps: 5_000_000,
                latency_ms: 10.0,
                packet_loss_pct: 0.0,
            },
        };
        assert_eq!(core.tray_state(), TrayState::Streaming);
    }

    #[test]
    fn tray_state_paused() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Paused {
            device_name: "TV".to_string(),
        };
        assert_eq!(core.tray_state(), TrayState::Paused);
    }

    #[test]
    fn pairing_success_transitions_to_mode_select() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;
        core.idle_view.connecting_device = Some("TV".to_string());

        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::ModeSelect { .. }));
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.error.is_none());
    }

    #[test]
    fn pairing_failed_returns_to_idle_with_error() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;

        evt_tx.send(BackendEvent::PairingFailed("wrong pin".to_string())).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert_eq!(core.idle_view.error.as_deref(), Some("wrong pin"));
    }

    #[test]
    fn pairing_timeout_event_returns_to_idle() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };

        evt_tx.send(BackendEvent::PairingTimeout).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert!(core.idle_view.error.is_some());
    }

    #[test]
    fn streaming_started_transitions_from_mode_select() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::ModeSelect {
            device_name: "TV".to_string(),
        };

        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Streaming { .. }));
    }

    #[test]
    fn stats_update_updates_streaming_stats() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Streaming {
            device_name: "TV".to_string(),
            stats: StreamStats {
                resolution_w: 0,
                resolution_h: 0,
                fps: 0.0,
                bitrate_bps: 0,
                latency_ms: 0.0,
                packet_loss_pct: 0.0,
            },
        };

        evt_tx
            .send(BackendEvent::StatsUpdate(StreamStats {
                resolution_w: 1920,
                resolution_h: 1080,
                fps: 60.0,
                bitrate_bps: 8_000_000,
                latency_ms: 5.0,
                packet_loss_pct: 0.1,
            }))
            .unwrap();
        core.process_backend_events();

        if let AppState::Streaming { stats, .. } = &core.state {
            assert_eq!(stats.resolution_w, 1920);
            assert_eq!(stats.fps, 60.0);
        } else {
            panic!("expected Streaming state");
        }
    }

    #[test]
    fn disconnected_returns_to_idle() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Streaming {
            device_name: "TV".to_string(),
            stats: StreamStats {
                resolution_w: 0,
                resolution_h: 0,
                fps: 0.0,
                bitrate_bps: 0,
                latency_ms: 0.0,
                packet_loss_pct: 0.0,
            },
        };

        evt_tx.send(BackendEvent::Disconnected("network error".to_string())).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert_eq!(core.idle_view.error.as_deref(), Some("network error"));
    }

    #[test]
    fn connecting_timeout_resets_to_idle() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now() - Duration::from_secs(11),
        };
        core.idle_view.connecting = true;

        core.check_connecting_timeout();

        assert!(matches!(core.state, AppState::Idle));
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.error.is_some());
    }

    #[test]
    fn devices_updated_populates_idle_view() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        assert!(core.idle_view.devices.is_empty());

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![
                dummy_device("TV1"),
                dummy_device("TV2"),
            ]))
            .unwrap();
        core.process_backend_events();

        assert_eq!(core.idle_view.devices.len(), 2);
        assert_eq!(core.idle_view.devices[0].name, "TV1");
    }
}
