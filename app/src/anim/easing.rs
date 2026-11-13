//! Easing curves: CSS cubic-bezier, CSS `linear()` and the prototype's spring.

use std::sync::OnceLock;

/// The prototype's spring curve (CSS linear()).
pub const SPRING: &str = "linear(0, 0.009, 0.035 2.1%, 0.141, 0.281 6.7%, 0.723 12.9%, 0.938 16.7%, 1.017, 1.077, 1.121, 1.149 24.3%, 1.159, 1.163, 1.161, 1.154 29.9%, 1.129 32.8%, 1.051 39.6%, 1.017 43.1%, 0.991, 0.977 51%, 0.974 53.8%, 0.975 57.1%, 0.997 69.8%, 1.003 76.9%, 1.004 83.8%, 1)";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    CubicBezier(f32, f32, f32, f32),
    Spring,
}

pub const EASE_DEFAULT: Ease = Ease::CubicBezier(0.2, 0.8, 0.2, 1.0);
/// CSS `ease`.
pub const EASE_CSS: Ease = Ease::CubicBezier(0.25, 0.1, 0.25, 1.0);
pub const EASE_IN_OUT: Ease = Ease::CubicBezier(0.42, 0.0, 0.58, 1.0);
pub const EASE_OUT: Ease = Ease::CubicBezier(0.0, 0.0, 0.58, 1.0);
pub const EASE_RING: Ease = Ease::CubicBezier(0.2, 0.9, 0.25, 1.05);

impl Ease {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Ease::Linear => t.clamp(0.0, 1.0),
            Ease::CubicBezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, t),
            Ease::Spring => spring(t),
        }
    }
}

fn bez(a: f32, b: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
}

fn bez_d(a: f32, b: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
}

/// Solves x(s) = t (Newton, bisection fallback) and returns y(s); y may overshoot 1.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let mut s = t;
    let mut solved = false;
    for _ in 0..8 {
        let err = bez(x1, x2, s) - t;
        if err.abs() < 1e-6 {
            solved = true;
            break;
        }
        let d = bez_d(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s -= err / d;
    }
    if !solved || !(0.0..=1.0).contains(&s) {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        s = t;
        for _ in 0..40 {
            let x = bez(x1, x2, s);
            if (x - t).abs() < 1e-6 {
                break;
            }
            if x < t {
                lo = s;
            } else {
                hi = s;
            }
            s = (lo + hi) / 2.0;
        }
    }
    bez(y1, y2, s)
}

/// A CSS `linear(...)` easing function.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearEasing {
    xs: Vec<f32>,
    ys: Vec<f32>,
}

impl LinearEasing {
    /// Same rules as CSS: unpositioned stops are spread evenly between positioned ones.
    #[allow(clippy::needless_range_loop)] // index-based port of the CSS fill rules
    pub fn parse(spec: &str) -> LinearEasing {
        let body = spec.trim();
        let body = body.strip_prefix("linear(").unwrap_or(body);
        let body = body.strip_suffix(')').unwrap_or(body);
        let mut ys = Vec::new();
        let mut xs: Vec<Option<f32>> = Vec::new();
        for part in body.split(',') {
            let mut it = part.split_whitespace();
            ys.push(it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0));
            xs.push(it.next().and_then(|p| {
                p.trim_end_matches('%')
                    .parse::<f32>()
                    .ok()
                    .map(|v| v / 100.0)
            }));
        }
        let n = xs.len();
        if xs[0].is_none() {
            xs[0] = Some(0.0);
        }
        if xs[n - 1].is_none() {
            xs[n - 1] = Some(1.0);
        }
        // Positions never go backwards.
        for i in 1..n {
            if let Some(x) = xs[i] {
                xs[i] = Some(x.max(xs[i - 1].unwrap_or(0.0)));
            }
        }
        // Unpositioned runs are spread evenly between known ones.
        for i in 1..n {
            if xs[i].is_some() {
                continue;
            }
            let mut j = i;
            while xs[j].is_none() {
                j += 1;
            }
            let a = xs[i - 1].unwrap();
            let b = xs[j].unwrap();
            for k in i..j {
                xs[k] = Some(a + (b - a) * (k - i + 1) as f32 / (j - i + 1) as f32);
            }
        }
        LinearEasing {
            xs: xs.into_iter().map(Option::unwrap).collect(),
            ys,
        }
    }

    pub fn at(&self, t: f32) -> f32 {
        let last = self.ys.len() - 1;
        if t <= 0.0 {
            return self.ys[0];
        }
        if t >= 1.0 {
            return self.ys[last];
        }
        let mut i = 1;
        while i < last && self.xs[i] < t {
            i += 1;
        }
        let (x0, x1) = (self.xs[i - 1], self.xs[i]);
        if x1 == x0 {
            self.ys[i]
        } else {
            self.ys[i - 1] + (self.ys[i] - self.ys[i - 1]) * (t - x0) / (x1 - x0)
        }
    }
}

/// The spring easing (parsed once).
pub fn spring(t: f32) -> f32 {
    static S: OnceLock<LinearEasing> = OnceLock::new();
    S.get_or_init(|| LinearEasing::parse(SPRING)).at(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn interpolates_evenly_spaced_points() {
        let f = LinearEasing::parse("linear(0, 0.5, 1)");
        assert_eq!(f.at(0.0), 0.0);
        assert!(close(f.at(0.25), 0.25));
        assert!(close(f.at(0.5), 0.5));
        assert_eq!(f.at(1.0), 1.0);
    }

    #[test]
    fn honours_explicit_stops_and_spreads_the_unpositioned_ones() {
        let f = LinearEasing::parse("linear(0, 0.2, 1 80%, 1)");
        assert!(close(f.at(0.4), 0.2)); // 0.2 sits halfway between 0% and 80%
        assert!(close(f.at(0.6), 0.6));
        assert!(close(f.at(0.9), 1.0));
    }

    #[test]
    fn clamps_the_input() {
        let f = LinearEasing::parse("linear(0, 1)");
        assert_eq!(f.at(-1.0), 0.0);
        assert_eq!(f.at(2.0), 1.0);
    }

    #[test]
    fn spring_is_the_prototype_constant() {
        assert!(SPRING.starts_with("linear(0, 0.009, 0.035 2.1%, 0.141"));
        assert!(SPRING.ends_with("1.004 83.8%, 1)"));
    }

    #[test]
    fn spring_overshoots_then_settles_at_one() {
        assert_eq!(spring(0.0), 0.0);
        assert!(close(spring(0.021), 0.035));
        assert!(close(spring(0.243), 1.149));
        assert_eq!(spring(1.0), 1.0);
        assert_eq!(Ease::Spring.apply(1.0), 1.0);
    }

    #[test]
    fn cubic_bezier_endpoints_and_known_point() {
        assert_eq!(EASE_CSS.apply(0.0), 0.0);
        assert_eq!(EASE_CSS.apply(1.0), 1.0);
        assert!((EASE_CSS.apply(0.5) - 0.8024).abs() < 1e-3);
        assert!(EASE_RING.apply(0.8) > 1.0); // overshoots
    }
}
