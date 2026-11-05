use crate::model::{LimitKey, Plan, Usage};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ResetKind {
    Planned,
    Surprise { early_secs: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetEvent {
    pub key: LimitKey,
    #[serde(flatten)]
    pub kind: ResetKind,
    pub from_used: f64,
}

pub fn detect_resets(prev: &Usage, cur: &Usage, now: DateTime<Utc>) -> Vec<ResetEvent> {
    [LimitKey::Session, LimitKey::Weekly]
        .into_iter()
        .filter_map(|key| {
            let (p, c) = (prev.get(key), cur.get(key));
            if !(p.used >= 1.0 && c.used < p.used - 0.5) {
                return None;
            }
            let kind = match p.resets_at {
                Some(r) if now < r - Duration::seconds(60) => {
                    ResetKind::Surprise { early_secs: (r - now).num_seconds() }
                }
                Some(_) => ResetKind::Planned,
                None => ResetKind::Surprise { early_secs: 0 },
            };
            Some(ResetEvent { key, kind, from_used: p.used })
        })
        .collect()
}

/// `detect_resets`, minus the session ring when the plan has no 5-hour limit.
pub fn detect_resets_for(plan: Option<Plan>, prev: &Usage, cur: &Usage, now: DateTime<Utc>) -> Vec<ResetEvent> {
    let mut events = detect_resets(prev, cur, now);
    if plan.is_some_and(|p| !p.has_session_limit()) {
        events.retain(|e| e.key != LimitKey::Session);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Window;

    fn at(hms: &str) -> DateTime<Utc> {
        format!("2026-11-03T{hms}Z").parse().unwrap()
    }
    fn w(used: f64, resets_hhmm: Option<&str>) -> Window {
        Window { used, resets_at: resets_hhmm.map(|h| at(&format!("{h}:00"))) }
    }
    fn u(session_used: f64, session_resets_hhmm: Option<&str>, weekly_used: f64) -> Usage {
        Usage {
            session: w(session_used, session_resets_hhmm),
            weekly: Window { used: weekly_used, resets_at: Some("2026-11-09T00:00:00Z".parse().unwrap()) },
        }
    }

    #[test] fn planned_at_reset_time() { let e = detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("14:00:20")); assert_eq!(e.len(), 1); assert_eq!(e[0].kind, ResetKind::Planned); assert_eq!(e[0].from_used, 78.0); }
    #[test] fn planned_within_60s_tolerance() { assert_eq!(detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("13:59:10"))[0].kind, ResetKind::Planned); }
    #[test] fn surprise_reports_early_seconds() { let e = detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("11:46:00")); assert_eq!(e[0].kind, ResetKind::Surprise { early_secs: 2 * 3600 + 14 * 60 }); }
    #[test] fn both_at_once() { assert_eq!(detect_resets(&u(78.0, Some("14:00"), 60.0), &u(0.0, None, 0.0), at("12:00:00")).len(), 2); }
    #[test] fn ignores_tiny_drop_and_low_prev() { assert!(detect_resets(&u(50.0, Some("14:00"), 34.0), &u(49.6, Some("14:00"), 34.0), at("12:00:00")).is_empty()); assert!(detect_resets(&u(0.8, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("12:00:00")).is_empty()); }
    #[test]
    fn prev_without_resets_at_counts_as_surprise_zero() {
        let e = detect_resets(&u(40.0, None, 34.0), &u(0.0, None, 34.0), at("12:00:00"));
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].key, LimitKey::Session);
        assert_eq!(e[0].kind, ResetKind::Surprise { early_secs: 0 });
    }
    #[test]
    fn ultra_ignores_session_resets() {
        use crate::model::Plan;
        let (p, c) = (u(78.0, Some("14:00"), 34.0), u(0.0, None, 0.0));
        let e = detect_resets_for(Some(Plan::Ultra), &p, &c, at("14:00:20"));
        assert_eq!(e.iter().map(|e| e.key).collect::<Vec<_>>(), vec![LimitKey::Weekly]);
        assert_eq!(detect_resets_for(Some(Plan::Max), &p, &c, at("14:00:20")).len(), 2);
        assert_eq!(detect_resets_for(None, &p, &c, at("14:00:20")).len(), 2);
    }
    #[test]
    fn serializes_flat_camel_case() {
        let planned = ResetEvent { key: LimitKey::Session, kind: ResetKind::Planned, from_used: 78.0 };
        assert_eq!(serde_json::to_string(&planned).unwrap(), r#"{"key":"session","kind":"planned","fromUsed":78.0}"#);
        let surprise = ResetEvent { key: LimitKey::Weekly, kind: ResetKind::Surprise { early_secs: 8040 }, from_used: 60.0 };
        assert_eq!(serde_json::to_string(&surprise).unwrap(), r#"{"key":"weekly","kind":"surprise","earlySecs":8040,"fromUsed":60.0}"#);
    }
}
