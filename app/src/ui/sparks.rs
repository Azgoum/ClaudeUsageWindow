//! The surprise reset's gold glow and sparks (port of `.tauri .wd .glow`, `Sparks.tsx`).
//!
//! Both stay on the glass (rule R13): the glow follows the rounded border on the inside, and a
//! spark fades out as it crosses the rounded edge (egui clips to plain rects only).

use egui::{vec2, Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind};

use super::colors;
use super::draw::{self, GLASS_RADIUS};
use super::resets::SparkDot;

/// `.wd .glow { box-shadow: inset 0 0 0 1.5px …, inset 0 0 26px … }`.
const GLOW_RING_W: f32 = 1.5;
const GLOW_BLUR: f32 = 26.0;
/// `.wd .spark { width: 5px; height: 5px; border-radius: 50% }`.
const SPARK_R: f32 = 2.5;

/// How much of a dot of radius `r` at `c` lies on the rounded glass `rect` (0..=1), from its
/// center's distance to the rounded edge.
pub fn coverage(rect: Rect, c: egui::Pos2, r: f32) -> f32 {
    let d = draw::rrect_sdf(rect, GLASS_RADIUS, c);
    ((r - d) / (2.0 * r)).clamp(0.0, 1.0)
}

/// The gold glow over the whole widget box `rect`, `alpha` 0..=1 (`glow_alpha`).
pub fn paint_glow(p: &Painter, rect: Rect, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    draw::inner_glow(
        p,
        rect,
        GLASS_RADIUS,
        colors::GLOW_INNER.gamma_multiply(alpha),
        GLOW_BLUR,
    );
    p.rect_stroke(
        rect,
        CornerRadius::same(GLASS_RADIUS as u8),
        Stroke::new(GLOW_RING_W, colors::GLOW_RING.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
}

/// The sparks in flight; their centers are widget-local points from `rect.min`.
pub fn paint_sparks(p: &Painter, rect: Rect, dots: &[SparkDot]) {
    for &(at, scale, alpha, color) in dots {
        let c = rect.min + vec2(at.x, at.y);
        let r = SPARK_R * scale;
        let a = alpha * coverage(rect, c, r);
        if a > 0.0 && r > 0.0 {
            p.circle_filled(c, r, fade(color, a));
        }
    }
}

fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}
