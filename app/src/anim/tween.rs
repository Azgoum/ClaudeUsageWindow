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

/// Linear in unmultiplied sRGB, like CSS color transitions.
impl Lerp for Color32 {
    fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
        let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
        let c = |i: usize| {
            (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Color32::from_rgba_unmultiplied(c(0), c(1), c(2), c(3))
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
    fn colors_lerp_in_unmultiplied_srgb() {
        let a = Color32::from_rgba_unmultiplied(0, 100, 200, 255);
        let b = Color32::from_rgba_unmultiplied(100, 200, 100, 255);
        let m = Color32::lerp(a, b, 0.5).to_srgba_unmultiplied();
        assert_eq!(m, [50, 150, 150, 255]);
    }
}
