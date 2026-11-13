//! Ring tooltip (port of `RingTooltip.tsx` and `index.css` `.tip`): a dark card 16 under the
//! hovered ring column with the plan, the "updated" age and the ring's usage, pace and reset.
//!
//! `.tip { font: 11px/1.5; padding: 7px 10px; border: 1px; border-radius: 9px;
//! white-space: nowrap; max-width: calc(100vw - 12px) }`, title `.tt` 650 weight, 5 px under
//! it a 1 px rule, 5 px more.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{pos2, vec2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Stroke, Vec2};
use limits_core::RingView;

use super::ring::{ago_text, tip_model, tip_title, TipModel};
use super::{colors, fonts, FrameInput};

/// Fade in and out, seconds.
pub const FADE: f32 = 0.15;
/// Gap between the hit box bottom and the tooltip.
pub const GAP: f32 = 16.0;
const SIZE: f32 = 11.0;
const LINE: f32 = SIZE * 1.5;
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 7.0;
const BORDER: f32 = 1.0;
const RADIUS: u8 = 9;
/// Title padding-bottom, rule, margin-bottom.
const TT_PAD: f32 = 5.0;
const RULE: f32 = 1.0;
const TT_MARGIN: f32 = 5.0;

/// A laid-out tooltip.
pub struct Tip {
    title: Arc<Galley>,
    lines: Vec<Arc<Galley>>,
    /// Whole box incl. border.
    pub size: Vec2,
}

fn format(font: FontId, color: Color32) -> TextFormat {
    TextFormat {
        font_id: font,
        color,
        line_height: Some(LINE),
        ..Default::default()
    }
}

/// `(text, bold color)` runs of one body line: `Some(color)` is bold in that color, `None`
/// regular `#ddd`.
type Runs = Vec<(String, Option<Color32>)>;

fn line_at(p: &Painter, runs: &Runs, size: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    for (text, bold) in runs {
        let f = match bold {
            Some(c) => format(fonts::bold(size), *c),
            None => format(fonts::regular(size), colors::TIP_TEXT),
        };
        job.append(text, 0.0, f);
    }
    job.wrap.max_width = f32::INFINITY;
    p.layout_job(job)
}

/// The line at 11 px, or, when wider than `max_width` (the window is only 244 wide), in a
/// smaller size that fits: CSS would let it run past the box and the window would cut it.
fn line(p: &Painter, runs: &Runs, max_width: f32) -> Arc<Galley> {
    let mut g = line_at(p, runs, SIZE);
    let mut size = SIZE;
    // Glyph positions snap to pixels, so the width is not exactly linear in the size: retry.
    while g.size().x > max_width && size > 1.0 {
        size = (size * max_width / g.size().x - 0.05).max(1.0);
        g = line_at(p, runs, size);
    }
    g
}

/// The title row (`{title} <span>· {ago}</span>`), cut with "…" past `max_width`.
fn title_row(p: &Painter, title: &str, ago: Option<&str>, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(title, 0.0, format(fonts::semibold(SIZE), Color32::WHITE));
    if let Some(ago) = ago {
        job.append(" ", 0.0, format(fonts::semibold(SIZE), Color32::WHITE));
        job.append(
            &format!("· {ago}"),
            0.0,
            format(fonts::regular(SIZE), colors::MUTE),
        );
    }
    job.wrap = TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    p.layout_job(job)
}

