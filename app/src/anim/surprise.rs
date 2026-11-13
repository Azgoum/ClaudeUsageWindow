//! Surprise-reset timeline, gold glow and sparks (port of `ui/src/anim/surprise.ts`,
//! `components/Sparks.tsx`).

use super::easing::{Ease, EASE_DEFAULT};
use super::planned::{delayed, RingFrame};
use super::track::Track;
use crate::ui::colors::GOLD;
use egui::{pos2, Color32, Pos2};

/// Surprise reset: the arc flashes gold and empties, the number blurs out and back in at 0.
pub fn surprise_frame(ms: f64, from_used: f64, live_num: i64) -> RingFrame {
    let from = (from_used / 100.0) as f32;
    let mut f = RingFrame::rest(live_num, GOLD);
    f.arc_flash = Track::new(420.0, &[0.0, 1.0, 0.25], None, EASE_DEFAULT).sample(ms);
    f.tick_alpha = if ms < 380.0 { 1.0 } else { 0.0 };
    let sweep = Ease::CubicBezier(0.4, 0.0, 0.2, 1.0);
    f.arc_fraction = delayed(Track::new(380.0, &[from, 0.0], None, sweep), 380.0).sample(ms);
    f.arc_alpha = delayed(Track::new(380.0, &[1.0, 0.2], None, sweep), 380.0).sample(ms);
    if ms < 640.0 {
        let blur = Track::new(260.0, &[0.0, 6.0], None, EASE_DEFAULT);
        let a = Track::new(260.0, &[1.0, 0.0], None, EASE_DEFAULT);
        f.num_blur = delayed(blur, 380.0).sample(ms);
        f.num_alpha = delayed(a, 380.0).sample(ms);
    } else {
        f.num = 0;
        let blur = Track::new(700.0, &[6.0, 0.0], None, Ease::Spring);
        let a = Track::new(700.0, &[0.0, 1.0], None, Ease::Spring);
        let sc = Track::new(700.0, &[1.4, 1.0], None, Ease::Spring);
        f.num_blur = delayed(blur, 640.0).sample(ms).max(0.0);
        f.num_alpha = delayed(a, 640.0).sample(ms).clamp(0.0, 1.0);
        f.num_scale = delayed(sc, 640.0).sample(ms);
    }
    f
}

/// The widget's gold glow overlay, 4.2 s.
pub fn glow_alpha(ms: f64) -> f32 {
    let times = [0.0, 0.12, 0.8, 1.0];
    Track::new(4200.0, &[0.0, 1.0, 1.0, 0.0], Some(&times), EASE_DEFAULT).sample(ms)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spark {
    /// Start, on the ring's circle.
    pub x: f32,
    pub y: f32,
    /// Travel over the whole flight.
    pub dx: f32,
    pub dy: f32,
    pub dur_ms: f32,
    pub color: Color32,
}

const SPARK_COLORS: [Color32; 4] = [
    Color32::from_rgb(0xfa, 0xcc, 0x15),
    Color32::from_rgb(0xfd, 0xe6, 0x8a),
    Color32::from_rgb(0xfb, 0x92, 0x3c),
    Color32::WHITE,
];

/// xorshift64, so a burst is reproducible without a `rand` dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        let s = seed ^ 0x9E37_79B9_7F4A_7C15;
        Rng(if s == 0 { 0x9E37_79B9_7F4A_7C15 } else { s })
    }

    /// Uniform in [0, 1).
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// `n` sparks thrown off a ring: they start on the circle within the first `arc_fraction` of a
/// turn from 12 o'clock.
pub fn sparks(seed: u64, center: Pos2, radius: f32, arc_fraction: f32, n: usize) -> Vec<Spark> {
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|i| {
            let a0 = -std::f32::consts::FRAC_PI_2
                + rng.next_f32() * std::f32::consts::TAU * arc_fraction;
            let d = 24.0 + rng.next_f32() * 50.0;
            let dur_ms = 700.0 + rng.next_f32() * 600.0;
            Spark {
                x: center.x + a0.cos() * radius,
                y: center.y + a0.sin() * radius,
                dx: a0.cos() * d,
                dy: a0.sin() * d,
                dur_ms,
                color: SPARK_COLORS[i % 4],
            }
        })
        .collect()
}

/// Position, scale and alpha of a spark `ms` after the burst; `None` once it is gone.
pub fn spark_at(s: &Spark, ms: f64) -> Option<(Pos2, f32, f32)> {
    if ms >= s.dur_ms as f64 {
        return None;
    }
    let p = (ms.max(0.0) / s.dur_ms as f64) as f32;
    let e = Ease::CubicBezier(0.1, 0.7, 0.3, 1.0).apply(p);
    Some((pos2(s.x + s.dx * e, s.y + s.dy * e), 1.0 - 0.8 * e, 1.0 - e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::colors::GOLD;
    use egui::pos2;

    #[test]
    fn surprise_number_is_zero_after_640() {
        assert_eq!(surprise_frame(639.0, 62.0, 7).num, 7);
        assert_eq!(surprise_frame(640.0, 62.0, 7).num, 0);
    }

    #[test]
    fn surprise_flashes_gold_then_empties() {
        let f = surprise_frame(0.0, 62.0, 7);
        assert_eq!((f.arc_color, f.num_color, f.arc_flash), (GOLD, GOLD, 0.0));
        assert_eq!(f.arc_fraction, 0.62);
        assert!(surprise_frame(200.0, 62.0, 7).arc_flash > 0.5);
        let f = surprise_frame(2000.0, 62.0, 7);
        assert_eq!(
            (f.arc_fraction, f.tick_alpha, f.arc_flash),
            (0.0, 0.0, 0.25)
        );
        assert!(f.arc_alpha < 0.3);
        assert!(surprise_frame(500.0, 62.0, 7).num_blur > 0.0);
    }

    #[test]
    fn glow_rises_holds_and_ends() {
        assert_eq!(glow_alpha(0.0), 0.0);
        assert!(glow_alpha(1500.0) > 0.99);
        assert_eq!(glow_alpha(4200.0), 0.0);
    }

    #[test]
    fn sparks_are_deterministic_and_start_on_circle() {
        let a = sparks(9, pos2(60.0, 56.0), 23.0, 0.62, 30);
        assert_eq!(a.len(), 30);
        assert_eq!(a, sparks(9, pos2(60.0, 56.0), 23.0, 0.62, 30));
        assert_ne!(a, sparks(10, pos2(60.0, 56.0), 23.0, 0.62, 30));
        for s in &a {
            assert!(((s.x - 60.0).hypot(s.y - 56.0) - 23.0).abs() < 1e-3);
            assert!((700.0..=1300.0).contains(&s.dur_ms));
            let d = s.dx.hypot(s.dy);
            assert!((24.0 - 1e-3..=74.0 + 1e-3).contains(&d));
        }
    }

    #[test]
    fn spark_flies_and_expires() {
        let s = &sparks(1, pos2(0.0, 0.0), 23.0, 0.5, 4)[0];
        let (p0, sc0, a0) = spark_at(s, 0.0).unwrap();
        assert_eq!((p0.x, p0.y, sc0, a0), (s.x, s.y, 1.0, 1.0));
        let (p, sc, a) = spark_at(s, s.dur_ms as f64 - 1.0).unwrap();
        assert!((p.x - (s.x + s.dx)).abs() < 1.0 && sc < 0.25 && a < 0.05);
        assert!(spark_at(s, s.dur_ms as f64).is_none());
    }
}
