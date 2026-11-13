//! Keyframe track with per-segment easing (motion's semantics).

use super::easing::Ease;

#[derive(Clone, Debug)]
pub struct Track {
    pub dur_ms: f64,
    pub delay_ms: f64,
    /// (time 0..1, value)
    pub keys: Vec<(f32, f32)>,
    pub ease: Ease,
}

impl Track {
    /// Evenly spaced keys when `times` is `None`.
    pub fn new(dur_ms: f64, values: &[f32], times: Option<&[f32]>, ease: Ease) -> Track {
        let n = values.len();
        let keys = values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let t = match times {
                    Some(ts) => ts[i],
                    None if n > 1 => i as f32 / (n - 1) as f32,
                    None => 0.0,
                };
                (t, v)
            })
            .collect();
        Track {
            dur_ms,
            delay_ms: 0.0,
            keys,
            ease,
        }
    }

    pub fn sample(&self, elapsed_ms: f64) -> f32 {
        let first = self.keys[0].1;
        let last = self.keys[self.keys.len() - 1].1;
        let e = elapsed_ms - self.delay_ms;
        if e <= 0.0 {
            return first;
        }
        if e >= self.dur_ms {
            return last;
        }
        let p = (e / self.dur_ms) as f32;
        for w in self.keys.windows(2) {
            let ((t0, v0), (t1, v1)) = (w[0], w[1]);
            if p < t1 {
                if p < t0 || t1 <= t0 {
                    return v0;
                }
                return v0 + (v1 - v0) * self.ease.apply((p - t0) / (t1 - t0));
            }
        }
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_per_segment_with_times() {
        let tr = Track::new(
            4200.0,
            &[0.0, 1.0, 1.0, 0.0],
            Some(&[0.0, 0.12, 0.8, 1.0]),
            Ease::Linear,
        );
        assert_eq!(tr.sample(-5.0), 0.0);
        assert!((tr.sample(252.0) - 0.5).abs() < 1e-3);
        assert_eq!(tr.sample(2000.0), 1.0);
        assert_eq!(tr.sample(9999.0), 0.0);
    }

    #[test]
    fn evenly_spaced_without_times_and_honours_delay() {
        let mut tr = Track::new(1000.0, &[0.0, 1.0, 0.0], None, Ease::Linear);
        tr.delay_ms = 100.0;
        assert_eq!(tr.sample(50.0), 0.0);
        assert!((tr.sample(350.0) - 0.5).abs() < 1e-3);
        assert!((tr.sample(600.0) - 1.0).abs() < 1e-3);
    }
}
