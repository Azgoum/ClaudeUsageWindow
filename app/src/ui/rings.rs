//! One ring column of the rings view (port of `Ring.tsx` and `.wd .rg`): the 64×64 ring,
//! its name and its reset line. Live values ease like Phase 1's CSS transitions; a
//! `RingFrame` (reset animations, Task 13) overrides them while it plays.

use std::f32::consts::TAU;
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Utc};
use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Color32, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke};
use limits_core::{LimitKey, RingView};

use super::format::reset_line;
use super::i18n::t;
use super::ring::{arc_fraction, arc_stroke, num_fill, ring_number, tick_angle_deg, tick_opacity};
use super::{colors, draw, fonts, Lang};
use crate::anim::easing::{EASE_CSS, EASE_RING};
use crate::anim::tween::Tween;
use crate::anim::RingFrame;

/// `.wd .rg { width: 104px }`.
pub const COLUMN_W: f32 = 104.0;
/// The ring's SVG box.
const SVG: f32 = 64.0;
/// `.rg svg { margin-bottom: 6px }`, `.rg .when { margin-top: 2px }`.
const NAME_GAP: f32 = 6.0;
const WHEN_GAP: f32 = 2.0;
const NAME_SIZE: f32 = 11.0;
const WHEN_SIZE: f32 = 10.5;
/// `.wd { font: 12px/1.2 }`: every text line box is 1.2 × its font size.
pub const LINE_HEIGHT: f32 = 1.2;
/// Column height: ring, name line, reset line.
pub const COLUMN_H: f32 =
    SVG + NAME_GAP + NAME_SIZE * LINE_HEIGHT + WHEN_GAP + WHEN_SIZE * LINE_HEIGHT;

/// Ring geometry inside the SVG box: center (32, 32), r 23, stroke 6; tick from y 3 to y 12.
const C: f32 = 32.0;
const R: f32 = 23.0;
const STROKE: f32 = 6.0;
const TICK_FROM: f32 = 3.0;
const TICK_TO: f32 = 12.0;
const TICK_WIDTH: f32 = 2.0;
/// Number: bold 13.5 with a bold 9 "%" 1 after it, baseline 36.5; no limit: "∞" bold 24 at 39.
const NUM_SIZE: f32 = 13.5;
const PCT_SIZE: f32 = 9.0;
const PCT_DX: f32 = 1.0;
const NUM_BASELINE: f32 = 36.5;
const INF_SIZE: f32 = 24.0;
const INF_BASELINE: f32 = 39.0;

/// CSS transitions: arc and tick 0.8 s ring ease, colors 0.4 s, tick opacity 0.3 s.
const ARC_DUR: f64 = 0.8;
const COLOR_DUR: f64 = 0.4;
const TICK_ALPHA_DUR: f64 = 0.3;

/// What the ring shows from the live view (after the transitions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Live {
    pub arc_fraction: f32,
    pub tick_deg: f32,
    pub tick_alpha: f32,
    pub arc_color: Color32,
    pub num_color: Color32,
}

impl Live {
    pub fn of(view: &RingView) -> Live {
        Live {
            arc_fraction: arc_fraction(view.used),
            tick_deg: tick_angle_deg(view.expected),
            tick_alpha: tick_opacity(view.expected),
            arc_color: arc_stroke(view),
            num_color: num_fill(view),
        }
    }
}

/// One ring's transitions. The first view after `new` / `reset` shows at once.
#[derive(Clone, Debug)]
pub struct RingTweens {
    arc: Tween<f32>,
    tick_deg: Tween<f32>,
    tick_alpha: Tween<f32>,
    arc_color: Tween<Color32>,
    num_color: Tween<Color32>,
    target: Option<Live>,
}

impl Default for RingTweens {
    fn default() -> Self {
        Self::new()
    }
}

impl RingTweens {
    pub fn new() -> RingTweens {
        RingTweens {
            arc: Tween::new(0.0),
            tick_deg: Tween::new(0.0),
            tick_alpha: Tween::new(0.0),
            arc_color: Tween::new(Color32::TRANSPARENT),
            num_color: Tween::new(Color32::TRANSPARENT),
            target: None,
        }
    }

