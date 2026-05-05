use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

pub struct SelectedRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

enum DragState {
    Idle,
    Dragging { start: Pos2, current: Pos2 },
    Selected { rect: Rect },
}

pub enum RegionAction {
    None,
    Confirmed(SelectedRegion),
    Cancelled,
}

pub struct RegionSelectView {
    texture: Option<egui::TextureHandle>,
    image_size: [u32; 2],
    rgba_data: Vec<u8>,
    state: DragState,
}

impl RegionSelectView {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            texture: None,
            image_size: [width, height],
            rgba_data: rgba,
            state: DragState::Idle,
        }
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_none() {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [self.image_size[0] as usize, self.image_size[1] as usize],
                &self.rgba_data,
            );
            self.texture =
                Some(ctx.load_texture("screenshot", image, egui::TextureOptions::LINEAR));
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui) -> RegionAction {
        self.ensure_texture(ui.ctx());

        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());

        // Draw screenshot as background
        if let Some(tex) = &self.texture {
            ui.painter().image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Draw dark overlay
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_black_alpha(100));

        // Handle keyboard
        let mut action = RegionAction::None;
        ui.input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = event
                {
                    match key {
                        egui::Key::Escape => {
                            action = RegionAction::Cancelled;
                        }
                        egui::Key::Enter => {
                            // handled below after matching state
                        }
                        _ => {}
                    }
                }
            }
        });

        // Check Enter confirmation
        if matches!(action, RegionAction::None)
            && let DragState::Selected { rect: sel_rect } = &self.state
        {
            let enter_pressed = ui.input(|input| {
                input.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
            if enter_pressed {
                let scale_x = self.image_size[0] as f64 / available.x as f64;
                let scale_y = self.image_size[1] as f64 / available.y as f64;
                let rel = Rect::from_min_max(
                    Pos2::new(sel_rect.min.x - rect.min.x, sel_rect.min.y - rect.min.y),
                    Pos2::new(sel_rect.max.x - rect.min.x, sel_rect.max.y - rect.min.y),
                );
                action = RegionAction::Confirmed(SelectedRegion {
                    x: rel.min.x as f64 * scale_x,
                    y: rel.min.y as f64 * scale_y,
                    width: rel.width() as f64 * scale_x,
                    height: rel.height() as f64 * scale_y,
                });
            }
        }

        if matches!(action, RegionAction::Cancelled) || matches!(action, RegionAction::Confirmed(_))
        {
            return action;
        }

        // Handle drag
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.state = DragState::Dragging {
                    start: pos,
                    current: pos,
                };
            }
        } else if response.dragged()
            && let DragState::Dragging { current, .. } = &mut self.state
            && let Some(pos) = response.interact_pointer_pos()
        {
            *current = pos;
        } else if response.drag_stopped()
            && let DragState::Dragging { start, current } = &self.state
        {
            let sel = Rect::from_two_pos(*start, *current);
            if sel.width() > 10.0 && sel.height() > 10.0 {
                self.state = DragState::Selected { rect: sel };
            } else {
                self.state = DragState::Idle;
            }
        }

        // Draw selection rectangle
        let selection_rect = match &self.state {
            DragState::Dragging { start, current } => {
                Some((Rect::from_two_pos(*start, *current), false))
            }
            DragState::Selected { rect: sel_rect } => Some((*sel_rect, true)),
            DragState::Idle => None,
        };

        if let Some((sel_rect, is_selected)) = selection_rect {
            // Draw the screenshot in the selected region to "clear" the overlay
            if let Some(tex) = &self.texture {
                // Compute UV coordinates for the selected region
                let uv_min = Pos2::new(
                    (sel_rect.min.x - rect.min.x) / rect.width(),
                    (sel_rect.min.y - rect.min.y) / rect.height(),
                );
                let uv_max = Pos2::new(
                    (sel_rect.max.x - rect.min.x) / rect.width(),
                    (sel_rect.max.y - rect.min.y) / rect.height(),
                );
                ui.painter().image(
                    tex.id(),
                    sel_rect,
                    Rect::from_min_max(uv_min, uv_max),
                    Color32::WHITE,
                );
            }

            // Draw border
            let stroke = if is_selected {
                Stroke::new(2.0, Color32::from_rgb(0, 200, 0))
            } else {
                Stroke::new(2.0, Color32::from_rgb(50, 130, 246))
            };
            ui.painter()
                .rect_stroke(sel_rect, 0.0, stroke, StrokeKind::Outside);

            // Size label
            let scale_x = self.image_size[0] as f32 / available.x;
            let scale_y = self.image_size[1] as f32 / available.y;
            let px_w = ((sel_rect.width()) * scale_x) as u32;
            let px_h = ((sel_rect.height()) * scale_y) as u32;
            let label = format!("{}x{}", px_w, px_h);
            ui.painter().text(
                sel_rect.left_bottom() + Vec2::new(4.0, 4.0),
                egui::Align2::LEFT_TOP,
                &label,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );

            // Hint text in Selected state
            if is_selected {
                ui.painter().text(
                    sel_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Enter确认 / Esc取消",
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
        }

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_region_fields() {
        let r = SelectedRegion {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
        };
        assert_eq!(r.x, 10.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 800.0);
        assert_eq!(r.height, 600.0);
    }

    #[test]
    fn region_select_view_new() {
        let rgba = vec![0u8; 4 * 100 * 100];
        let view = RegionSelectView::new(rgba, 100, 100);
        assert_eq!(view.image_size, [100, 100]);
        assert!(view.texture.is_none());
    }
}
