use eframe::egui::{self, Color32, Pos2, Rect};

const NODE_COUNT: usize = 15;
const CONNECT_DIST: f32 = 140.0;

pub struct AiBackground {
    start_time: std::time::Instant,
}

impl AiBackground {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
        }
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    pub fn paint(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter_at(rect);
        let t = self.start_time.elapsed().as_secs_f32();

        self.paint_glow_orbs(&painter, rect, t);
        self.paint_network(&painter, rect, t);
        self.paint_rising_particles(&painter, rect, t);

        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn paint_glow_orbs(&self, painter: &egui::Painter, rect: Rect, t: f32) {
        let orbs = [
            (rect.right_top() + egui::vec2(-50.0, 80.0), 100.0, 0.0),
            (rect.left_bottom() + egui::vec2(60.0, -100.0), 90.0, 1.8),
        ];

        for (center, base_radius, phase) in orbs {
            let breath = (t * 0.3 + phase).sin() * 0.15 + 0.85;
            let radius = base_radius * breath;
            for ring in 0..6 {
                let frac = ring as f32 / 6.0;
                let r = radius * (1.0 - frac * 0.8);
                let alpha = ((1.0 - frac) * 12.0 * breath) as u8;
                painter.circle_filled(
                    center,
                    r,
                    Color32::from_rgba_premultiplied(28, 100, 242, alpha),
                );
            }
        }
    }

    fn node_position(&self, index: usize, rect: Rect, t: f32) -> Pos2 {
        let seed = index as f32 * 2.618;
        let base_x = ((seed * 7.3).sin() * 0.42 + 0.5) * rect.width();
        let base_y = ((seed * 3.7).cos() * 0.42 + 0.5) * rect.height();
        let dx = (t * 0.25 + seed * 1.3).sin() * 18.0;
        let dy = (t * 0.2 + seed * 2.1).cos() * 14.0;
        Pos2::new(rect.left() + base_x + dx, rect.top() + base_y + dy)
    }

    fn paint_network(&self, painter: &egui::Painter, rect: Rect, t: f32) {
        let positions: Vec<Pos2> = (0..NODE_COUNT)
            .map(|i| self.node_position(i, rect, t))
            .collect();

        // Connections
        for i in 0..NODE_COUNT {
            for j in (i + 1)..NODE_COUNT {
                let dist = positions[i].distance(positions[j]);
                if dist < CONNECT_DIST {
                    let strength = 1.0 - dist / CONNECT_DIST;
                    let pulse = (t * 0.8 + i as f32 * 0.3).sin() * 0.3 + 0.7;
                    let alpha = (strength * 45.0 * pulse) as u8;
                    painter.line_segment(
                        [positions[i], positions[j]],
                        egui::Stroke::new(
                            0.6 + strength * 0.6,
                            Color32::from_rgba_premultiplied(28, 100, 242, alpha),
                        ),
                    );

                    // Traveling pulse dot
                    if strength > 0.45 {
                        let travel = ((t * 1.5 + i as f32 * 0.7 + j as f32 * 0.3) % 4.0) / 4.0;
                        let pulse_pos = Pos2::new(
                            positions[i].x + (positions[j].x - positions[i].x) * travel,
                            positions[i].y + (positions[j].y - positions[i].y) * travel,
                        );
                        painter.circle_filled(
                            pulse_pos,
                            1.8,
                            Color32::from_rgba_premultiplied(100, 160, 255, (strength * 90.0) as u8),
                        );
                    }
                }
            }
        }

        // Nodes
        for (i, pos) in positions.iter().enumerate() {
            let pulse = (t * 0.8 + i as f32 * 0.7).sin() * 0.3 + 0.7;
            let alpha = (pulse * 100.0) as u8;
            let radius = 2.5 * (0.8 + pulse * 0.3);

            // Glow
            painter.circle_filled(
                *pos,
                radius * 3.0,
                Color32::from_rgba_premultiplied(28, 100, 242, alpha / 6),
            );
            // Core
            painter.circle_filled(
                *pos,
                radius,
                Color32::from_rgba_premultiplied(60, 130, 255, alpha),
            );
            // Bright center
            painter.circle_filled(
                *pos,
                radius * 0.35,
                Color32::from_rgba_premultiplied(180, 210, 255, alpha),
            );
        }
    }

    fn paint_rising_particles(&self, painter: &egui::Painter, rect: Rect, t: f32) {
        for i in 0..12 {
            let seed = i as f32 * std::f32::consts::PI;
            let x_base = ((seed * 2.3).sin() * 0.4 + 0.5) * rect.width() + rect.left();
            let cycle = 8.0 + (seed * 0.7).sin() * 3.0;
            let phase = (t * 0.3 + seed) % cycle;
            let y_frac = phase / cycle;
            let y = rect.bottom() - y_frac * rect.height() * 1.1;

            let fade = if y_frac < 0.1 {
                y_frac / 0.1
            } else if y_frac > 0.8 {
                (1.0 - y_frac) / 0.2
            } else {
                1.0
            };

            let x_drift = (t * 0.5 + seed * 1.7).sin() * 8.0;
            let alpha = (fade * 40.0) as u8;
            if alpha < 3 {
                continue;
            }

            let size = 1.5 + (seed * 0.5).sin().abs() * 1.5;
            painter.circle_filled(
                Pos2::new(x_base + x_drift, y),
                size,
                Color32::from_rgba_premultiplied(100, 160, 255, alpha),
            );
        }
    }
}
