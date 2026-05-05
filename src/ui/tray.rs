use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Streaming,
    Paused,
}

pub struct AppTray {
    _tray: TrayIcon,
    state: TrayState,
    icon_idle: Icon,
    icon_streaming: Icon,
    icon_paused: Icon,
    show_requested: Arc<AtomicBool>,
    quit_requested: Arc<AtomicBool>,
}

fn create_icon(r: u8, g: u8, b: u8) -> Icon {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = size as f32 / 2.0 - 1.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist <= radius {
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("valid icon data")
}

impl Default for AppTray {
    fn default() -> Self {
        Self::new()
    }
}

impl AppTray {
    pub fn new() -> Self {
        let icon_idle = create_icon(128, 128, 128);
        let icon_streaming = create_icon(39, 174, 96);
        let icon_paused = create_icon(243, 156, 18);

        let menu = Menu::new();
        let show_item = MenuItem::new("显示窗口", true, None);
        let quit_item = MenuItem::new("退出", true, None);
        let show_id = show_item.id().clone();
        let quit_id = quit_item.id().clone();
        menu.append(&show_item).unwrap();
        menu.append(&quit_item).unwrap();

        let show_requested = Arc::new(AtomicBool::new(false));
        let quit_requested = Arc::new(AtomicBool::new(false));

        // Set up event handlers via callbacks (works on macOS where receiver() doesn't)
        let show_flag = show_requested.clone();
        let quit_flag = quit_requested.clone();
        let show_id_clone = show_id.clone();
        let quit_id_clone = quit_id.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            eprintln!("[TRAY-CB] MenuEvent: {:?}", event.id());
            if *event.id() == show_id_clone {
                show_flag.store(true, Ordering::SeqCst);
            } else if *event.id() == quit_id_clone {
                quit_flag.store(true, Ordering::SeqCst);
            }
        }));

        let click_show = show_requested.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            eprintln!("[TRAY-CB] TrayIconEvent: {:?}", event);
            if let TrayIconEvent::Click { .. } = event {
                click_show.store(true, Ordering::SeqCst);
            }
        }));

        let tray = TrayIconBuilder::new()
            .with_icon(icon_idle.clone())
            .with_tooltip("舜宇投屏")
            .with_menu(Box::new(menu))
            .build()
            .expect("failed to create tray icon");

        Self {
            _tray: tray,
            state: TrayState::Idle,
            icon_idle,
            icon_streaming,
            icon_paused,
            show_requested,
            quit_requested,
        }
    }

    pub fn poll_show(&self) -> bool {
        self.show_requested.swap(false, Ordering::SeqCst)
    }

    pub fn poll_quit(&self) -> bool {
        self.quit_requested.swap(false, Ordering::SeqCst)
    }

    pub fn set_state(&mut self, state: TrayState) {
        if self.state == state {
            return;
        }
        self.state = state;
        let icon = match state {
            TrayState::Idle => &self.icon_idle,
            TrayState::Streaming => &self.icon_streaming,
            TrayState::Paused => &self.icon_paused,
        };
        let _ = self._tray.set_icon(Some(icon.clone()));
    }
}
