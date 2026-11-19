//! A value that eases towards a target; retargeting starts from what is shown.

use super::easing::Ease;
use egui::Color32;

pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

/// Linear on the premultiplied components, like CSS color transitions (no drift through black).
impl Lerp for Color32 {
    fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
        let c = |a: u8, b: u8| {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Color32::from_rgba_premultiplied(
            c(a.r(), b.r()),
            c(a.g(), b.g()),
            c(a.b(), b.b()),
            c(a.a(), b.a()),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tween<T: Lerp> {
    from: T,
    to: T,
    start: f64,
    dur: f64,
    ease: Ease,
}

impl<T: Lerp> Tween<T> {
    pub fn new(v: T) -> Self {
        Tween {
            from: v,
            to: v,
            start: 0.0,
            dur: 0.0,
            ease: Ease::Linear,
        }
    }

    /// Heads to `to` over `dur` seconds, starting from the currently displayed value.
    pub fn retarget(&mut self, now: f64, to: T, dur: f64, ease: Ease) {
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.dur = dur;
        self.ease = ease;
    }

    pub fn snap(&mut self, v: T) {
        self.from = v;
        self.to = v;
        self.dur = 0.0;
    }

    pub fn value(&self, now: f64) -> T {
        if self.done(now) {
            return self.to;
        }
        let p = ((now - self.start) / self.dur).clamp(0.0, 1.0) as f32;
        T::lerp(self.from, self.to, self.ease.apply(p))
    }

    pub fn done(&self, now: f64) -> bool {
        self.dur <= 0.0 || now >= self.start + self.dur
    }
}

#[cfg(test)]
mod tests {
    use super::super::easing::EASE_RING;
    use super::*;

    #[test]
    fn done_exactly_at_duration() {
        let mut tw = Tween::new(0.0f32);
        tw.retarget(10.0, 1.0, 0.8, EASE_RING);
        assert!(!tw.done(10.79));
        assert!(tw.done(10.8));
        assert_eq!(tw.value(11.0), 1.0);
    }

    #[test]
    fn retarget_mid_flight_starts_from_shown_value() {
        let mut tw = Tween::new(0.0f32);
        tw.retarget(0.0, 1.0, 1.0, Ease::Linear);
        tw.retarget(0.5, 0.0, 1.0, Ease::Linear);
        assert!((tw.value(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn snap_jumps_and_is_done() {
        let mut tw = Tween::new(0.0f32);
        tw.retarget(0.0, 1.0, 1.0, Ease::Linear);
        tw.snap(0.25);
        assert_eq!(tw.value(0.1), 0.25);
        assert!(tw.done(0.1));
    }

    #[test]
    fn transparent_to_opaque_red_keeps_hue() {
        let m = Color32::lerp(Color32::TRANSPARENT, Color32::from_rgb(255, 0, 0), 0.5);
        assert_eq!([m.r(), m.g(), m.b(), m.a()], [128, 0, 0, 128]);
    }

    #[test]
    fn nonlinear_retarget_mid_flight_starts_from_shown_value() {
        let mut tw = Tween::new(0.0f32);
        tw.retarget(0.0, 1.0, 1.0, EASE_RING);
        let shown = tw.value(0.4);
        tw.retarget(0.4, 0.0, 1.0, EASE_RING);
        assert!((tw.value(0.4) - shown).abs() < 1e-6);
    }

    #[test]
    fn colors_lerp_opaque() {
        let a = Color32::from_rgba_unmultiplied(0, 100, 200, 255);
        let b = Color32::from_rgba_unmultiplied(100, 200, 100, 255);
        let m = Color32::lerp(a, b, 0.5).to_srgba_unmultiplied();
        assert_eq!(m, [50, 150, 150, 255]);
    }
}
