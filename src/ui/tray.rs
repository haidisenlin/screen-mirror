use eframe::egui;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Streaming,
    Paused,
}

pub struct AppTray {
    tray: TrayIcon,
    state: TrayState,
    icon_idle: Icon,
    icon_streaming: Icon,
    icon_paused: Icon,
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

        let tray = TrayIconBuilder::new()
            .with_icon(icon_idle.clone())
            .with_tooltip("screen-mirror")
            .build()
            .expect("failed to create tray icon");

        Self {
            tray,
            state: TrayState::Idle,
            icon_idle,
            icon_streaming,
            icon_paused,
        }
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
        let _ = self.tray.set_icon(Some(icon.clone()));
    }
}

#[cfg(target_os = "macos")]
pub fn calculate_position(icon_rect: tray_icon::Rect) -> egui::Pos2 {
    use crate::ui::theme::PANEL_WIDTH;
    let x = icon_rect.position.x + icon_rect.size.width as f64 / 2.0 - PANEL_WIDTH as f64 / 2.0;
    let y = icon_rect.position.y + icon_rect.size.height as f64;
    egui::pos2(x as f32, y as f32)
}

#[cfg(target_os = "windows")]
pub fn calculate_position(icon_rect: tray_icon::Rect) -> egui::Pos2 {
    use crate::ui::theme::{PANEL_MAX_HEIGHT, PANEL_WIDTH};
    let x = icon_rect.position.x + icon_rect.size.width as f64 / 2.0 - PANEL_WIDTH as f64 / 2.0;
    let y = icon_rect.position.y - PANEL_MAX_HEIGHT as f64;
    egui::pos2(x as f32, y as f32)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn calculate_position(icon_rect: tray_icon::Rect) -> egui::Pos2 {
    use crate::ui::theme::PANEL_WIDTH;
    let x = icon_rect.position.x + icon_rect.size.width as f64 / 2.0 - PANEL_WIDTH as f64 / 2.0;
    let y = icon_rect.position.y + icon_rect.size.height as f64;
    egui::pos2(x as f32, y as f32)
}
