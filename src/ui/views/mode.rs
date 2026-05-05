use eframe::egui::{self, Button, CornerRadius, Frame, RichText, Ui, Vec2};

use crate::ui::messages::{CaptureMode, WindowInfo};
use crate::ui::theme::*;

pub enum ModeAction {
    None,
    Start(CaptureMode),
    RequestWindowList,
    StartRegionSelect,
}

pub fn render(ui: &mut Ui, device_name: &str, window_list: &[WindowInfo]) -> ModeAction {
    let mut action = ModeAction::None;

    // Header
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(
            RichText::new(APP_NAME)
                .strong()
                .size(15.0)
                .color(COLOR_TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(PADDING);
            Frame::new()
                .fill(COLOR_BRAND_LIGHT)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("已连接")
                            .size(11.0)
                            .color(COLOR_BRAND)
                            .strong(),
                    );
                });
        });
    });
    ui.add_space(8.0);

    // Connected device
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
                                .fill(COLOR_BRAND)
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
                                    RichText::new("选择投屏模式").size(11.0).color(COLOR_MUTED),
                                );
                            });
                        });
                    });
            });
    });

    ui.add_space(16.0);

    // Mode cards
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(
            RichText::new("投屏模式")
                .size(11.0)
                .color(COLOR_MUTED)
                .strong(),
        );
    });
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let card_width = (PANEL_WIDTH - 28.0 - SPACING * 2.0) / 3.0;

        // Full screen - enabled
        let btn = Button::new(
            RichText::new("🖥️\n全屏镜像")
                .size(12.0)
                .color(COLOR_TEXT)
                .strong(),
        )
        .fill(COLOR_BG_CARD)
        .stroke(egui::Stroke::new(1.5, COLOR_BRAND))
        .corner_radius(CornerRadius::same(CARD_ROUNDING))
        .min_size(Vec2::new(card_width, 70.0));
        if ui.add(btn).clicked() {
            action = ModeAction::Start(CaptureMode::FullScreen);
        }

        ui.add_space(SPACING);

        // Window select - enabled
        let btn = Button::new(
            RichText::new("🪟\n选择窗口")
                .size(12.0)
                .color(COLOR_TEXT)
                .strong(),
        )
        .fill(COLOR_BG_CARD)
        .stroke(egui::Stroke::new(1.5, COLOR_BRAND))
        .corner_radius(CornerRadius::same(CARD_ROUNDING))
        .min_size(Vec2::new(card_width, 70.0));
        if ui.add(btn).clicked() {
            action = ModeAction::RequestWindowList;
        }

        ui.add_space(SPACING);

        // Custom region - enabled
        let btn = Button::new(
            RichText::new("⬜\n自定义区域")
                .size(12.0)
                .color(COLOR_TEXT)
                .strong(),
        )
        .fill(COLOR_BG_CARD)
        .stroke(egui::Stroke::new(1.5, COLOR_BRAND))
        .corner_radius(CornerRadius::same(CARD_ROUNDING))
        .min_size(Vec2::new(card_width, 70.0));
        if ui.add(btn).clicked() {
            action = ModeAction::StartRegionSelect;
        }
    });

    ui.add_space(PADDING);

    // Window list (shown when populated)
    if !window_list.is_empty() {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(
                RichText::new("选择窗口")
                    .size(11.0)
                    .color(COLOR_MUTED)
                    .strong(),
            );
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for win in window_list {
                    ui.horizontal(|ui| {
                        ui.add_space(PADDING);
                        let label = if win.app_name.is_empty() {
                            win.title.clone()
                        } else {
                            format!("{} — {}", win.app_name, win.title)
                        };
                        let btn = Button::new(RichText::new(&label).size(12.0).color(COLOR_TEXT))
                            .fill(COLOR_BG_WHITE)
                            .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                            .min_size(Vec2::new(PANEL_WIDTH - 32.0, 32.0));
                        if ui.add(btn).clicked() {
                            action = ModeAction::Start(CaptureMode::Window {
                                id: win.id,
                                title: win.title.clone(),
                            });
                        }
                    });
                    ui.add_space(2.0);
                }
            });
    }

    action
}
