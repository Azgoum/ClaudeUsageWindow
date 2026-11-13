//! Ring entrance and refresh-button press (port of `Ring.tsx` entrance, `HoverControls.tsx`).

use super::easing::{spring, EASE_RING};
use super::track::Track;

const ENTER_MS: f64 = 1200.0;
const ENTER_STAGGER_MS: f64 = 90.0;

/// Drawn arc fraction `ms` after the rings appear: springs from empty to `target`, 90 ms apart.
pub fn entrance_fraction(ms: f64, index: usize, target: f32) -> f32 {
    let mut t = Track::new(ENTER_MS, &[0.0, target], None, EASE_RING);
    t.delay_ms = index as f64 * ENTER_STAGGER_MS;
    t.sample(ms)
}

/// The request goes out while the icon is still turning.
pub const REFRESH_SEND_AT: f64 = 350.0;
pub const REFRESH_END: f64 = 900.0;

fn press_scale(p: f32) -> f32 {
    if p < 0.25 {
        1.0 - 0.14 * (p / 0.25)
    } else {
        0.86 + 0.14 * (p - 0.25) / 0.75
    }
}

/// (button scale, icon turns 0..1) `ms` after the click: press 500 ms, one turn 900 ms, spring.
pub fn refresh_press(ms: f64) -> (f32, f32) {
    let press = spring((ms / 500.0).clamp(0.0, 1.0) as f32);
    let turn = spring((ms / 900.0).clamp(0.0, 1.0) as f32);
    (press_scale(press), turn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrance_staggers() {
        assert_eq!(entrance_fraction(90.0, 1, 0.34), 0.0);
        assert_eq!(entrance_fraction(1290.0, 1, 0.34), 0.34);
        assert!(entrance_fraction(500.0, 1, 0.34) > 0.0);
        assert!(entrance_fraction(1000.0, 0, 0.5) > 0.4);
    }

    #[test]
    fn refresh_press_dips_to_086_and_turns_once() {
        assert_eq!(refresh_press(0.0), (1.0, 0.0));
        assert_eq!(refresh_press(REFRESH_END), (1.0, 1.0));
        assert!(refresh_press(60.0).0 < 1.0);
        assert!(refresh_press(500.0).0 > 0.99);
        assert!(refresh_press(450.0).1 > 0.3);
    }
}
