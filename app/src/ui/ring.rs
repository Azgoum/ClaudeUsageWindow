//! Ring geometry and colors (port of Phase 1 (git bcaa1bf) `ui/src/lib/ring.ts`).

use chrono::{DateTime, Utc};
use egui::Color32;
use limits_core::{Pace, RingView};

use super::colors;
use super::format::fmt_dur;
use super::i18n::t;
use super::Lang;

/// Circumference of the r=23 ring.
pub const CIRC: f32 = 2.0 * std::f32::consts::PI * 23.0;

/// Filled fraction of the arc, 0..=1 (replaces `arcOffset`).
pub fn arc_fraction(used: f64) -> f32 {
    (used.clamp(0.0, 100.0) / 100.0) as f32
}

/// Angle of the pace tick: expected x 3.6 degrees.
pub fn tick_angle_deg(expected: Option<f64>) -> f32 {
    (expected.unwrap_or(0.0) * 3.6) as f32
}

pub fn tick_opacity(expected: Option<f64>) -> f32 {
    if expected.is_none() {
        0.0
    } else {
        0.95
    }
}

pub fn ring_number(used: f64) -> i64 {
    used.floor() as i64
}

pub fn num_fill(r: &RingView) -> Color32 {
    if r.no_limit || r.pace == Pace::Idle {
        colors::MUTE
    } else {
        colors::pace_color(r.pace)
    }
}

/// Arc colour: a faint grey for a ring without limit, else the pace colour.
pub fn arc_stroke(r: &RingView) -> Color32 {
    if r.no_limit {
        colors::NO_LIMIT_ARC
    } else {
        colors::pace_color(r.pace)
    }
}

pub fn is_near_limit(rings: Option<&[RingView; 2]>) -> bool {
    rings.is_some_and(|rs| rs.iter().any(|r| r.used >= 90.0))
}

pub fn err_line(retry_at: DateTime<Utc>, now: DateTime<Utc>, lang: Lang) -> String {
    (t(lang).err)(&fmt_dur((retry_at - now).num_milliseconds(), lang))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};
    use limits_core::{LimitKey, Pace, RingView};

    fn cet() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }
    fn local(m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        cet()
            .with_ymd_and_hms(2026, m, d, h, min, 0)
            .unwrap()
            .with_timezone(&Utc)
    }
    fn now() -> DateTime<Utc> {
        local(11, 3, 11, 30)
    }
    fn ring() -> RingView {
        RingView {
            key: LimitKey::Session,
            used: 62.0,
            resets_at: Some(local(11, 3, 14, 0)),
            expected: Some(48.0),
            pace: Pace::Ahead,
            no_limit: false,
        }
    }
    fn idle_ring() -> RingView {
        RingView {
            pace: Pace::Idle,
            expected: None,
            resets_at: None,
            ..ring()
        }
    }

    #[test]
    fn circumference_of_r23() {
        assert!((CIRC - 2.0 * std::f32::consts::PI * 23.0).abs() < 1e-4);
    }
    #[test]
    fn arc_fraction_empties_at_0_fills_at_100_clamped() {
        assert_eq!(arc_fraction(0.0), 0.0);
        assert_eq!(arc_fraction(100.0), 1.0);
        assert_eq!(arc_fraction(50.0), 0.5);
        assert_eq!(arc_fraction(140.0), 1.0);
        assert_eq!(arc_fraction(-5.0), 0.0);
    }
    #[test]
    fn tick_at_expected_times_3_6_hidden_when_unknown() {
        assert_eq!(tick_angle_deg(Some(50.0)), 180.0);
        assert_eq!(tick_angle_deg(None), 0.0);
        assert_eq!(tick_opacity(Some(48.0)), 0.95);
        assert_eq!(tick_opacity(None), 0.0);
    }
    #[test]
    fn number_is_floored_idle_number_is_gray() {
        assert_eq!(ring_number(62.9), 62);
        assert_eq!(num_fill(&ring()), colors::RED);
        assert_eq!(num_fill(&idle_ring()), colors::MUTE);
    }

    #[test]
    fn near_limit_when_any_ring_at_or_above_90() {
        let w = |used| RingView {
            key: LimitKey::Weekly,
            used,
            ..ring()
        };
        let s = |used| RingView { used, ..ring() };
        assert!(is_near_limit(Some(&[s(90.0), w(10.0)])));
        assert!(!is_near_limit(Some(&[s(89.9), w(10.0)])));
        assert!(!is_near_limit(None));
    }

    #[test]
    fn error_line_counts_down_to_retry_at() {
        assert_eq!(
            err_line(now() + chrono::Duration::minutes(2), now(), Lang::En),
            "⚠ claude.ai not responding · retry in 2 min"
        );
        assert_eq!(
            err_line(now() + chrono::Duration::seconds(30), now(), Lang::Fr),
            "⚠ claude.ai ne répond pas · nouvel essai dans 30 s"
        );
    }

    fn no_limit_ring() -> RingView {
        RingView {
            used: 0.0,
            resets_at: None,
            expected: None,
            pace: Pace::Idle,
            no_limit: true,
            ..ring()
        }
    }
    #[test]
    fn no_limit_ring_is_greyed_with_no_pace_tick() {
        let nl = no_limit_ring();
        assert_eq!(num_fill(&nl), colors::MUTE);
        assert_eq!(arc_stroke(&nl), Color32::from_white_alpha(46));
        assert_eq!(arc_stroke(&ring()), colors::RED);
        assert_eq!(tick_opacity(nl.expected), 0.0);
    }
}
