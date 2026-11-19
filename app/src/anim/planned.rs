//! Planned-reset and reduced-motion timelines (port of Phase 1 (git bcaa1bf)
//! `ui/src/anim/planned.ts`) and the shared `RingFrame`.

use super::easing::{Ease, EASE_CSS, EASE_DEFAULT, EASE_IN_OUT, EASE_OUT};
use super::track::Track;
use crate::ui::colors::GREEN;
use egui::Color32;

/// What a ring looks like at one instant of a reset animation (overrides the live view).
#[derive(Clone, Debug, PartialEq)]
pub struct RingFrame {
    /// 0..1 drawn arc.
    pub arc_fraction: f32,
    pub arc_color: Color32,
    /// 0..1.
    pub arc_alpha: f32,
    /// 0..1 gold brightness + glow strength (surprise).
    pub arc_flash: f32,
    /// (position along the arc 0..1, alpha).
    pub glint: Option<(f32, f32)>,
    pub tick_alpha: f32,
    /// Whole ring.
    pub scale: f32,
    pub num: i64,
    pub num_color: Color32,
    pub num_alpha: f32,
    pub num_dy: f32,
    pub num_scale: f32,
    /// 0..6 px.
    pub num_blur: f32,
    /// The check mark after the name.
    pub ok_alpha: f32,
    pub ok_scale: f32,
}

impl RingFrame {
    /// The ring as the live view shows it (arc left empty for the caller to fill).
    pub(super) fn rest(live_num: i64, live_color: Color32) -> RingFrame {
        RingFrame {
            arc_fraction: 0.0,
            arc_color: live_color,
            arc_alpha: 1.0,
            arc_flash: 0.0,
            glint: None,
            tick_alpha: 1.0,
            scale: 1.0,
            num: live_num,
            num_color: live_color,
            num_alpha: 1.0,
            num_dy: 0.0,
            num_scale: 1.0,
            num_blur: 0.0,
            ok_alpha: 0.0,
            ok_scale: 0.3,
        }
    }
}

/// Planned: the pill appears when the glint and the number roll are done.
pub const PLANNED_PILL_AT: f64 = 1450.0;
/// Planned: the ring may show live values again.
pub const PLANNED_END: f64 = 2150.0;
pub const SURPRISE_PILL_AT: f64 = 640.0;
pub const SURPRISE_END: f64 = 3040.0;
pub const CROSSFADE_PILL_AT: f64 = 200.0;
pub const CROSSFADE_END: f64 = 500.0;
/// The check keeps its own 3200 ms timeline (from 950) after the ring unlocks.
pub const OK_END: f64 = 950.0 + 3200.0;

pub(super) fn delayed(mut t: Track, delay_ms: f64) -> Track {
    t.delay_ms = delay_ms;
    t
}

/// Planned reset ("P2" light sweep): glint along the arc, arc empties, ring squeezes, number
/// rolls to 0 in green, a check appears.
pub fn planned_frame(ms: f64, from_used: f64, live_num: i64, live_color: Color32) -> RingFrame {
    let from = (from_used / 100.0) as f32;
    let mut f = RingFrame::rest(live_num, live_color);
    f.arc_fraction = if ms < 750.0 {
        from
    } else {
        let ease = Ease::CubicBezier(0.6, 0.0, 0.4, 1.0);
        delayed(Track::new(450.0, &[from, 0.0], None, ease), 750.0).sample(ms)
    };
    if ms < 750.0 {
        let times = [0.0, 0.15, 0.85, 1.0];
        let alpha = Track::new(750.0, &[0.0, 1.0, 1.0, 0.0], Some(&times), EASE_IN_OUT).sample(ms);
        let pos = Track::new(750.0, &[0.0, from], Some(&[0.0, 1.0]), EASE_IN_OUT).sample(ms);
        f.glint = Some((pos, alpha));
    }
    f.tick_alpha = delayed(Track::new(300.0, &[1.0, 0.0], None, EASE_CSS), 750.0).sample(ms);
    let squeeze = Track::new(
        800.0,
        &[1.0, 0.9, 1.0],
        Some(&[0.0, 0.35, 1.0]),
        Ease::Spring,
    );
    f.scale = delayed(squeeze, 750.0).sample(ms);
    if ms < 950.0 {
        let dy = Track::new(200.0, &[0.0, -8.0], None, EASE_DEFAULT);
        let a = Track::new(200.0, &[1.0, 0.0], None, EASE_DEFAULT);
        f.num_dy = delayed(dy, 750.0).sample(ms);
        f.num_alpha = delayed(a, 750.0).sample(ms);
    } else {
        f.num = 0;
        f.num_color = GREEN;
        f.arc_color = GREEN;
        let dy = Track::new(500.0, &[8.0, 0.0], None, Ease::Spring);
        let a = Track::new(500.0, &[0.0, 1.0], None, Ease::Spring);
        f.num_dy = delayed(dy, 950.0).sample(ms);
        f.num_alpha = delayed(a, 950.0).sample(ms).clamp(0.0, 1.0);
        let times = [0.0, 0.45, 0.9, 1.0];
        let ok_a = Track::new(3200.0, &[0.0, 1.0, 1.0, 0.0], Some(&times), EASE_OUT);
        let ok_s = Track::new(3200.0, &[0.3, 1.0, 1.0, 1.0], Some(&times), EASE_OUT);
        f.ok_alpha = delayed(ok_a, 950.0).sample(ms);
        f.ok_scale = delayed(ok_s, 950.0).sample(ms);
    }
    f
}

