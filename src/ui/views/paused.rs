use eframe::egui::{Button, RichText, Ui, Vec2};

use crate::ui::theme::*;

pub enum PausedAction {
    None,
    Resume,
    Disconnect,
}

pub fn render(ui: &mut Ui, device_name: &str) -> PausedAction {
    let mut action = PausedAction::None;

    ui.add_space(PADDING);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new(format!("📺 {device_name} · 已暂停")).strong().color(COLOR_WARNING));
    });
    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SEPARATOR_SPACING);

    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new("接收端看到最后一帧画面").color(COLOR_MUTED));
    });

    ui.add_space(SEPARATOR_SPACING * 2.0);

    // Action buttons
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let half_width = (ui.available_width() - PADDING * 2.0 - SPACING) / 2.0;

        if ui.add(Button::new("▶ 恢复").min_size(Vec2::new(half_width, BUTTON_HEIGHT))).clicked() {
            action = PausedAction::Resume;
        }
        ui.add_space(SPACING);
        if ui.add(Button::new("⏹ 断开").min_size(Vec2::new(half_width, BUTTON_HEIGHT))).clicked() {
            action = PausedAction::Disconnect;
        }
    });

    ui.add_space(PADDING);
    action
}