impl Tip {
    /// Lays out the tooltip of `view`; the box is at most `max_width` wide (window − 12).
    pub fn layout(p: &Painter, view: &RingView, input: &FrameInput, max_width: f32) -> Tip {
        let state = input.state;
        let lang = input.prefs.lang;
        let title = tip_title(state.org.as_ref(), &state.orgs, lang);
        let ago = ago_text(state.last_ok, input.now, lang);
        let white = Some(Color32::WHITE);
        let headed =
            |title: String, rest: String| vec![(title, white), (format!(" · {rest}"), None)];
        let runs: Vec<Runs> = match tip_model(view, input.now, input.tz, lang) {
            TipModel::NoLimit { title, line: l } => vec![headed(title, l)],
            TipModel::Idle {
                title,
                line: l,
                sub,
            } => vec![headed(title, l), vec![(sub, None)]],
            TipModel::Active {
                title,
                usage,
                pace,
                resets,
            } => {
                let mut v = vec![headed(title, usage)];
                if let Some((text, color)) = pace {
                    v.push(vec![(text, Some(color))]);
                }
                if let Some(r) = resets {
                    v.push(vec![(r, None)]);
                }
                v
            }
        };
        let inner_max = max_width - 2.0 * (PAD_X + BORDER);
        let lines: Vec<_> = runs.iter().map(|r| line(p, r, inner_max)).collect();
        let natural = title_row(p, &title, ago.as_deref(), f32::INFINITY);
        let widest = lines
            .iter()
            .map(|g| g.size().x)
            .fold(natural.size().x, f32::max);
        let inner = widest.min(inner_max);
        let title = if natural.size().x > inner {
            title_row(p, &title, ago.as_deref(), inner)
        } else {
            natural
        };
        let h = 2.0 * (BORDER + PAD_Y) + LINE + TT_PAD + RULE + TT_MARGIN;
        let size = vec2(
            inner + 2.0 * (PAD_X + BORDER),
            h + LINE * lines.len() as f32,
        );
        Tip { title, lines, size }
    }

    /// Paints the box with its top-left corner at `min`.
    pub fn paint(&self, p: &Painter, min: Pos2) {
        let rect = Rect::from_min_size(min, self.size);
        p.rect(
            rect,
            CornerRadius::same(RADIUS),
            colors::TIP_BG,
            Stroke::new(BORDER, colors::TIP_BORDER),
            egui::StrokeKind::Inside,
        );
        let left = rect.left() + BORDER + PAD_X;
        let right = rect.right() - BORDER - PAD_X;
        let mut y = rect.top() + BORDER + PAD_Y;
        p.galley(pos2(left, y), self.title.clone(), Color32::WHITE);
        y += LINE + TT_PAD;
        p.rect_filled(
            Rect::from_min_max(pos2(left, y), pos2(right, y + RULE)),
            0,
            colors::TIP_BORDER,
        );
        y += RULE + TT_MARGIN;
        for g in &self.lines {
            p.galley(pos2(left, y), g.clone(), colors::TIP_TEXT);
            y += LINE;
        }
    }
}

