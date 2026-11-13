//! The reset pill (port of `Pill.tsx` and `.wd .chip`): one line, centered 2 px below the
//! widget's padding-box top, fully rounded; text too wide for the widget shrinks instead of
//! wrapping (`pill_font_size`).

use egui::{pos2, vec2, Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind};

use super::rings::{line_galley_in, LINE_HEIGHT};
use super::{colors, draw, fonts};
use crate::anim::pill::{pill_font_size, PillKind, PillSpec, PILL_FONT};
use crate::anim::PillFrame;

const BORDER: f32 = 1.0;
/// `.chip { top: 2px; padding: 3px 8px }`, inside the widget's 1 px border.
const TOP: f32 = 2.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 3.0;
/// Padding + border, which do not shrink with the font.
const CHROME: f32 = 2.0 * (PAD_X + BORDER);
/// `Pill.tsx` `SIDE_MARGIN`: room left on each side inside the widget.
const SIDE_MARGIN: f32 = 4.0;
/// `.tauri .wd .chip.gold { box-shadow: 0 0 12px rgba(250,204,21,.3) }`.
const GOLD_GLOW_BLUR: f32 = 12.0;

struct Look {
    bg: Color32,
    text: Color32,
    border: Color32,
    glow: Option<Color32>,
}

fn look(kind: PillKind) -> Look {
    match kind {
        PillKind::Calm => Look {
            bg: colors::PILL_CALM_BG,
            text: colors::PILL_CALM_TEXT,
            border: colors::PILL_CALM_BORDER,
            glow: None,
        },
        PillKind::Gold => Look {
            bg: colors::PILL_GOLD_BG,
            text: colors::PILL_GOLD_TEXT,
            border: colors::PILL_GOLD_BORDER,
            glow: Some(colors::PILL_GOLD_GLOW),
        },
    }
}

/// Paints the pill `spec` at `frame` over the widget box `widget`; returns its rect (unscaled).
pub fn paint(p: &Painter, widget: Rect, spec: &PillSpec, frame: &PillFrame) -> Rect {
    let look = look(spec.kind);
    let avail = widget.width() - 2.0 * BORDER - 2.0 * SIDE_MARGIN;
    let font_at = |size: f32| {
        line_galley_in(
            p,
            &spec.text,
            fonts::semibold(size),
            look.text,
            size * LINE_HEIGHT,
        )
    };
    let base = font_at(PILL_FONT);
    let size = pill_font_size(base.size().x + CHROME, avail, CHROME).unwrap_or(PILL_FONT);
    let text_w = if size == PILL_FONT {
        base.size().x
    } else {
        font_at(size).size().x
    };
    let pill = vec2(text_w + CHROME, size * LINE_HEIGHT + 2.0 * (PAD_Y + BORDER));
    let rest = Rect::from_min_size(
        pos2(
            widget.center().x - pill.x / 2.0,
            widget.top() + BORDER + TOP,
        ),
        pill,
    );
    // `translate(-50%, dy) scale(s)` about the pill's center, and the whole pill's opacity.
    let s = frame.scale;
    let r = Rect::from_center_size(rest.center() + vec2(0.0, frame.dy), pill * s);
    let mut p = p.clone();
    p.multiply_opacity(frame.alpha.clamp(0.0, 1.0));
    let radius = r.height() / 2.0;
    if let Some(glow) = look.glow {
        let px = 1.0 / p.pixels_per_point();
        let mut mesh = draw::soft_shadow_mesh(r, radius, 0.0, GOLD_GLOW_BLUR, glow);
        draw::fade_outside(&mut mesh, widget, draw::GLASS_RADIUS, px);
        p.add(mesh);
    }
    let corners = CornerRadius::same(radius.round().min(255.0) as u8);
    p.rect_filled(r, corners, look.bg);
    p.rect_stroke(
        r,
        corners,
        Stroke::new(BORDER * s, look.border),
        StrokeKind::Inside,
    );
    let g = font_at(size * s);
    p.galley(r.center() - g.size() / 2.0, g, look.text);
    rest
}
