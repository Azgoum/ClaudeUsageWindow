//! Test fixtures: port of the Phase 1 `mockBridge.ts` scenarios and resets.
//!
//! Orgs are `Jane's Organization` and `Acme Studio`; no real account data, no email.
//! "Next Friday 23:59" is computed in UTC (the scenario has no zone); snapshot tests that want
//! a local time pass `tz = UTC` to the formatters.

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use limits_core::{
    AppState, Conn, ErrorInfo, LimitKey, OrgSummary, Pace, Plan, ResetEvent, ResetKind, RingView,
};

fn org_personal() -> OrgSummary {
    OrgSummary {
        uuid: "org-personal".into(),
        name: "Jane's Organization".into(),
        plan: Some(Plan::Max),
    }
}
fn org_ultra() -> OrgSummary {
    OrgSummary {
        uuid: "org-ultra".into(),
        name: "Jane's Organization".into(),
        plan: Some(Plan::Ultra),
    }
}
fn org_team() -> OrgSummary {
    OrgSummary {
        uuid: "org-team".into(),
        name: "Acme Studio".into(),
        plan: Some(Plan::Team),
    }
}

/// Next Friday 23:59 (UTC), always in the future.
fn next_friday_2359(now: DateTime<Utc>) -> DateTime<Utc> {
    let d = now.date_naive();
    let ahead = (5 + 7 - d.weekday().num_days_from_sunday() as i64) % 7;
    let at = |days: i64| {
        let day = d + Duration::days(days);
        Utc.from_utc_datetime(&day.and_hms_opt(23, 59, 0).unwrap())
    };
    let f = at(ahead);
    if f <= now {
        at(ahead + 7)
    } else {
        f
    }
}

struct S {
    used: f64,
    expected: Option<f64>,
    pace: Pace,
    resets_at: Option<DateTime<Utc>>,
}
struct W {
    used: f64,
    expected: f64,
    pace: Pace,
}

fn rings(now: DateTime<Utc>, s: S, w: W) -> [RingView; 2] {
    [
        RingView {
            key: LimitKey::Session,
            used: s.used,
            resets_at: s.resets_at,
            expected: s.expected,
            pace: s.pace,
            no_limit: false,
        },
        RingView {
            key: LimitKey::Weekly,
            used: w.used,
            resets_at: Some(next_friday_2359(now)),
            expected: Some(w.expected),
            pace: w.pace,
            no_limit: false,
        },
    ]
}

fn hours(h: f64) -> Duration {
    Duration::milliseconds((h * 3_600_000.0) as i64)
}

const WEEKLY_ROOM: W = W {
    used: 34.0,
    expected: 45.0,
    pace: Pace::Room,
};

/// Kinds: connected, ontrack, waiting, disconnected, timedout, needsFirefox, expired, error,
/// near, idle, ultra, twoOrgs. Anything else is `connected`.
pub fn scenario(kind: &str, now: DateTime<Utc>) -> AppState {
    let base = AppState {
        conn: Conn::Connected,
        org: Some(org_personal()),
        orgs: vec![org_personal()],
        rings: Some(rings(
            now,
            S {
                used: 62.0,
                expected: Some(48.0),
                pace: Pace::Ahead,
                resets_at: Some(now + hours(2.5)),
            },
            WEEKLY_ROOM,
        )),
        last_ok: Some(now),
        error: None,
        resets: vec![],
        reset_seq: 0,
    };
    let away = || AppState {
        org: None,
        orgs: vec![],
        rings: None,
        last_ok: None,
        ..base.clone()
    };
    match kind {
        "waiting" => AppState {
            conn: Conn::Waiting,
            ..away()
        },
        "disconnected" => AppState {
            conn: Conn::Disconnected { timed_out: false },
            ..away()
        },
        "timedout" => AppState {
            conn: Conn::Disconnected { timed_out: true },
            ..away()
        },
        "needsFirefox" => AppState {
            conn: Conn::NeedsFirefox,
            ..away()
        },
        "expired" => AppState {
            conn: Conn::Expired,
            ..base
        },
        "error" => AppState {
            last_ok: Some(now - Duration::minutes(4)),
            error: Some(ErrorInfo {
                retry_at: now + Duration::minutes(2),
            }),
            ..base
        },
        "near" => AppState {
            rings: Some(rings(
                now,
                S {
                    used: 94.0,
                    expected: Some(80.0),
                    pace: Pace::Ahead,
                    resets_at: Some(now + hours(1.2)),
                },
                W {
                    used: 88.0,
                    expected: 70.0,
                    pace: Pace::Ahead,
                },
            )),
            ..base
        },
        "ontrack" => AppState {
            rings: Some(rings(
                now,
                S {
                    used: 50.0,
                    expected: Some(48.0),
                    pace: Pace::OnTrack,
                    resets_at: Some(now + hours(2.5)),
                },
                WEEKLY_ROOM,
            )),
            ..base
        },
        "idle" => AppState {
            rings: Some(rings(
                now,
                S {
                    used: 0.0,
                    expected: None,
                    pace: Pace::Idle,
                    resets_at: None,
                },
                WEEKLY_ROOM,
            )),
            ..base
        },
        "ultra" => {
            let weekly = rings(
                now,
                S {
                    used: 0.0,
                    expected: None,
                    pace: Pace::Idle,
                    resets_at: None,
                },
                WEEKLY_ROOM,
            )[1]
            .clone();
            let session = RingView {
                key: LimitKey::Session,
                used: 0.0,
                resets_at: None,
                expected: None,
                pace: Pace::Idle,
                no_limit: true,
            };
            AppState {
                org: Some(org_ultra()),
                orgs: vec![org_ultra()],
                rings: Some([session, weekly]),
                ..base
            }
        }
        "twoOrgs" => AppState {
            orgs: vec![org_personal(), org_team()],
            ..base
        },
        _ => base,
    }
}