/// Reduced motion: no sweep, no sparks; the number crossfades to 0.
pub fn crossfade_frame(ms: f64, live_num: i64, live_color: Color32) -> RingFrame {
    let mut f = RingFrame::rest(live_num, live_color);
    f.tick_alpha = 0.0;
    if ms < 200.0 {
        f.num_alpha = Track::new(200.0, &[1.0, 0.0], None, EASE_DEFAULT).sample(ms);
    } else {
        f.num = 0;
        let a = Track::new(300.0, &[0.0, 1.0], None, EASE_DEFAULT);
        f.num_alpha = delayed(a, 200.0).sample(ms);
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::colors::{BLUE, GREEN};

    #[test]
    fn planned_key_moments() {
        let f = planned_frame(0.0, 62.0, 40, BLUE);
        assert_eq!((f.num, f.arc_fraction), (40, 0.62));
        assert_eq!(f.glint.unwrap().1, 0.0);
        let f = planned_frame(1300.0, 62.0, 40, BLUE);
        assert_eq!((f.num, f.num_color, f.arc_fraction), (0, GREEN, 0.0));
        assert_eq!(f.tick_alpha, 0.0);
        assert!(planned_frame(2000.0, 62.0, 40, BLUE).ok_alpha > 0.8);
    }

    #[test]
    fn planned_glint_runs_to_the_arc_end_then_goes() {
        let g = planned_frame(375.0, 62.0, 40, BLUE).glint.unwrap();
        assert!(g.0 > 0.0 && g.0 < 0.62 && g.1 > 0.9);
        assert!(planned_frame(750.0, 62.0, 40, BLUE).glint.is_none());
    }

    #[test]
    fn planned_number_swaps_at_950_and_ok_outlives_the_lock() {
        assert_eq!(planned_frame(949.0, 62.0, 40, BLUE).num, 40);
        let f = planned_frame(950.0, 62.0, 40, BLUE);
        assert_eq!((f.num, f.arc_color, f.num_color), (0, GREEN, GREEN));
        assert!(planned_frame(PLANNED_END + 500.0, 62.0, 40, BLUE).ok_alpha > 0.0);
        assert_eq!(planned_frame(OK_END, 62.0, 40, BLUE).ok_alpha, 0.0);
    }

    #[test]
    fn crossfade_swaps_the_number_at_200() {
        let f = crossfade_frame(0.0, 40, BLUE);
        assert_eq!(
            (f.num, f.arc_fraction, f.tick_alpha, f.num_alpha),
            (40, 0.0, 0.0, 1.0)
        );
        let f = crossfade_frame(200.0, 40, BLUE);
        assert_eq!((f.num, f.num_alpha), (0, 0.0));
        assert_eq!(crossfade_frame(CROSSFADE_END, 40, BLUE).num_alpha, 1.0);
    }
}
