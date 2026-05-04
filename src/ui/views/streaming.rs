use eframe::egui::{Button, RichText, Ui, Vec2};

use crate::ui::messages::StreamStats;
use crate::ui::theme::*;

pub enum StreamingAction {
    None,
    Pause,
    Disconnect,
}

pub fn render(ui: &mut Ui, device_name: &str, stats: &StreamStats) -> StreamingAction {
    let mut action = StreamingAction::None;

    ui.add_space(PADDING);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new(format!("📺 {device_name} · 投屏中")).strong().color(COLOR_SUCCESS));
    });
    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SEPARATOR_SPACING);

    // Stats grid
    let stats_items = [
        ("分辨率", format!("{} × {}", stats.resolution_w, stats.resolution_h)),
        ("帧率", format!("{:.0} fps", stats.fps)),
        ("码率", format_bitrate(stats.bitrate_bps)),
        ("延迟", format!("{:.1} ms", stats.latency_ms)),
        ("丢包", format!("{:.1}%", stats.packet_loss_pct)),
    ];

    for (label, value) in &stats_items {
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(RichText::new(*label).color(COLOR_MUTED));
            ui.add_space(STAT_LABEL_WIDTH - ui.min_rect().width());
            ui.label(value.as_str());
        });
        ui.add_space(2.0);
    }

    ui.add_space(SPACING);
    ui.separator();
    ui.add_space(SPACING);

    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(RichText::new("模式: 全屏镜像").color(COLOR_MUTED));
    });

    ui.add_space(SEPARATOR_SPACING);
    ui.separator();
    ui.add_space(SEPARATOR_SPACING);

    // Action buttons
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let half_width = (ui.available_width() - PADDING * 2.0 - SPACING) / 2.0;

        if ui.add(Button::new("⏸ 暂停").min_size(Vec2::new(half_width, BUTTON_HEIGHT))).clicked() {
            action = StreamingAction::Pause;
        }
        ui.add_space(SPACING);
        if ui.add(Button::new("⏹ 断开").min_size(Vec2::new(half_width, BUTTON_HEIGHT))).clicked() {
            action = StreamingAction::Disconnect;
        }
    });

    ui.add_space(PADDING);
    action
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