/// Where the tooltip of the hit box `hit` goes in a window `window_w` wide: 16 under it,
/// centered on it, shifted to stay 6 inside the window.
pub fn place(hit: Rect, size: Vec2, window_w: f32) -> Pos2 {
    let pad = super::room::PAD;
    let x = (hit.center().x - size.x / 2.0)
        .min(window_w - pad - size.x)
        .max(pad);
    pos2(x, hit.bottom() + GAP)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui::{pos2, Pos2};
    use limits_core::LimitKey;

    use crate::ui::widget::snapshot_tests::{click_at, hover, render, H};
    use crate::ui::{Action, FrameOutput, Lang, Repaint};

    /// Between the two ring columns: plain glass, no tooltip.
    const GLASS: Pos2 = pos2(122.0, 60.0);

    fn ring_center(h: &H, key: LimitKey) -> Pos2 {
        let name = match key {
            LimitKey::Session => "ring_session",
            LimitKey::Weekly => "ring_weekly",
        };
        h.state().widget.debug_rects()[name].center()
    }

    fn hovered_ring(kind: &str, key: LimitKey, lang: Lang) -> H {
        let mut h = render(kind, lang, 1.0);
        let at = ring_center(&h, key);
        hover(&mut h, at);
        h
    }

    fn out(mut h: H) -> FrameOutput {
        h.state_mut().out.take().expect("shown")
    }

    fn hover_ring(kind: &str, key: LimitKey) -> FrameOutput {
        out(hovered_ring(kind, key, Lang::En))
    }

    fn hover_then_leave(kind: &str) -> FrameOutput {
        let mut h = hovered_ring(kind, LimitKey::Session, Lang::En);
        h.input_mut().events.push(egui::Event::PointerMoved(GLASS));
        h.run();
        out(h)
    }

    #[test]
    fn tooltip_room_grows_window() {
        let o = hover_ring("connected", LimitKey::Session);
        assert!(o.extra > 60.0);
    }

    #[test]
    fn tooltip_room_matches_phase_1() {
        // Trigger bottom 122.8, 16 gap, 93 tall (2 + 14 + 16.5 + 11 + 3 × 16.5), 6 padding.
        assert_eq!(hover_ring("connected", LimitKey::Session).extra, 92.0);
    }

    #[test]
    fn closing_tooltip_returns_to_rest() {
        assert_eq!(hover_then_leave("connected").extra, 0.0);
    }

    #[test]
    fn tooltip_ticks_every_second() {
        assert_eq!(
            hover_ring("connected", LimitKey::Weekly).repaint,
            Repaint::After(Duration::from_secs(1))
        );
    }

    #[test]
    fn tooltip_stays_inside_the_window() {
        for key in [LimitKey::Session, LimitKey::Weekly] {
            for lang in [Lang::En, Lang::Fr] {
                let h = hovered_ring("connected", key, lang);
                let rects = h.state().widget.debug_rects();
                let tip = rects
                    .get("tip_session")
                    .or(rects.get("tip_weekly"))
                    .expect("open");
                assert!(tip.left() >= 6.0 && tip.right() <= 238.0, "{key:?} {tip:?}");
            }
        }
    }

    #[test]
    fn first_frame_before_the_window_grew_is_invisible_and_repaints() {
        let mut h = render("connected", Lang::En, 1.0);
        let at = ring_center(&h, LimitKey::Session);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.step();
        let o = h.state().out.as_ref().unwrap();
        assert!(o.extra > 0.0);
        assert_eq!(o.repaint, Repaint::Now);
    }

    #[test]
    fn press_closes_the_tooltip_at_once() {
        // A press on the ring starts a drag: the room goes back to 0 in that very frame.
        let mut h = hovered_ring("connected", LimitKey::Session, Lang::En);
        let at = ring_center(&h, LimitKey::Session);
        click_at(&mut h, at, egui::PointerButton::Primary);
        let o = h.state().out.as_ref().unwrap();
        assert_eq!(o.actions, vec![Action::StartDrag]);
        assert_eq!(o.extra, 0.0);
    }

    fn snap(kind: &str, key: LimitKey, lang: Lang, name: &str) {
        hovered_ring(kind, key, lang).snapshot(name);
    }

    #[test]
    fn tooltip_session_en() {
        snap(
            "connected",
            LimitKey::Session,
            Lang::En,
            "tooltip_session_en",
        );
    }
    #[test]
    fn tooltip_session_fr() {
        snap(
            "connected",
            LimitKey::Session,
            Lang::Fr,
            "tooltip_session_fr",
        );
    }
    #[test]
    fn tooltip_weekly_en() {
        snap("connected", LimitKey::Weekly, Lang::En, "tooltip_weekly_en");
    }
    #[test]
    fn tooltip_idle_en() {
        snap("idle", LimitKey::Session, Lang::En, "tooltip_idle_en");
    }
    #[test]
    fn tooltip_ultra_en() {
        snap("ultra", LimitKey::Session, Lang::En, "tooltip_ultra_en");
    }
    #[test]
    fn tooltip_ultra_fr() {
        snap("ultra", LimitKey::Session, Lang::Fr, "tooltip_ultra_fr");
    }
}
