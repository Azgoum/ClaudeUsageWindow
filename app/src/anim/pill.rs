//! The single pill shown for one batch of reset events, and its font fitting.

use crate::ui::format::fmt_dur;
use crate::ui::i18n::t;
use crate::ui::Lang;
use limits_core::{LimitKey, ResetEvent, ResetKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillKind {
    Calm,
    Gold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PillSpec {
    pub kind: PillKind,
    pub text: String,
    pub hold_ms: u64,
}

/// Prototype `chip` holds: 2200 ms after a planned reset, 2800 ms after a surprise.
const HOLD_CALM: u64 = 2200;
const HOLD_GOLD: u64 = 2800;

pub fn pill_for(events: &[ResetEvent], lang: Lang) -> Option<PillSpec> {
    let first = events.first()?;
    let l = t(lang);
    let gold = events
        .iter()
        .any(|e| matches!(e.kind, ResetKind::Surprise { .. }));
    let kind = if gold { PillKind::Gold } else { PillKind::Calm };
    let text = if events.len() > 1 {
        let s = if gold {
            l.chip_both
        } else {
            l.chip_both_planned
        };
        s.to_string()
    } else {
        match first.kind {
            ResetKind::Surprise { early_secs } => (l.surprise)(&fmt_dur(early_secs * 1000, lang)),
            ResetKind::Planned => match first.key {
                LimitKey::Session => l.chip_s.to_string(),
                LimitKey::Weekly => l.chip_w.to_string(),
            },
        }
    };
    let hold_ms = if gold { HOLD_GOLD } else { HOLD_CALM };
    Some(PillSpec {
        kind,
        text,
        hold_ms,
    })
}

/// Pill base font size and the smallest it may shrink to (px).
pub const PILL_FONT: f32 = 10.0;
pub const PILL_FONT_MIN: f32 = 8.5;

/// Font size that keeps the pill on one line inside `avail`, or `None` if the base size fits.
/// `width` is the pill's measured width at the base size and `chrome` its padding + border,
/// which don't scale with the font. Rounded down to a quarter px, never below the floor.
pub fn pill_font_size(width: f32, avail: f32, chrome: f32) -> Option<f32> {
    if width <= avail {
        return None;
    }
    let fit = PILL_FONT * (avail - chrome) / (width - chrome);
    Some(PILL_FONT_MIN.max((fit * 4.0).floor() / 4.0))
}

/// Pill opacity and transform at one instant.
#[derive(Clone, Debug, PartialEq)]
pub struct PillFrame {
    pub alpha: f32,
    pub dy: f32,
    pub scale: f32,
}

/// The pill `ms` after it was shown (spring in, hold, fade out); `None` once it is gone.
pub fn pill_frame(ms: f64, hold_ms: f64, reduced: bool) -> Option<PillFrame> {
    use super::easing::{Ease, EASE_DEFAULT};
    let (in_ms, out_ms) = if reduced {
        (300.0, 400.0)
    } else {
        (650.0, 400.0)
    };
    let out_at = in_ms + hold_ms;
    if ms >= out_at + out_ms {
        return None;
    }
    let lerp = |a: f32, b: f32, e: f32| a + (b - a) * e;
    if ms < in_ms {
        let p = (ms.max(0.0) / in_ms) as f32;
        return Some(if reduced {
            PillFrame {
                alpha: EASE_DEFAULT.apply(p),
                dy: 0.0,
                scale: 1.0,
            }
        } else {
            let e = Ease::Spring.apply(p);
            PillFrame {
                alpha: e.clamp(0.0, 1.0),
                dy: lerp(6.0, 0.0, e),
                scale: lerp(0.85, 1.0, e),
            }
        });
    }
    if ms < out_at {
        return Some(PillFrame {
            alpha: 1.0,
            dy: 0.0,
            scale: 1.0,
        });
    }
    let e = EASE_DEFAULT.apply(((ms - out_at) / out_ms) as f32);
    Some(if reduced {
        PillFrame {
            alpha: 1.0 - e,
            dy: 0.0,
            scale: 1.0,
        }
    } else {
        PillFrame {
            alpha: 1.0 - e,
            dy: lerp(0.0, -4.0, e),
            scale: lerp(1.0, 0.96, e),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: LimitKey, kind: ResetKind, from_used: f64) -> ResetEvent {
        ResetEvent {
            key,
            kind,
            from_used,
        }
    }
    fn planned_s() -> ResetEvent {
        ev(LimitKey::Session, ResetKind::Planned, 62.0)
    }
    fn planned_w() -> ResetEvent {
        ev(LimitKey::Weekly, ResetKind::Planned, 34.0)
    }
    fn surprise_s() -> ResetEvent {
        ev(
            LimitKey::Session,
            ResetKind::Surprise { early_secs: 8040 },
            62.0,
        )
    }
    fn surprise_w() -> ResetEvent {
        ev(
            LimitKey::Weekly,
            ResetKind::Surprise {
                early_secs: 302_400,
            },
            34.0,
        )
    }
    fn spec(kind: PillKind, text: &str, hold_ms: u64) -> Option<PillSpec> {
        Some(PillSpec {
            kind,
            text: text.into(),
            hold_ms,
        })
    }

    #[test]
    fn shows_nothing_without_events() {
        assert_eq!(pill_for(&[], Lang::En), None);
    }

    #[test]
    fn one_planned_session_reset_is_calm_chip_s() {
        assert_eq!(
            pill_for(&[planned_s()], Lang::En),
            spec(PillKind::Calm, "✓ Session reset · fresh 5 hours", 2200)
        );
    }

    #[test]
    fn one_planned_weekly_reset_is_calm_chip_w() {
        assert_eq!(
            pill_for(&[planned_w()], Lang::Fr),
            spec(
                PillKind::Calm,
                "✓ Nouvelle semaine · limite hebdo remise à zéro",
                2200
            )
        );
    }

    #[test]
    fn one_surprise_is_gold_with_the_early_duration() {
        assert_eq!(
            pill_for(&[surprise_s()], Lang::En),
            spec(PillKind::Gold, "★ Surprise reset · 2h14 early", 2800)
        );
        assert_eq!(
            pill_for(&[surprise_w()], Lang::Fr).unwrap().text,
            "★ Reset surprise · 3 j 12 h en avance"
        );
    }

    #[test]
    fn two_resets_with_any_surprise_are_gold_chip_both() {
        let both = spec(PillKind::Gold, "★ Surprise reset · both limits", 2800);
        assert_eq!(pill_for(&[surprise_s(), surprise_w()], Lang::En), both);
        assert_eq!(pill_for(&[planned_s(), surprise_w()], Lang::En), both);
        assert_eq!(pill_for(&[surprise_s(), planned_w()], Lang::En), both);
    }

    #[test]
    fn two_planned_resets_are_calm_chip_both_planned() {
        assert_eq!(
            pill_for(&[planned_s(), planned_w()], Lang::En),
            spec(PillKind::Calm, "✓ Both limits reset", 2200)
        );
    }

    #[test]
    fn font_keeps_base_size_when_the_pill_fits() {
        assert_eq!(pill_font_size(170.0, 236.0, 18.0), None);
        assert_eq!(pill_font_size(236.0, 236.0, 18.0), None);
    }

    #[test]
    fn font_shrinks_only_the_text_rounded_down_to_a_quarter_px() {
        // text 230 px at 10 px; room for 218 px of text -> 9.478... -> 9.25
        let fs = pill_font_size(248.0, 236.0, 18.0).unwrap();
        assert_eq!(fs, 9.25);
        assert!((248.0 - 18.0) * (fs / 10.0) + 18.0 <= 236.0);
    }

    #[test]
    fn font_never_goes_below_the_floor() {
        assert_eq!(pill_font_size(400.0, 236.0, 18.0), Some(PILL_FONT_MIN));
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn pill_gone_after_hold() {
        assert!(pill_frame(650.0 + 2200.0 + 401.0, 2200.0, false).is_none());
        assert!(pill_frame(650.0 + 2200.0 + 200.0, 2200.0, false).is_some());
    }

    #[test]
    fn pill_enters_holds_and_leaves() {
        let f = pill_frame(0.0, 2200.0, false).unwrap();
        assert_eq!((f.alpha, f.dy, f.scale), (0.0, 6.0, 0.85));
        let f = pill_frame(1500.0, 2200.0, false).unwrap();
        assert_eq!((f.alpha, f.dy, f.scale), (1.0, 0.0, 1.0));
        let f = pill_frame(650.0 + 2200.0 + 399.0, 2200.0, false).unwrap();
        assert!(f.alpha < 0.05 && f.dy < -3.5);
    }

    #[test]
    fn reduced_motion_only_fades() {
        let f = pill_frame(150.0, 2200.0, true).unwrap();
        assert!(f.alpha > 0.0 && f.alpha < 1.0 && f.dy == 0.0 && f.scale == 1.0);
        assert!(pill_frame(300.0 + 2200.0 + 400.0, 2200.0, true).is_none());
    }
}
