use eframe::egui::{self, Button, CornerRadius, Frame, RichText, Ui, Vec2};

use crate::ui::messages::StreamStats;
use crate::ui::theme::*;

pub enum StreamingAction {
    None,
    Pause,
    Disconnect,
}

pub fn render(ui: &mut Ui, device_name: &str, stats: &StreamStats) -> StreamingAction {
    let mut action = StreamingAction::None;

    // Header with green status dot
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 4.0, COLOR_SUCCESS);
        ui.add_space(2.0);
        ui.label(RichText::new(APP_NAME).strong().size(15.0).color(COLOR_TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(PADDING);
            Frame::new()
                .fill(COLOR_SUCCESS_LIGHT)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(RichText::new("投屏中").size(11.0).color(COLOR_SUCCESS).strong());
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
                                .fill(COLOR_SUCCESS)
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

    ui.add_space(12.0);

    // Stats 2x2 grid
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let half = (PANEL_WIDTH - PADDING * 2.0 - SPACING - 4.0) / 2.0;
        ui.vertical(|ui| {
            ui.set_width(half);
            stat_card(ui, "分辨率", &format!("{}×{}", stats.resolution_w, stats.resolution_h));
            ui.add_space(6.0);
            stat_card(ui, "码率", &format_bitrate(stats.bitrate_bps));
        });
        ui.add_space(SPACING);
        ui.vertical(|ui| {
            ui.set_width(half);
            stat_card(ui, "帧率", &format!("{:.0} fps", stats.fps));
            ui.add_space(6.0);
            stat_card(ui, "延迟", &format!("{:.1} ms", stats.latency_ms));
        });
    });

    ui.add_space(14.0);

    // Action buttons
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let half_width = (PANEL_WIDTH - PADDING * 2.0 - SPACING - 4.0) / 2.0;

        let pause_btn = Button::new(RichText::new("⏸ 暂停").size(13.0).color(COLOR_TEXT_SECONDARY))
            .fill(COLOR_BG_CARD)
            .stroke(egui::Stroke::new(1.0, COLOR_BORDER))
            .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
            .min_size(Vec2::new(half_width, BUTTON_HEIGHT));
        if ui.add(pause_btn).clicked() {
            action = StreamingAction::Pause;
        }

        ui.add_space(SPACING);

        let stop_btn = Button::new(RichText::new("⏹ 断开").size(13.0).color(COLOR_ERROR))
            .fill(COLOR_ERROR_LIGHT)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(254, 202, 202)))
            .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
            .min_size(Vec2::new(half_width, BUTTON_HEIGHT));
        if ui.add(stop_btn).clicked() {
            action = StreamingAction::Disconnect;
        }
    });

    ui.add_space(PADDING);
    action
}

fn stat_card(ui: &mut Ui, label: &str, value: &str) {
    Frame::new()
        .fill(COLOR_BG_CARD)
        .corner_radius(CornerRadius::same(ITEM_ROUNDING))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(10.0).color(COLOR_MUTED));
            ui.add_space(2.0);
            ui.label(RichText::new(value).size(14.0).strong().color(COLOR_TEXT));
        });
}

fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.0} Kbps", bps as f64 / 1_000.0)
    } else {
        format!("{bps} bps")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bitrate_megabits() {
        assert_eq!(format_bitrate(18_200_000), "18.2 Mbps");
        assert_eq!(format_bitrate(1_000_000), "1.0 Mbps");
        assert_eq!(format_bitrate(40_000_000), "40.0 Mbps");
    }

    #[test]
    fn format_bitrate_kilobits() {
        assert_eq!(format_bitrate(128_000), "128 Kbps");
        assert_eq!(format_bitrate(1_000), "1 Kbps");
        assert_eq!(format_bitrate(999_999), "1000 Kbps");
    }

    #[test]
    fn format_bitrate_bits() {
        assert_eq!(format_bitrate(0), "0 bps");
        assert_eq!(format_bitrate(999), "999 bps");
        assert_eq!(format_bitrate(1), "1 bps");
    }
}
