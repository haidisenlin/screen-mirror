use eframe::egui::{self, Button, CornerRadius, Frame, RichText, Ui, Vec2};

use crate::ui::theme::*;

pub enum PausedAction {
    None,
    Resume,
    Disconnect,
}

pub fn render(ui: &mut Ui, device_name: &str) -> PausedAction {
    let mut action = PausedAction::None;

    // Header with yellow status dot
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 4.0, COLOR_WARNING);
        ui.add_space(2.0);
        ui.label(RichText::new(APP_NAME).strong().size(15.0).color(COLOR_TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(PADDING);
            Frame::new()
                .fill(COLOR_WARNING_LIGHT)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(RichText::new("已暂停").size(11.0).color(COLOR_WARNING).strong());
                });
        });
    });
    ui.add_space(8.0);

    // Connected device card
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        Frame::new()
            .fill(COLOR_BG_CARD)
            .corner_radius(CornerRadius::same(CARD_ROUNDING))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(PANEL_WIDTH - 28.0);
                Frame::new()
                    .fill(COLOR_BG_WHITE)
                    .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .shadow(egui::epaint::Shadow {
                        spread: 0,
                        blur: 6,
                        offset: [0, 1],
                        color: Color32::from_black_alpha(15),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            Frame::new()
                                .fill(COLOR_WARNING)
                                .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("📺").size(14.0).color(COLOR_BG_WHITE));
                                });
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(device_name)
                                        .size(13.0)
                                        .strong()
                                        .color(COLOR_TEXT),
                                );
                                ui.label(
                                    RichText::new("全屏镜像").size(11.0).color(COLOR_MUTED),
                                );
                            });
                        });
                    });
            });
    });

    // Paused icon & message
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("⏸").size(32.0));
        ui.add_space(8.0);
        ui.label(RichText::new("投屏已暂停").size(13.0).color(COLOR_TEXT_SECONDARY));
        ui.add_space(4.0);
        ui.label(RichText::new("接收端显示最后一帧画面").size(11.0).color(COLOR_MUTED));
    });
    ui.add_space(24.0);

    // Action buttons
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let half_width = (PANEL_WIDTH - PADDING * 2.0 - SPACING - 4.0) / 2.0;

        let resume_btn = Button::new(RichText::new("▶ 恢复").size(13.0).color(COLOR_BG_WHITE).strong())
            .fill(COLOR_BRAND)
            .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
            .min_size(Vec2::new(half_width, BUTTON_HEIGHT));
        if ui.add(resume_btn).clicked() {
            action = PausedAction::Resume;
        }

        ui.add_space(SPACING);

        let stop_btn = Button::new(RichText::new("⏹ 断开").size(13.0).color(COLOR_ERROR))
            .fill(COLOR_ERROR_LIGHT)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(254, 202, 202)))
            .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
            .min_size(Vec2::new(half_width, BUTTON_HEIGHT));
        if ui.add(stop_btn).clicked() {
            action = PausedAction::Disconnect;
        }
    });

    ui.add_space(PADDING);
    action
}