    /// Heads to `view`'s values at `t`; each property restarts only when its own target changes.
    pub fn update(&mut self, view: &RingView, t: f64) {
        let to = Live::of(view);
        match self.target {
            None => {
                self.arc.snap(to.arc_fraction);
                self.tick_deg.snap(to.tick_deg);
                self.tick_alpha.snap(to.tick_alpha);
                self.arc_color.snap(to.arc_color);
                self.num_color.snap(to.num_color);
            }
            Some(from) => {
                if from.arc_fraction != to.arc_fraction {
                    self.arc.retarget(t, to.arc_fraction, ARC_DUR, EASE_RING);
                }
                if from.tick_deg != to.tick_deg {
                    self.tick_deg.retarget(t, to.tick_deg, ARC_DUR, EASE_RING);
                }
                if from.tick_alpha != to.tick_alpha {
                    self.tick_alpha
                        .retarget(t, to.tick_alpha, TICK_ALPHA_DUR, EASE_CSS);
                }
                if from.arc_color != to.arc_color {
                    self.arc_color
                        .retarget(t, to.arc_color, COLOR_DUR, EASE_CSS);
                }
                if from.num_color != to.num_color {
                    self.num_color
                        .retarget(t, to.num_color, COLOR_DUR, EASE_CSS);
                }
            }
        }
        self.target = Some(to);
    }

    /// The rings went away: the next view shows at once.
    pub fn reset(&mut self) {
        self.target = None;
    }

    pub fn value(&self, t: f64) -> Live {
        Live {
            arc_fraction: self.arc.value(t),
            tick_deg: self.tick_deg.value(t),
            tick_alpha: self.tick_alpha.value(t),
            arc_color: self.arc_color.value(t),
            num_color: self.num_color.value(t),
        }
    }

    pub fn animating(&self, t: f64) -> bool {
        !(self.arc.done(t)
            && self.tick_deg.done(t)
            && self.tick_alpha.done(t)
            && self.arc_color.done(t)
            && self.num_color.done(t))
    }
}

/// Everything one column needs to paint.
pub struct Column<'a> {
    pub view: &'a RingView,
    pub live: Live,
    /// Reset animation frame overriding the live values (Task 13).
    pub frame: Option<&'a RingFrame>,
    pub lang: Lang,
    pub now: DateTime<Utc>,
    pub tz: FixedOffset,
}

/// The column's box when its top-left corner is `min`.
pub fn column_rect(min: Pos2) -> Rect {
    Rect::from_min_size(min, vec2(COLUMN_W, COLUMN_H))
}

/// Premultiplied color `f` times as opaque.
fn fade(c: Color32, f: f32) -> Color32 {
    c.gamma_multiply(f.clamp(0.0, 1.0))
}

/// One line of text in a CSS line box `font.size × 1.2` tall, glyphs centered in it.
fn line_galley(p: &Painter, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    let line_height = font.size * LINE_HEIGHT;
    line_galley_in(p, text, font, color, line_height)
}

/// One line of text centered in a line box `line_height` tall.
pub(super) fn line_galley_in(
    p: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
) -> Arc<Galley> {
    let line_height = Some(line_height);
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color,
            line_height,
            ..Default::default()
        },
    );
    job.wrap.max_width = f32::INFINITY;
    p.layout_job(job)
}

/// Paints a text line centered on `center_x` with its line box at `top`.
pub fn centered_line(
    p: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    center_x: f32,
    top: f32,
) {
    let g = line_galley(p, text, font, color);
    p.galley(pos2(center_x - g.size().x / 2.0, top), g, color);
}

/// Like `centered_line`, but a line wider than `max_width` is drawn in a smaller font
/// (same line box) so it fits. Returns the painted text's rect.
pub fn centered_line_fit(
    p: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    center_x: f32,
    top: f32,
    max_width: f32,
) -> Rect {
    let line_height = font.size * LINE_HEIGHT;
    let mut g = line_galley_in(p, text, font.clone(), color, line_height);
    let mut size = font.size;
    // Glyph positions snap to pixels, so the width is not exactly linear in the size: retry.
    while g.size().x > max_width && size > 1.0 {
        size = (size * max_width / g.size().x - 0.05).max(1.0);
        let smaller = FontId::new(size, font.family.clone());
        g = line_galley_in(p, text, smaller, color, line_height);
    }
    let rect = Rect::from_min_size(pos2(center_x - g.size().x / 2.0, top), g.size());
    p.galley(rect.min, g, color);
    rect
}

