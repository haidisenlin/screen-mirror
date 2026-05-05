use std::net::SocketAddr;
use std::time::Instant;

use eframe::egui::{
    self, Button, CornerRadius, Frame, Pos2, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::discovery::browser::DiscoveredReceiver;
use crate::ui::theme::*;

#[derive(Debug)]
pub enum PinVerifyState {
    Idle,
    Debouncing {
        since: Instant,
        pin: String,
    },
    Verifying {
        pin: String,
    },
    Matched {
        device_name: String,
        addr: SocketAddr,
        pin: String,
    },
    NotFound {
        pin: String,
    },
}

impl Default for PinVerifyState {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct IdleViewState {
    pub devices: Vec<DiscoveredReceiver>,
    pub selected_device: Option<usize>,
    pub pin_input: String,
    pub pin_cursor: usize,
    pub pin_verify_state: PinVerifyState,
    pub error: Option<String>,
    pub connecting: bool,
    pub connecting_device: Option<String>,
}

pub enum IdleAction {
    None,
    Connect { device_index: usize, pin: String },
    ConnectMatched,
}

fn paint_cast_icon(ui: &mut Ui, center: Pos2, size: f32, color: Color32) {
    let p = ui.painter();
    let s = size;
    // Monitor body
    let monitor_w = s * 0.7;
    let monitor_h = s * 0.45;
    let top_left = Pos2::new(
        center.x - monitor_w / 2.0,
        center.y - monitor_h / 2.0 - s * 0.08,
    );
    let bottom_right = Pos2::new(
        center.x + monitor_w / 2.0,
        center.y + monitor_h / 2.0 - s * 0.08,
    );
    p.rect_stroke(
        egui::Rect::from_two_pos(top_left, bottom_right),
        CornerRadius::same(3),
        Stroke::new(2.0, color),
        StrokeKind::Outside,
    );
    // Stand
    let stand_y = bottom_right.y;
    p.line_segment(
        [
            Pos2::new(center.x, stand_y),
            Pos2::new(center.x, stand_y + s * 0.12),
        ],
        Stroke::new(2.0, color),
    );
    p.line_segment(
        [
            Pos2::new(center.x - s * 0.15, stand_y + s * 0.12),
            Pos2::new(center.x + s * 0.15, stand_y + s * 0.12),
        ],
        Stroke::new(2.0, color),
    );
    // Wireless arcs (right side)
    let arc_center = Pos2::new(top_left.x + s * 0.12, bottom_right.y - s * 0.10);
    for i in 1..=3 {
        let r = s * 0.06 * i as f32;
        let alpha = (200 - i as u8 * 40).max(60);
        let arc_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha);
        let segments = 12;
        for seg in 0..segments {
            let a1 = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::FRAC_PI_2 * seg as f32 / segments as f32;
            let a2 = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::FRAC_PI_2 * (seg + 1) as f32 / segments as f32;
            p.line_segment(
                [
                    Pos2::new(arc_center.x + r * a1.cos(), arc_center.y + r * a1.sin()),
                    Pos2::new(arc_center.x + r * a2.cos(), arc_center.y + r * a2.sin()),
                ],
                Stroke::new(1.5, arc_color),
            );
        }
    }
    p.circle_filled(arc_center, 2.0, color);
}

fn paint_brand_icon(ui: &mut Ui, center: Pos2, radius: f32) {
    let p = ui.painter();
    p.circle_filled(center, radius, COLOR_BRAND);
    // Inner ring
    p.circle_stroke(center, radius * 0.55, Stroke::new(1.5, Color32::WHITE));
    // Center dot
    p.circle_filled(center, radius * 0.2, Color32::WHITE);
}

pub fn render(ui: &mut Ui, state: &mut IdleViewState) -> IdleAction {
    let mut action = IdleAction::None;

    // Header
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        // Brand icon
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
        paint_brand_icon(ui, icon_rect.center(), 10.0);
        ui.add_space(6.0);
        ui.label(
            RichText::new(APP_NAME)
                .strong()
                .size(15.0)
                .color(COLOR_TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(PADDING);
            let count = state.devices.len();
            let (badge_text, badge_color, badge_bg) = if count == 0 {
                ("搜索中...".to_string(), COLOR_MUTED, COLOR_BG_CARD)
            } else {
                (
                    format!("{count} 台可用"),
                    COLOR_SUCCESS,
                    COLOR_SUCCESS_LIGHT,
                )
            };
            Frame::new()
                .fill(badge_bg)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if count == 0 {
                            // Pulsing dot for "searching"
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
                            let t = ui.input(|i| i.time) as f32;
                            let alpha = (t * 2.0).sin() * 0.4 + 0.6;
                            let dot_color = Color32::from_rgba_unmultiplied(
                                COLOR_BRAND.r(),
                                COLOR_BRAND.g(),
                                COLOR_BRAND.b(),
                                (alpha * 255.0) as u8,
                            );
                            ui.painter()
                                .circle_filled(dot_rect.center(), 3.0, dot_color);
                            ui.add_space(2.0);
                        } else {
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
                            ui.painter()
                                .circle_filled(dot_rect.center(), 3.0, COLOR_SUCCESS);
                            ui.add_space(2.0);
                        }
                        ui.label(RichText::new(badge_text).size(11.0).color(badge_color));
                    });
                });
        });
    });
    ui.add_space(4.0);

    // Separator line
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(PANEL_WIDTH - PADDING * 2.0, 1.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, COLOR_BORDER);
    });

    if state.connecting {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!(
                    "正在连接 {}...",
                    state.connecting_device.as_deref().unwrap_or("设备")
                ))
                .size(14.0)
                .color(COLOR_TEXT_SECONDARY),
            );
        });
        ui.add_space(PADDING);
        return action;
    }

    // Illustration: SVG-style cast icon
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
        paint_cast_icon(ui, icon_rect.center(), 56.0, COLOR_TEXT_SECONDARY);
        ui.add_space(12.0);
        ui.label(
            RichText::new("输入投屏码开始投屏")
                .size(14.0)
                .color(COLOR_TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("请查看接收端屏幕上的 6 位数字")
                .size(11.0)
                .color(COLOR_MUTED),
        );
    });
    ui.add_space(24.0);

    // PIN input label
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        ui.label(
            RichText::new("投屏码")
                .size(12.0)
                .strong()
                .color(COLOR_TEXT_SECONDARY),
        );
    });
    ui.add_space(6.0);

    // PIN input boxes
    let pin_focus_id = ui.id().with("pin_focus");
    state.pin_cursor = state.pin_cursor.min(5);

    let mut clicked_box: Option<usize> = None;
    let boxes_response = ui
        .horizontal(|ui| {
            ui.add_space(PADDING);
            let total_width = PANEL_WIDTH - PADDING * 2.0 - 4.0;
            let gap = 8.0;
            let box_width = (total_width - gap * 5.0) / 6.0;

            for i in 0..6 {
                let ch = state
                    .pin_input
                    .chars()
                    .nth(i)
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                let has_digit = !ch.is_empty();
                let is_cursor = state.pin_cursor == i;

                let (border, bg) = if is_cursor {
                    (Stroke::new(2.0, COLOR_BRAND), COLOR_BRAND_LIGHT)
                } else if has_digit {
                    (Stroke::new(1.0, COLOR_BORDER), COLOR_BG_WHITE)
                } else {
                    (Stroke::new(1.0, COLOR_BORDER_LIGHT), COLOR_BG_CARD)
                };

                let shadow = if has_digit && !is_cursor {
                    egui::epaint::Shadow {
                        spread: 0,
                        blur: 4,
                        offset: [0, 1],
                        color: Color32::from_black_alpha(10),
                    }
                } else {
                    egui::epaint::Shadow::NONE
                };

                let box_resp = Frame::new()
                    .fill(bg)
                    .stroke(border)
                    .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                    .shadow(shadow)
                    .show(ui, |ui| {
                        ui.set_width(box_width);
                        ui.set_height(48.0);
                        ui.centered_and_justified(|ui| {
                            if has_digit {
                                ui.label(
                                    RichText::new(&ch)
                                        .size(24.0)
                                        .family(egui::FontFamily::Monospace)
                                        .strong()
                                        .color(COLOR_TEXT),
                                );
                            } else if !is_cursor {
                                let center = ui.max_rect().center();
                                ui.painter().line_segment(
                                    [
                                        Pos2::new(center.x - 6.0, center.y + 8.0),
                                        Pos2::new(center.x + 6.0, center.y + 8.0),
                                    ],
                                    Stroke::new(1.5, COLOR_BORDER_LIGHT),
                                );
                            }
                        });
                    })
                    .response;

                let click_id = ui.id().with(("pin_box_click", i));
                let click_resp = ui.interact(box_resp.rect, click_id, Sense::click());
                if click_resp.clicked() {
                    clicked_box = Some(i);
                }

                if i < 5 {
                    ui.add_space(gap - ui.spacing().item_spacing.x);
                }
            }
        })
        .response;

    // Handle box click — allow jumping to any box
    if let Some(i) = clicked_box {
        state.pin_cursor = i;
        ui.memory_mut(|m| m.request_focus(pin_focus_id));
    }

    // Focusable overlay for keyboard capture (no click sense — boxes handle clicks)
    ui.interact(
        boxes_response.rect,
        pin_focus_id,
        Sense::focusable_noninteractive(),
    );
    if !ui.memory(|m| m.has_focus(pin_focus_id)) {
        ui.memory_mut(|m| m.request_focus(pin_focus_id));
    }

    // Capture keyboard input
    ui.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Text(text) => {
                    for ch in text.chars() {
                        if ch.is_ascii_digit() && state.pin_cursor < 6 {
                            let cursor = state.pin_cursor;
                            // Pad with zeros if cursor is beyond current length
                            while state.pin_input.len() <= cursor {
                                state.pin_input.push('0');
                            }
                            let bytes = unsafe { state.pin_input.as_bytes_mut() };
                            bytes[cursor] = ch as u8;
                            state.pin_cursor = (cursor + 1).min(5);
                        }
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    ..
                } => {
                    if state.pin_cursor > 0 && !state.pin_input.is_empty() {
                        let remove_at = state.pin_cursor.min(state.pin_input.len()) - 1;
                        state.pin_input.remove(remove_at);
                        state.pin_cursor = remove_at;
                    }
                }
                egui::Event::Key {
                    key: egui::Key::ArrowLeft,
                    pressed: true,
                    ..
                } => {
                    if state.pin_cursor > 0 {
                        state.pin_cursor -= 1;
                    }
                }
                egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    pressed: true,
                    ..
                } => {
                    state.pin_cursor = (state.pin_cursor + 1).min(state.pin_input.len().min(5));
                }
                _ => {}
            }
        }
    });

    ui.add_space(16.0);

    // Connect button
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let btn_width = PANEL_WIDTH - PADDING * 2.0 - 4.0;

        let (btn_text, text_color, fill, stroke, enabled) = match &state.pin_verify_state {
            PinVerifyState::Matched { device_name, .. } => (
                format!("连接到 {device_name}"),
                COLOR_BG_WHITE,
                COLOR_BRAND,
                Stroke::NONE,
                true,
            ),
            PinVerifyState::Verifying { .. } => (
                "匹配中...".to_string(),
                COLOR_MUTED,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
            PinVerifyState::NotFound { .. } => (
                "未找到设备".to_string(),
                COLOR_ERROR,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
            _ => (
                "开始投屏".to_string(),
                COLOR_MUTED,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
        };

        let btn = Button::new(
            RichText::new(&btn_text)
                .size(15.0)
                .strong()
                .color(text_color),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
        .min_size(Vec2::new(btn_width, BUTTON_HEIGHT));

        if ui.add_enabled(enabled, btn).clicked() {
            if matches!(state.pin_verify_state, PinVerifyState::Matched { .. }) {
                action = IdleAction::ConnectMatched;
            }
        }
    });

    ui.add_space(16.0);

    // Device list
    if !state.devices.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(
                RichText::new("可用设备")
                    .size(11.0)
                    .strong()
                    .color(COLOR_MUTED),
            );
        });
        ui.add_space(4.0);

        for (idx, device) in state.devices.iter().enumerate() {
            let is_selected = state.selected_device == Some(idx);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let card_width = PANEL_WIDTH - 28.0;
                let card_bg = if is_selected {
                    COLOR_BRAND_LIGHT
                } else {
                    COLOR_BG_CARD
                };
                let card_border = if is_selected {
                    Stroke::new(1.0, COLOR_BRAND)
                } else {
                    Stroke::NONE
                };

                let resp = Frame::new()
                    .fill(card_bg)
                    .stroke(card_border)
                    .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(card_width - 24.0);
                        ui.horizontal(|ui| {
                            // Device icon (monitor drawn with painter)
                            let (icon_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                            let ic = icon_rect.center();
                            let icon_color = if is_selected {
                                COLOR_BRAND
                            } else {
                                COLOR_TEXT_SECONDARY
                            };
                            ui.painter().rect_stroke(
                                egui::Rect::from_center_size(
                                    Pos2::new(ic.x, ic.y - 1.0),
                                    Vec2::new(14.0, 10.0),
                                ),
                                CornerRadius::same(1),
                                Stroke::new(1.5, icon_color),
                                StrokeKind::Outside,
                            );
                            ui.painter().line_segment(
                                [
                                    Pos2::new(ic.x - 4.0, ic.y + 6.0),
                                    Pos2::new(ic.x + 4.0, ic.y + 6.0),
                                ],
                                Stroke::new(1.5, icon_color),
                            );
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&device.name).size(12.0).strong().color(
                                    if is_selected {
                                        COLOR_BRAND_DARK
                                    } else {
                                        COLOR_TEXT
                                    },
                                ));
                                ui.label(
                                    RichText::new(device.addr.to_string())
                                        .size(10.0)
                                        .color(COLOR_MUTED),
                                );
                            });
                        });
                    })
                    .response;

                if resp.interact(Sense::click()).clicked() {
                    state.selected_device = Some(idx);
                }
            });
            ui.add_space(4.0);
        }
    }

    // Error display
    if let Some(error) = &state.error {
        ui.add_space(SPACING);
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            Frame::new()
                .fill(COLOR_ERROR_LIGHT)
                .corner_radius(CornerRadius::same(ITEM_ROUNDING))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(PANEL_WIDTH - PADDING * 2.0 - 28.0);
                    ui.label(
                        RichText::new(format!("✕ {error}"))
                            .size(12.0)
                            .color(COLOR_ERROR),
                    );
                });
        });
    }

    ui.add_space(PADDING);
    action
}
