use eframe::egui::{Button, RichText, Ui, Vec2};

use crate::ui::messages::CaptureMode;
use crate::ui::theme::*;

pub enum ModeAction {
    None,
    Start(CaptureMode),
}

pub fn render(ui: &mut Ui, device_name: &str) -> ModeAction {
    let mut action = ModeAction::None;

    ui.add_space(PADDING);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new("选择投屏内容").strong().size(14.0));
    });
    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SEPARATOR_SPACING);

    // Mode cards
    ui.horizontal_wrapped(|ui| {
        ui.add_space(PADDING);

        // Full screen - enabled
        let btn = Button::new(RichText::new("🖥️\n全屏镜像").size(13.0))
            .min_size(Vec2::new(90.0, 70.0));
        if ui.add(btn).clicked() {
            action = ModeAction::Start(CaptureMode::FullScreen);
        }

        ui.add_space(SPACING);

        // Window select - disabled
        let btn = Button::new(RichText::new("🪟\n选择窗口\n(即将推出)").size(11.0).color(COLOR_MUTED))
            .min_size(Vec2::new(90.0, 70.0));
        ui.add_enabled(false, btn);

        ui.add_space(SPACING);

        // Custom region - disabled
        let btn = Button::new(RichText::new("⬜\n自定义区域\n(即将推出)").size(11.0).color(COLOR_MUTED))
            .min_size(Vec2::new(90.0, 70.0));
        ui.add_enabled(false, btn);
    });

    ui.add_space(SEPARATOR_SPACING);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new(format!("目标: {device_name}")).color(COLOR_MUTED));
    });

    ui.add_space(PADDING);
    action
}
