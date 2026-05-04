use eframe::egui::{self, Button, RichText, ScrollArea, Sense, TextEdit, Ui, Vec2};

use crate::discovery::browser::DiscoveredReceiver;
use crate::ui::theme::*;

pub struct IdleViewState {
    pub devices: Vec<DiscoveredReceiver>,
    pub selected_device: Option<usize>,
    pub pin_input: String,
    pub error: Option<String>,
    pub connecting: bool,
    pub connecting_device: Option<String>,
}

pub enum IdleAction {
    None,
    Connect { device_index: usize, pin: String },
}

pub fn render(ui: &mut Ui, state: &mut IdleViewState) -> IdleAction {
    let mut action = IdleAction::None;

    ui.add_space(PADDING);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new("screen-mirror").strong().size(14.0));
    });
    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SPACING);

    if state.connecting {
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.spinner();
            ui.label(format!(
                "正在连接 {}...",
                state.connecting_device.as_deref().unwrap_or("")
            ));
        });
        ui.add_space(PADDING);
        return action;
    }

    // Device discovery status
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        if state.devices.is_empty() {
            ui.label(RichText::new("正在搜索设备...").color(COLOR_MUTED));
        } else {
            ui.label(RichText::new(format!("发现 {} 台设备", state.devices.len())).color(COLOR_MUTED));
        }
    });
    ui.add_space(SPACING);

    // Device list
    ScrollArea::vertical()
        .max_height(150.0)
        .show(ui, |ui| {
            for (idx, device) in state.devices.iter().enumerate() {
                let selected = state.selected_device == Some(idx);
                let response = ui
                    .horizontal(|ui| {
                        ui.add_space(PADDING);
                        if selected {
                            let rect = ui.available_rect_before_wrap();
                            ui.painter().rect_filled(rect, 4.0, COLOR_ACCENT.linear_multiply(0.2));
                        }
                        ui.label("📺");
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&device.name).strong());
                            ui.label(RichText::new(device.addr.to_string()).small().color(COLOR_MUTED));
                        });
                    })
                    .response;
                if response.interact(Sense::click()).clicked() {
                    state.selected_device = Some(idx);
                }
                ui.add_space(2.0);
            }
        });

    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SPACING);

    // PIN input
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label("投屏码:");
        ui.add(
            TextEdit::singleline(&mut state.pin_input)
                .desired_width(100.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("6位数字"),
        );
    });
    // Filter non-digits and limit to 6
    state.pin_input.retain(|c| c.is_ascii_digit());
    if state.pin_input.len() > 6 {
        state.pin_input.truncate(6);
    }

    ui.add_space(SPACING);

    // Connect button
    let can_connect = state.selected_device.is_some() && state.pin_input.len() == 6;
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let btn = Button::new("连 接")
            .min_size(Vec2::new(ui.available_width() - PADDING * 2.0, BUTTON_HEIGHT));
        if ui.add_enabled(can_connect, btn).clicked()
            && let Some(idx) = state.selected_device
        {
            action = IdleAction::Connect {
                device_index: idx,
                pin: state.pin_input.clone(),
            };
        }
    });

    // Error display
    if let Some(error) = &state.error {
        ui.add_space(SPACING);
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(RichText::new(format!("❌ {error}")).color(COLOR_ERROR));
        });
    }

    ui.add_space(PADDING);
    action
}
