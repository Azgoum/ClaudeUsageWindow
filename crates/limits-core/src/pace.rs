use crate::model::{LimitKey, Window};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    Ahead,
    OnTrack,
    Room,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaceInfo {
    pub expected: Option<f64>,
    pub diff: Option<f64>,
    pub pace: Pace,
}

pub fn window_len(k: LimitKey) -> Duration {
    match k {
        LimitKey::Session => Duration::hours(5),
        LimitKey::Weekly => Duration::days(7),
    }
}

pub fn pace(k: LimitKey, w: &Window, now: DateTime<Utc>) -> PaceInfo {
    let Some(resets_at) = w.resets_at else {
        return PaceInfo { expected: None, diff: None, pace: Pace::Idle };
    };
    let len = window_len(k);
    let elapsed = now - (resets_at - len);
    let expected = (elapsed.num_milliseconds() as f64 / len.num_milliseconds() as f64 * 100.0)
        .clamp(0.0, 100.0);
    let diff = w.used - expected;
    let pace = if diff > 5.0 {
        Pace::Ahead
    } else if diff < -5.0 {
        Pace::Room
    } else {
        Pace::OnTrack
    };
    PaceInfo { expected: Some(expected), diff: Some(diff), pace }
}

/// Poll back-off after `consecutive_failures` failures: 30 s doubling, capped at 10 min.
pub fn backoff_delay(consecutive_failures: u32) -> std::time::Duration {
    let secs = 30u64.saturating_mul(1u64 << consecutive_failures.min(10));
    std::time::Duration::from_secs(secs.min(600))
}

#[cfg(test)]
mod tests {
    use super::*;
    use LimitKey::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    fn w(used: f64, resets: &str) -> Window {
        Window { used, resets_at: Some(t(resets)) }
    }

    // session resets 14:00 UTC; window started 09:00
    #[test] fn ahead_when_more_than_5_over() { let p = pace(Session, &w(62.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:24:00Z")); assert_eq!(p.expected.unwrap().round(), 48.0); assert_eq!(p.pace, Pace::Ahead); }
    #[test] fn on_track_within_5() { assert_eq!(pace(Session, &w(52.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:24:00Z")).pace, Pace::OnTrack); }
    #[test] fn room_when_more_than_5_under() { assert_eq!(pace(Weekly, &w(34.0, "2026-11-06T23:59:00Z"), t("2026-11-03T11:30:00Z")).pace, Pace::Room); }
    #[test]
    fn exactly_plus_5_is_on_track() {
        // 11:30 is exactly half of the 09:00-14:00 window -> expected 50.0
        let p = pace(Session, &w(55.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:30:00Z"));
        assert_eq!(p.expected, Some(50.0));
        assert_eq!(p.diff, Some(5.0));
        assert_eq!(p.pace, Pace::OnTrack);
        let p = pace(Session, &w(45.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:30:00Z"));
        assert_eq!(p.pace, Pace::OnTrack);
    }
    #[test] fn idle_session() { let p = pace(Session, &Window { used: 0.0, resets_at: None }, t("2026-11-03T11:00:00Z")); assert_eq!(p.pace, Pace::Idle); assert!(p.expected.is_none()); }
    #[test] fn idle_weekly_without_resets_at() { assert_eq!(pace(Weekly, &Window::idle(), t("2026-11-03T11:00:00Z")).pace, Pace::Idle); }
    #[test] fn expected_clamped_after_reset_time() { assert_eq!(pace(Session, &w(10.0, "2026-11-03T14:00:00Z"), t("2026-11-03T15:00:00Z")).expected, Some(100.0)); }
    #[test] fn backoff_schedule() { let s: Vec<u64> = (0..7).map(|n| backoff_delay(n).as_secs()).collect(); assert_eq!(s, vec![30, 60, 120, 240, 480, 600, 600]); }
    #[test] fn pace_serializes_lowercase() { assert_eq!(serde_json::to_string(&Pace::OnTrack).unwrap(), "\"ontrack\""); }
}