/// How early the mock's surprises come: 2h14 (session), 3d 12h (weekly).
fn early_secs(key: LimitKey) -> i64 {
    match key {
        LimitKey::Session => 8040,
        LimitKey::Weekly => 302_400,
    }
}

/// Applies a reset to `base`: planned / surprise (session ring), plannedWeekly / surpriseWeekly
/// (weekly ring), both (two surprises), bothPlanned (two planned resets). `now` is `base.last_ok`
/// (what `scenario` sets), else the wall clock. Returns `base` unchanged without rings.
pub fn reset_state(base: &AppState, kind: &str, seq: u64) -> AppState {
    let Some(rs) = &base.rings else {
        return base.clone();
    };
    let now = base.last_ok.unwrap_or_else(Utc::now);
    let keys: &[LimitKey] = match kind {
        "both" | "bothPlanned" => &[LimitKey::Session, LimitKey::Weekly],
        "plannedWeekly" | "surpriseWeekly" => &[LimitKey::Weekly],
        _ => &[LimitKey::Session],
    };
    let planned = matches!(kind, "planned" | "bothPlanned" | "plannedWeekly");
    let events = keys
        .iter()
        .map(|&key| {
            let from_used = rs.iter().find(|r| r.key == key).map_or(0.0, |r| r.used);
            ResetEvent {
                key,
                kind: if planned {
                    ResetKind::Planned
                } else {
                    ResetKind::Surprise {
                        early_secs: early_secs(key),
                    }
                },
                from_used,
            }
        })
        .collect();
    let next = rs.clone().map(|r| {
        if !keys.contains(&r.key) {
            return r;
        }
        RingView {
            used: 0.0,
            pace: Pace::Room,
            resets_at: Some(match r.key {
                LimitKey::Session => now + hours(5.0),
                LimitKey::Weekly => next_friday_2359(now),
            }),
            expected: if r.key == LimitKey::Session {
                Some(0.0)
            } else {
                r.expected
            },
            ..r
        }
    });
    AppState {
        rings: Some(next),
        resets: events,
        reset_seq: seq,
        last_ok: Some(now),
        ..base.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        // Tuesday 2026-11-03 10:30 UTC.
        Utc.with_ymd_and_hms(2026, 11, 3, 10, 30, 0).unwrap()
    }

    #[test]
    fn friday_is_next_and_in_the_future() {
        let f = next_friday_2359(now());
        assert_eq!(f, Utc.with_ymd_and_hms(2026, 11, 6, 23, 59, 0).unwrap());
        // On a Friday after 23:59 it moves a week on.
        let late = Utc.with_ymd_and_hms(2026, 11, 6, 23, 59, 30).unwrap();
        assert_eq!(
            next_friday_2359(late),
            Utc.with_ymd_and_hms(2026, 11, 13, 23, 59, 0).unwrap()
        );
    }

    #[test]
    fn connected_is_the_default() {
        let s = scenario("connected", now());
        assert_eq!(s, scenario("whatever", now()));
        assert_eq!(s.conn, Conn::Connected);
        assert_eq!(s.org.unwrap().name, "Jane's Organization");
        let [session, weekly] = s.rings.unwrap();
        assert_eq!((session.used, session.pace), (62.0, Pace::Ahead));
        assert_eq!((weekly.used, weekly.pace), (34.0, Pace::Room));
        assert_eq!(s.last_ok, Some(now()));
    }

    #[test]
    fn ontrack_scenario() {
        let [session, _] = scenario("ontrack", now()).rings.unwrap();
        assert_eq!(
            (session.used, session.expected, session.pace),
            (50.0, Some(48.0), Pace::OnTrack)
        );
    }

    #[test]
    fn away_kinds_have_no_org_or_rings() {
        for (kind, conn) in [
            ("waiting", Conn::Waiting),
            ("disconnected", Conn::Disconnected { timed_out: false }),
            ("timedout", Conn::Disconnected { timed_out: true }),
            ("needsFirefox", Conn::NeedsFirefox),
        ] {
            let s = scenario(kind, now());
            assert_eq!(s.conn, conn, "{kind}");
            assert!(s.org.is_none() && s.orgs.is_empty() && s.rings.is_none());
            assert!(s.last_ok.is_none());
        }
        let expired = scenario("expired", now());
        assert_eq!(expired.conn, Conn::Expired);
        assert!(expired.rings.is_some());
    }

    #[test]
    fn error_scenario_has_a_retry() {
        let s = scenario("error", now());
        assert_eq!(s.error.unwrap().retry_at, now() + Duration::minutes(2));
        assert_eq!(s.last_ok, Some(now() - Duration::minutes(4)));
    }

    #[test]
    fn near_and_idle_and_ultra() {
        let [s, w] = scenario("near", now()).rings.unwrap();
        assert_eq!((s.used, w.used), (94.0, 88.0));
        let [s, _] = scenario("idle", now()).rings.unwrap();
        assert_eq!((s.pace, s.resets_at), (Pace::Idle, None));
        let u = scenario("ultra", now());
        assert_eq!(u.org.as_ref().unwrap().plan, Some(Plan::Ultra));
        assert!(u.rings.unwrap()[0].no_limit);
    }

    #[test]
    fn two_orgs_names_never_an_email() {
        let s = scenario("twoOrgs", now());
        let names: Vec<_> = s.orgs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Jane's Organization", "Acme Studio"]);
        assert!(names.iter().all(|n| !n.contains('@')));
    }

    #[test]
    fn planned_resets_the_session_ring() {
        let base = scenario("connected", now());
        let s = reset_state(&base, "planned", 7);
        assert_eq!(s.reset_seq, 7);
        assert_eq!(
            s.resets,
            [ResetEvent {
                key: LimitKey::Session,
                kind: ResetKind::Planned,
                from_used: 62.0
            }]
        );
        let [session, weekly] = s.rings.unwrap();
        assert_eq!(
            (session.used, session.pace, session.expected),
            (0.0, Pace::Room, Some(0.0))
        );
        assert_eq!(session.resets_at, Some(now() + hours(5.0)));
        assert_eq!(weekly.used, 34.0);
    }

    #[test]
    fn surprise_kinds() {
        let base = scenario("connected", now());
        let s = reset_state(&base, "surpriseWeekly", 1);
        assert_eq!(
            s.resets[0].kind,
            ResetKind::Surprise {
                early_secs: 302_400
            }
        );
        assert_eq!(s.resets[0].key, LimitKey::Weekly);
        let both = reset_state(&base, "both", 2);
        assert_eq!(both.resets.len(), 2);
        assert_eq!(
            both.resets[0].kind,
            ResetKind::Surprise { early_secs: 8040 }
        );
        let bp = reset_state(&base, "bothPlanned", 3);
        assert!(bp.resets.iter().all(|e| e.kind == ResetKind::Planned));
        let pw = reset_state(&base, "plannedWeekly", 4);
        assert_eq!(pw.resets.len(), 1);
        assert_eq!(pw.rings.unwrap()[1].used, 0.0);
    }

    #[test]
    fn reset_without_rings_is_a_no_op() {
        let base = scenario("waiting", now());
        assert_eq!(reset_state(&base, "planned", 9), base);
    }
}