/// Baseline of the galley's first row, from the galley top.
pub(super) fn baseline(g: &Galley) -> f32 {
    g.rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|gl| r.pos.y + gl.pos.y))
        .unwrap_or(0.0)
}

/// A line with round caps as one convex shape (no double blend where caps meet the line).
fn capsule(a: Pos2, b: Pos2, width: f32, color: Color32) -> Shape {
    let d = b - a;
    let len = d.length();
    let dir = if len > 0.0 { d / len } else { vec2(0.0, -1.0) };
    let n = vec2(-dir.y, dir.x);
    let hw = width / 2.0;
    const HALF: usize = 8;
    let mut pts = Vec::with_capacity(2 * (HALF + 1));
    for (end, sign) in [(b, 1.0), (a, -1.0)] {
        for k in 0..=HALF {
            let phi = std::f32::consts::PI * k as f32 / HALF as f32;
            // From +n through ±dir to -n (sweeping around the end).
            let u = n * phi.cos() * sign + dir * phi.sin() * sign;
            pts.push(end + u * hw);
        }
    }
    Shape::convex_polygon(pts, color, Stroke::NONE)
}

/// Paints the column whose box is `rect`; every color goes through `fx` (error grayscale).
pub fn paint_column(p: &Painter, rect: Rect, col: &Column, fx: &dyn Fn(Color32) -> Color32) {
    let px = 1.0 / p.pixels_per_point();
    let view = col.view;
    let svg = pos2(rect.center().x - SVG / 2.0, rect.top());
    let scale = col.frame.map_or(1.0, |f| f.scale);
    let center = svg + vec2(C, C);
    // The whole ring scales about its center (Task 13 squeeze).
    let at = |x: f32, y: f32| center + (vec2(x, y) - vec2(C, C)) * scale;

    p.add(draw::arc_mesh(
        center,
        R * scale,
        STROKE * scale,
        0.0,
        1.0,
        fx(colors::TRACK),
        px,
    ));
    let (fraction, arc_color) = match col.frame {
        Some(f) => (f.arc_fraction, fade(f.arc_color, f.arc_alpha)),
        None => (col.live.arc_fraction, col.live.arc_color),
    };
    p.add(draw::arc_mesh(
        center,
        R * scale,
        STROKE * scale,
        0.0,
        fraction,
        fx(arc_color),
        px,
    ));

    let tick_alpha = col.live.tick_alpha * col.frame.map_or(1.0, |f| f.tick_alpha);
    if tick_alpha > 0.0 {
        let rot = egui::emath::Rot2::from_angle(col.live.tick_deg / 360.0 * TAU);
        let end = |y: f32| center + rot * (at(C, y) - center);
        let color = fx(fade(Color32::WHITE, tick_alpha));
        p.add(capsule(
            end(TICK_FROM),
            end(TICK_TO),
            TICK_WIDTH * scale,
            color,
        ));
    }

    paint_number(p, svg, center, scale, col, fx);

    let l = t(col.lang);
    let name = if view.no_limit {
        l.no_limit
    } else if view.key == LimitKey::Session {
        l.s
    } else {
        l.w
    };
    let name_top = rect.top() + SVG + NAME_GAP;
    let cx = rect.center().x;
    centered_line(
        p,
        name,
        fonts::semibold(NAME_SIZE),
        fx(colors::NAME),
        cx,
        name_top,
    );
    if !view.no_limit {
        let when = reset_line(view.key, view.resets_at, col.now, col.tz, col.lang);
        let when_top = name_top + NAME_SIZE * LINE_HEIGHT + WHEN_GAP;
        centered_line(
            p,
            &when,
            fonts::regular(WHEN_SIZE),
            fx(colors::INK),
            cx,
            when_top,
        );
    }
}

