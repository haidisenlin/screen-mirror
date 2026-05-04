use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportCommand};

use crate::ui::anim::AiBackground;
use crate::ui::messages::{BackendEvent, StreamStats, UiCommand};
use crate::ui::theme::{PANEL_MAX_HEIGHT, PANEL_WIDTH};
use crate::ui::tray::{AppTray, TrayState};
use crate::ui::views::idle::{IdleAction, IdleViewState, PinVerifyState};
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
#[allow(dead_code)]
struct AppCore {
    state: AppState,
    visible: bool,
    idle_view: IdleViewState,
    background: AiBackground,
    cmd_tx: mpsc::Sender<UiCommand>,
    event_rx: mpsc::Receiver<BackendEvent>,
}

impl AppCore {
    fn new(cmd_tx: mpsc::Sender<UiCommand>, event_rx: mpsc::Receiver<BackendEvent>) -> Self {
        Self {
            state: AppState::Idle,
            visible: true,
            idle_view: IdleViewState {
                devices: Vec::new(),
                selected_device: None,
                pin_input: String::new(),
                pin_cursor: 0,
                pin_verify_state: PinVerifyState::Idle,
                error: None,
                connecting: false,
                connecting_device: None,
            },
            background: AiBackground::new(),
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
                    if matches!(self.state, AppState::Connecting { .. }) {
                        if let AppState::Connecting { device_name, .. } =
                            std::mem::replace(&mut self.state, AppState::Idle)
                        {
                            self.idle_view.connecting = false;
                            self.idle_view.connecting_device = None;
                            self.idle_view.error = None;
                            self.state = AppState::ModeSelect { device_name };
                        }
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
                BackendEvent::PinMatched { device_name, addr } => {
                    let pin = self.idle_view.pin_input.clone();
                    self.idle_view.pin_verify_state = PinVerifyState::Matched { device_name, addr, pin };
                }
                BackendEvent::PinNotFound => {
                    let pin = self.idle_view.pin_input.clone();
                    self.idle_view.pin_verify_state = PinVerifyState::NotFound { pin };
                }
                BackendEvent::StreamingStarted => {
                    if matches!(self.state, AppState::ModeSelect { .. }) {
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

    fn check_pin_verify(&mut self) {
        let pin = &self.idle_view.pin_input;

        if pin.len() < 6 {
            self.idle_view.pin_verify_state = PinVerifyState::Idle;
            return;
        }

        match &self.idle_view.pin_verify_state {
            PinVerifyState::Debouncing { pin: prev_pin, since } => {
                if *prev_pin != *pin {
                    self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                        since: Instant::now(),
                        pin: pin.clone(),
                    };
                } else if since.elapsed() >= Duration::from_millis(300) {
                    let verified_pin = pin.clone();
                    let _ = self.cmd_tx.send(UiCommand::VerifyPin { pin: pin.clone() });
                    self.idle_view.pin_verify_state = PinVerifyState::Verifying { pin: verified_pin };
                }
            }
            PinVerifyState::Matched { pin: verified_pin, .. }
            | PinVerifyState::NotFound { pin: verified_pin } => {
                if *pin != *verified_pin {
                    self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                        since: Instant::now(),
                        pin: pin.clone(),
                    };
                }
            }
            PinVerifyState::Idle => {
                self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                    since: Instant::now(),
                    pin: pin.clone(),
                };
            }
            PinVerifyState::Verifying { .. } => {}
        }
    }

    fn handle_connect_matched(&mut self) {
        if let PinVerifyState::Matched { device_name, addr, .. } = &self.idle_view.pin_verify_state {
            let addr = *addr;
            let name = device_name.clone();
            let pin = self.idle_view.pin_input.clone();
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

    #[cfg(debug_assertions)]
    fn handle_debug_keys(&mut self, ctx: &egui::Context) {
        use crate::discovery::browser::DiscoveredReceiver;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        ctx.input(|input| {
            let ctrl = input.modifiers.ctrl || input.modifiers.mac_cmd;
            if !ctrl {
                return;
            }
            for event in &input.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    match key {
                        egui::Key::Num1 => {
                            self.idle_view.connecting = false;
                            self.idle_view.connecting_device = None;
                            self.idle_view.error = None;
                            self.state = AppState::Idle;
                        }
                        egui::Key::Num2 => {
                            self.state = AppState::ModeSelect {
                                device_name: "测试设备".to_string(),
                            };
                        }
                        egui::Key::Num3 => {
                            self.state = AppState::Streaming {
                                device_name: "测试设备".to_string(),
                                stats: StreamStats {
                                    resolution_w: 2560,
                                    resolution_h: 1440,
                                    fps: 60.0,
                                    bitrate_bps: 18_200_000,
                                    latency_ms: 3.2,
                                    packet_loss_pct: 0.1,
                                },
                            };
                        }
                        egui::Key::Num4 => {
                            self.state = AppState::Paused {
                                device_name: "测试设备".to_string(),
                            };
                        }
                        egui::Key::Num5 => {
                            self.idle_view.devices = vec![
                                DiscoveredReceiver {
                                    name: "客厅电视".to_string(),
                                    addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 9000),
                                },
                                DiscoveredReceiver {
                                    name: "会议室投影".to_string(),
                                    addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101)), 9000),
                                },
                            ];
                        }
                        egui::Key::Num6 => {
                            self.idle_view.error = Some("投屏码错误，请重试".to_string());
                        }
                        egui::Key::Num7 => {
                            self.idle_view.pin_verify_state = PinVerifyState::Matched {
                                device_name: "测试电视".to_string(),
                                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 9000),
                                pin: self.idle_view.pin_input.clone(),
                            };
                        }
                        egui::Key::Num8 => {
                            self.idle_view.pin_verify_state = PinVerifyState::NotFound {
                                pin: self.idle_view.pin_input.clone(),
                            };
                        }
                        egui::Key::Num9 => {
                            self.idle_view.pin_verify_state = PinVerifyState::Verifying {
                                pin: self.idle_view.pin_input.clone(),
                            };
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    fn render_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(PANEL_WIDTH);
        ui.set_max_width(PANEL_WIDTH);

        match &mut self.state {
            AppState::Idle | AppState::Connecting { .. } => {
                let action = crate::ui::views::idle::render(ui, &mut self.idle_view);
                match action {
                    IdleAction::Connect { device_index, pin } => {
                        if let Some(d) = self.idle_view.devices.get(device_index) {
                            let addr = d.addr;
                            let name = d.name.clone();
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
                    IdleAction::ConnectMatched => {
                        self.handle_connect_matched();
                    }
                    IdleAction::None => {}
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
        ctx.request_repaint_after(Duration::from_millis(200));

        // Check tray events (via AtomicBool flags set by callbacks)
        if self.tray.poll_show() {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        if self.tray.poll_quit() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        self.core.process_backend_events();
        self.core.check_connecting_timeout();
        self.core.check_pin_verify();

        #[cfg(debug_assertions)]
        self.core.handle_debug_keys(ctx);

        self.tray.set_state(self.core.tray_state());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let panel_frame = egui::Frame::new()
            .fill(egui::Color32::WHITE)
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(0))
            .shadow(egui::epaint::Shadow {
                spread: 0,
                blur: 12,
                offset: [0, 2],
                color: egui::Color32::from_black_alpha(30),
            });
        panel_frame.show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            self.core.background.paint(ui);
            let r = ui.interact(ui.max_rect(), ui.id().with("drag"), egui::Sense::drag());
            if r.dragged() {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            self.core.render_ui(ui);
        });
    }
}

pub fn run(
    cmd_tx: mpsc::Sender<UiCommand>,
    event_rx: mpsc::Receiver<BackendEvent>,
) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([PANEL_WIDTH, PANEL_MAX_HEIGHT]),
        ..Default::default()
    };

    eframe::run_native(
        "screen-mirror",
        native_options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            let mut visuals = egui::Visuals::light();
            visuals.window_fill = egui::Color32::TRANSPARENT;
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            // Tray must be created AFTER event loop is running (macOS requirement)
            let app = App::new(cmd_tx, event_rx);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Try loading system CJK font (macOS: PingFang SC, Windows: Microsoft YaHei)
    let font_paths = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "C:\\Windows\\Fonts\\msyh.ttc",
    ];

    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "system_cjk".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(1, "system_cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system_cjk".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
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

    // ── Tray state coverage for Connecting and ModeSelect ──

    #[test]
    fn tray_state_connecting_is_idle() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        assert_eq!(core.tray_state(), TrayState::Idle);
    }

    #[test]
    fn tray_state_mode_select_is_idle() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::ModeSelect {
            device_name: "TV".to_string(),
        };
        assert_eq!(core.tray_state(), TrayState::Idle);
    }

    // ── Timeout boundary: 9s → still connecting ──

    #[test]
    fn connecting_not_timed_out_stays_connecting() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now() - Duration::from_secs(9),
        };
        core.idle_view.connecting = true;

        core.check_connecting_timeout();

        assert!(matches!(core.state, AppState::Connecting { .. }));
        assert!(core.idle_view.connecting);
    }

    // ── Wrong-state events: should be silently ignored ──

    #[test]
    fn pairing_success_when_idle_ignored() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        assert!(matches!(core.state, AppState::Idle));

        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
    }

    #[test]
    fn streaming_started_when_idle_ignored() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        assert!(matches!(core.state, AppState::Idle));

        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
    }

