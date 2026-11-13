//! Shared scene for the renderer spikes: the glass plus one ring whose arc
//! animates 0 -> 62 % over 1.2 s, then stops requesting repaints.

use egui::{epaint::PathShape, Color32, LayerId, Pos2, Stroke};

pub const TITLE: &str = "Claude Limit Checker spike";
/// Logical screen position, over the busy test background.
pub const POSITION: [f32; 2] = [1200.0, 400.0];

const RING_RADIUS: f32 = 23.0;
const RING_WIDTH: f32 = 6.0;
const RING_TRACK: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 20);
const RING_ARC: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
const TARGET: f32 = 0.62;
const DURATION: f64 = 1.2;

#[derive(Default)]
pub struct Scene {
    start: Option<f64>,
}

impl Scene {
    pub fn draw(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let start = *self.start.get_or_insert(now);
        let t = ((now - start) / DURATION).clamp(0.0, 1.0) as f32;
        let eased = 1.0 - (1.0 - t).powi(3);

        let painter = ctx.layer_painter(LayerId::background());
        let rect = ctx.content_rect();
        limitrings_app::ui::draw::glass(&painter, rect, false);

        let center = Pos2::new(rect.left() + 50.0, rect.center().y);
        painter.circle_stroke(center, RING_RADIUS, Stroke::new(RING_WIDTH, RING_TRACK));
        let frac = TARGET * eased;
        if frac > 0.0 {
            painter.add(PathShape::line(
                arc_points(center, RING_RADIUS, frac),
                Stroke::new(RING_WIDTH, RING_ARC),
            ));
        }

        if t < 1.0 {
            ctx.request_repaint();
        }
    }
}

/// Points of a clockwise arc starting at 12 o'clock covering `frac` of the circle.
fn arc_points(center: Pos2, radius: f32, frac: f32) -> Vec<Pos2> {
    let sweep = frac * std::f32::consts::TAU;
    let n = ((sweep * radius / 2.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + sweep * i as f32 / n as f32;
            center + radius * egui::vec2(a.cos(), a.sin())
        })
        .collect()
}