/// The number (or "∞") centered as a whole on the ring, on its SVG baseline.
fn paint_number(
    p: &Painter,
    svg: Pos2,
    center: Pos2,
    scale: f32,
    col: &Column,
    fx: &dyn Fn(Color32) -> Color32,
) {
    let (num, color, dy, num_scale) = match col.frame {
        Some(f) => (f.num, fade(f.num_color, f.num_alpha), f.num_dy, f.num_scale),
        None => (ring_number(col.view.used), col.live.num_color, 0.0, 1.0),
    };
    let color = fx(color);
    if color.a() == 0 {
        return;
    }
    let s = scale * num_scale;
    let (parts, base_y, size): (Vec<(String, f32)>, f32, f32) = if col.view.no_limit {
        (vec![("∞".into(), INF_SIZE)], INF_BASELINE, INF_SIZE)
    } else {
        (
            vec![(num.to_string(), NUM_SIZE), ("%".into(), PCT_SIZE)],
            NUM_BASELINE,
            NUM_SIZE,
        )
    };
    // Baseline in the ring box (whole-ring scale), then the number's own scale about its
    // middle (CSS `transform-origin: center` of the text box: about 0.36 em above the baseline).
    let ring_base = center.y + (svg.y + base_y + dy - center.y) * scale;
    let mid = ring_base - 0.36 * size * scale;
    let base = mid + (ring_base - mid) * num_scale;
    let galleys: Vec<Arc<Galley>> = parts
        .iter()
        .map(|(text, sz)| p.layout_no_wrap(text.clone(), fonts::bold(sz * s), color))
        .collect();
    let gap = if galleys.len() > 1 { PCT_DX * s } else { 0.0 };
    let total: f32 = galleys.iter().map(|g| g.size().x).sum::<f32>() + gap;
    let mut x = center.x - total / 2.0;
    for g in galleys {
        let w = g.size().x;
        let top = base - baseline(&g);
        p.galley(pos2(x, top), g, color);
        x += w + gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limits_core::Pace;

    fn view(used: f64, pace: Pace) -> RingView {
        RingView {
            key: LimitKey::Session,
            used,
            resets_at: None,
            expected: Some(48.0),
            pace,
            no_limit: false,
        }
    }

    #[test]
    fn first_view_shows_at_once_then_values_ease() {
        let mut tw = RingTweens::new();
        tw.update(&view(62.0, Pace::Ahead), 5.0);
        assert!(!tw.animating(5.0));
        assert_eq!(tw.value(5.0), Live::of(&view(62.0, Pace::Ahead)));

        tw.update(&view(40.0, Pace::Room), 10.0);
        let mid = tw.value(10.2);
        assert!(mid.arc_fraction < 0.62 && mid.arc_fraction > 0.4, "{mid:?}");
        assert_ne!(mid.arc_color, colors::BLUE);
        // Colors settle after 0.4 s, the arc after 0.8 s.
        assert_eq!(tw.value(10.4).arc_color, colors::BLUE);
        assert!(tw.animating(10.79));
        assert!(!tw.animating(10.8));
        assert_eq!(tw.value(10.8).arc_fraction, 0.4);
    }

    #[test]
    fn unchanged_properties_do_not_restart() {
        let mut tw = RingTweens::new();
        tw.update(&view(62.0, Pace::Ahead), 0.0);
        tw.update(&view(62.0, Pace::Ahead), 1.0);
        assert!(!tw.animating(1.0));
        // Only the tick changes (expected gone): it fades in 0.3 s and turns to 0° in 0.8 s.
        let mut v = view(62.0, Pace::Ahead);
        v.expected = None;
        tw.update(&v, 2.0);
        let mid = tw.value(2.15);
        assert_eq!(
            (mid.arc_fraction, mid.arc_color, mid.num_color),
            (0.62, colors::RED, colors::RED)
        );
        assert!(mid.tick_alpha > 0.0 && mid.tick_alpha < 0.95);
        assert_eq!(tw.value(2.3).tick_alpha, 0.0);
        assert!(tw.animating(2.79) && !tw.animating(2.8));
    }

    #[test]
    fn reset_snaps_the_next_view() {
        let mut tw = RingTweens::new();
        tw.update(&view(62.0, Pace::Ahead), 0.0);
        tw.reset();
        tw.update(&view(10.0, Pace::Room), 1.0);
        assert!(!tw.animating(1.0));
        assert_eq!(tw.value(1.0).arc_fraction, 0.1);
    }

    #[test]
    fn column_is_ring_name_and_reset_line() {
        assert!((COLUMN_H - (64.0 + 6.0 + 13.2 + 2.0 + 12.6)).abs() < 1e-4);
    }
}