    #[test]
    fn stats_update_when_idle_ignored() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        assert!(matches!(core.state, AppState::Idle));

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

        assert!(matches!(core.state, AppState::Idle));
    }

    #[test]
    fn stats_update_when_paused_ignored() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Paused {
            device_name: "TV".to_string(),
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

        assert!(matches!(core.state, AppState::Paused { .. }));
    }

    #[test]
    fn streaming_started_when_connecting_ignored() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };

        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Connecting { .. }));
    }

    // ── Disconnected edge cases ──

    #[test]
    fn disconnected_empty_reason_no_error() {
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
            .send(BackendEvent::Disconnected(String::new()))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert!(core.idle_view.error.is_none());
    }

    #[test]
    fn disconnected_from_paused_returns_to_idle() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Paused {
            device_name: "TV".to_string(),
        };

        evt_tx
            .send(BackendEvent::Disconnected("network error".to_string()))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert_eq!(core.idle_view.error.as_deref(), Some("network error"));
    }

    #[test]
    fn disconnected_from_connecting_returns_to_idle() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;
        core.idle_view.connecting_device = Some("TV".to_string());

        evt_tx
            .send(BackendEvent::Disconnected("refused".to_string()))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.connecting_device.is_none());
        assert_eq!(core.idle_view.error.as_deref(), Some("refused"));
    }

    #[test]
    fn disconnected_from_mode_select_returns_to_idle() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::ModeSelect {
            device_name: "TV".to_string(),
        };

        evt_tx
            .send(BackendEvent::Disconnected("link lost".to_string()))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
    }

    // ── Multiple rapid events ──

    #[test]
    fn multiple_rapid_events_all_processed() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;

        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        evt_tx
            .send(BackendEvent::StatsUpdate(StreamStats {
                resolution_w: 2560,
                resolution_h: 1440,
                fps: 30.0,
                bitrate_bps: 20_000_000,
                latency_ms: 2.0,
                packet_loss_pct: 0.0,
            }))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Streaming { .. }));
        if let AppState::Streaming { stats, device_name } = &core.state {
            assert_eq!(device_name, "TV");
            assert_eq!(stats.resolution_w, 2560);
            assert_eq!(stats.fps, 30.0);
        }
    }

    // ── Full lifecycle ──

    #[test]
    fn full_lifecycle_idle_to_streaming_to_disconnect() {
        let (mut core, evt_tx, _cmd_rx) = make_core();

        // 1. Start Idle
        assert!(matches!(core.state, AppState::Idle));
        assert_eq!(core.tray_state(), TrayState::Idle);

        // 2. Devices discovered
        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![dummy_device("客厅电视")]))
            .unwrap();
        core.process_backend_events();
        assert_eq!(core.idle_view.devices.len(), 1);

        // 3. Transition to Connecting
        core.idle_view.connecting = true;
        core.idle_view.connecting_device = Some("客厅电视".to_string());
        core.state = AppState::Connecting {
            device_name: "客厅电视".to_string(),
            started_at: Instant::now(),
        };
        assert_eq!(core.tray_state(), TrayState::Idle);

        // 4. Pairing success → ModeSelect
        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        core.process_backend_events();
        assert!(matches!(core.state, AppState::ModeSelect { .. }));
        assert!(!core.idle_view.connecting);

        // 5. Streaming started
        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();
        assert!(matches!(core.state, AppState::Streaming { .. }));
        assert_eq!(core.tray_state(), TrayState::Streaming);

        // 6. Stats update
        evt_tx
            .send(BackendEvent::StatsUpdate(StreamStats {
                resolution_w: 1920,
                resolution_h: 1080,
                fps: 60.0,
                bitrate_bps: 10_000_000,
                latency_ms: 3.0,
                packet_loss_pct: 0.0,
            }))
            .unwrap();
        core.process_backend_events();

        // 7. Disconnect
        evt_tx
            .send(BackendEvent::Disconnected(String::new()))
            .unwrap();
        core.process_backend_events();
        assert!(matches!(core.state, AppState::Idle));
        assert_eq!(core.tray_state(), TrayState::Idle);
        assert!(core.idle_view.error.is_none());
    }

    #[test]
    fn full_lifecycle_with_pause_resume() {
        let (mut core, evt_tx, cmd_rx) = make_core();

        // Setup: reach Streaming state
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();
        assert!(matches!(core.state, AppState::Streaming { .. }));

        // Manually simulate Pause action (render_ui would do this)
        let _ = core.cmd_tx.send(UiCommand::Pause);
        core.state = AppState::Paused {
            device_name: "TV".to_string(),
        };
        assert_eq!(core.tray_state(), TrayState::Paused);

        // Verify Pause command was sent
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, UiCommand::Pause));

        // Resume
        let _ = core.cmd_tx.send(UiCommand::Resume);
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
        assert_eq!(core.tray_state(), TrayState::Streaming);

        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, UiCommand::Resume));
    }

    // ── Device list during non-idle states ──

    #[test]
    fn devices_updated_during_connecting_still_updates() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![
                dummy_device("TV1"),
                dummy_device("TV2"),
                dummy_device("TV3"),
            ]))
            .unwrap();
        core.process_backend_events();

        assert_eq!(core.idle_view.devices.len(), 3);
        assert!(matches!(core.state, AppState::Connecting { .. }));
    }

    #[test]
    fn devices_updated_during_streaming_still_updates() {
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
            .send(BackendEvent::DevicesUpdated(vec![dummy_device("NewTV")]))
            .unwrap();
        core.process_backend_events();

        assert_eq!(core.idle_view.devices.len(), 1);
        assert!(matches!(core.state, AppState::Streaming { .. }));
    }

    // ── Pairing failed clears connecting state properly ──

    #[test]
    fn pairing_failed_clears_connecting_state() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;
        core.idle_view.connecting_device = Some("TV".to_string());

        evt_tx
            .send(BackendEvent::PairingFailed("bad pin".to_string()))
            .unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.connecting_device.is_none());
        assert_eq!(core.idle_view.error.as_deref(), Some("bad pin"));
    }

    // ── Pairing success clears error from previous attempt ──

    #[test]
    fn pairing_success_clears_previous_error() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.idle_view.error = Some("previous error".to_string());
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };

        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::ModeSelect { .. }));
        assert!(core.idle_view.error.is_none());
    }

    // ── Device name propagation ──

    #[test]
    fn device_name_preserved_through_transitions() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "会议室投影".to_string(),
            started_at: Instant::now(),
        };

        evt_tx.send(BackendEvent::PairingSuccess).unwrap();
        core.process_backend_events();

        if let AppState::ModeSelect { device_name } = &core.state {
            assert_eq!(device_name, "会议室投影");
        } else {
            panic!("expected ModeSelect");
        }

        evt_tx.send(BackendEvent::StreamingStarted).unwrap();
        core.process_backend_events();

        if let AppState::Streaming { device_name, .. } = &core.state {
            assert_eq!(device_name, "会议室投影");
        } else {
            panic!("expected Streaming");
        }
    }

    // ── Connect timeout clears error properly ──

    #[test]
    fn connecting_timeout_sets_chinese_error_message() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now() - Duration::from_secs(11),
        };
        core.idle_view.connecting = true;
        core.idle_view.connecting_device = Some("TV".to_string());

        core.check_connecting_timeout();

        assert!(matches!(core.state, AppState::Idle));
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.connecting_device.is_none());
        assert_eq!(core.idle_view.error.as_deref(), Some("连接超时"));
    }

    // ── Initial idle view state ──

    #[test]
    fn initial_idle_view_state() {
        let (core, _evt_tx, _cmd_rx) = make_core();
        assert!(core.idle_view.devices.is_empty());
        assert!(core.idle_view.selected_device.is_none());
        assert!(core.idle_view.pin_input.is_empty());
        assert_eq!(core.idle_view.pin_cursor, 0);
        assert!(core.idle_view.error.is_none());
        assert!(!core.idle_view.connecting);
        assert!(core.idle_view.connecting_device.is_none());
    }

    // ── Devices replaced entirely (not appended) ──

    #[test]
    fn devices_updated_replaces_entire_list() {
        let (mut core, evt_tx, _cmd_rx) = make_core();

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![
                dummy_device("TV1"),
                dummy_device("TV2"),
            ]))
            .unwrap();
        core.process_backend_events();
        assert_eq!(core.idle_view.devices.len(), 2);

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![dummy_device("TV3")]))
            .unwrap();
        core.process_backend_events();
        assert_eq!(core.idle_view.devices.len(), 1);
        assert_eq!(core.idle_view.devices[0].name, "TV3");
    }

    // ── Empty device list update ──

    #[test]
    fn devices_updated_to_empty() {
        let (mut core, evt_tx, _cmd_rx) = make_core();

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![dummy_device("TV1")]))
            .unwrap();
        core.process_backend_events();
        assert_eq!(core.idle_view.devices.len(), 1);

        evt_tx
            .send(BackendEvent::DevicesUpdated(vec![]))
            .unwrap();
        core.process_backend_events();
        assert!(core.idle_view.devices.is_empty());
    }

    // ── Pairing timeout from PairingTimeout event ──

    #[test]
    fn pairing_timeout_sets_chinese_error() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.state = AppState::Connecting {
            device_name: "TV".to_string(),
            started_at: Instant::now(),
        };
        core.idle_view.connecting = true;

        evt_tx.send(BackendEvent::PairingTimeout).unwrap();
        core.process_backend_events();

        assert!(matches!(core.state, AppState::Idle));
        assert!(!core.idle_view.connecting);
        assert_eq!(core.idle_view.error.as_deref(), Some("连接超时"));
    }

    // ── Visibility state ──

    #[test]
    fn initial_visibility_is_true() {
        let (core, _evt_tx, _cmd_rx) = make_core();
        assert!(core.visible);
    }

    // ── PinVerifyState tests ──

    #[test]
    fn pin_verify_debounce_starts_on_6_digits() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.check_pin_verify();
        assert!(matches!(
            core.idle_view.pin_verify_state,
            PinVerifyState::Debouncing { .. }
        ));
    }

    #[test]
    fn pin_verify_resets_on_short_pin() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.check_pin_verify();
        core.idle_view.pin_input = "12345".to_string();
        core.check_pin_verify();
        assert!(matches!(
            core.idle_view.pin_verify_state,
            PinVerifyState::Idle
        ));
    }

    #[test]
    fn pin_matched_event_updates_state() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Verifying { pin: "123456".to_string() };
        let addr: SocketAddr = "192.168.1.100:9000".parse().unwrap();
        evt_tx
            .send(BackendEvent::PinMatched {
                device_name: "客厅电视".to_string(),
                addr,
            })
            .unwrap();
        core.process_backend_events();
        assert!(matches!(
            core.idle_view.pin_verify_state,
            PinVerifyState::Matched { ref device_name, .. } if device_name == "客厅电视"
        ));
    }

    #[test]
    fn pin_not_found_event_updates_state() {
        let (mut core, evt_tx, _cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Verifying { pin: "123456".to_string() };
        evt_tx.send(BackendEvent::PinNotFound).unwrap();
        core.process_backend_events();
        assert!(matches!(
            core.idle_view.pin_verify_state,
            PinVerifyState::NotFound { .. }
        ));
    }

    #[test]
    fn connect_matched_sends_command() {
        let (mut core, _evt_tx, cmd_rx) = make_core();
        let addr: SocketAddr = "192.168.1.100:9000".parse().unwrap();
        core.idle_view.pin_input = "123456".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Matched {
            device_name: "客厅电视".to_string(),
            addr,
            pin: "123456".to_string(),
        };
        core.handle_connect_matched();
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, UiCommand::Connect { .. }));
        assert!(matches!(core.state, AppState::Connecting { .. }));
        assert!(core.idle_view.connecting);
    }

    #[test]
    fn pin_verify_no_refire_when_pin_unchanged() {
        let (mut core, _evt_tx, cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Matched {
            device_name: "TV".to_string(),
            addr: "192.168.1.1:9000".parse().unwrap(),
            pin: "123456".to_string(),
        };
        core.check_pin_verify();
        // Should stay in Matched — no re-debounce
        assert!(matches!(core.idle_view.pin_verify_state, PinVerifyState::Matched { .. }));
        assert!(cmd_rx.try_recv().is_err());
    }

    #[test]
    fn pin_verify_refires_when_pin_changed() {
        let (mut core, _evt_tx, _cmd_rx) = make_core();
        core.idle_view.pin_input = "654321".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Matched {
            device_name: "TV".to_string(),
            addr: "192.168.1.1:9000".parse().unwrap(),
            pin: "123456".to_string(),
        };
        core.check_pin_verify();
        // Should re-debounce because PIN changed
        assert!(matches!(core.idle_view.pin_verify_state, PinVerifyState::Debouncing { .. }));
    }

    #[test]
    fn pin_verify_debounce_fires_after_300ms() {
        let (mut core, _evt_tx, cmd_rx) = make_core();
        core.idle_view.pin_input = "123456".to_string();
        core.idle_view.pin_verify_state = PinVerifyState::Debouncing {
            since: Instant::now() - Duration::from_millis(301),
            pin: "123456".to_string(),
        };
        core.check_pin_verify();
        assert!(matches!(core.idle_view.pin_verify_state, PinVerifyState::Verifying { .. }));
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, UiCommand::VerifyPin { .. }));
    }
}
