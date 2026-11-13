//! Ring geometry, colors and tooltip text (port of `ui/src/lib/ring.ts`).

use chrono::{DateTime, FixedOffset, Utc};
use egui::Color32;
use limits_core::{LimitKey, OrgSummary, Pace, Plan, RingView};

use super::colors;
use super::format::{fmt_day, fmt_dur, fmt_time};
use super::i18n::t;
use super::Lang;

/// JS `Math.round`: halves round toward +infinity (-2.5 -> -2).
fn js_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

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

pub fn ago_text(last_ok: Option<DateTime<Utc>>, now: DateTime<Utc>, lang: Lang) -> Option<String> {
    last_ok.map(|ok| {
        let ms = (now - ok).num_milliseconds().max(1000);
        (t(lang).ago)(&fmt_dur(ms, lang))
    })
}

pub fn err_line(retry_at: DateTime<Utc>, now: DateTime<Utc>, lang: Lang) -> String {
    (t(lang).err)(&fmt_dur((retry_at - now).num_milliseconds(), lang))
}

#[derive(Clone, Debug, PartialEq)]
pub enum TipModel {
    NoLimit {
        title: String,
        line: String,
    },
    Idle {
        title: String,
        line: String,
        sub: String,
    },
    Active {
        title: String,
        usage: String,
        pace: Option<(String, Color32)>,
        resets: Option<String>,
    },
}

/// A personal org is named "<email>'s Organization": it adds nothing and shows the email.
fn is_personal_org(name: &str) -> bool {
    name.ends_with("'s Organization") || name.ends_with("\u{2019}s Organization")
}

/// Tooltip title (spec section 8): the plan, or the org name when the plan is unknown.
pub fn tip_title(org: Option<&OrgSummary>, orgs: &[OrgSummary], lang: Lang) -> String {
    let Some(org) = org else {
        return String::new();
    };
    let l = t(lang);
    let Some(plan) = org.plan else {
        return org.name.clone();
    };
    let label = match plan {
        Plan::Max => l.plan_max,
        Plan::Ultra => l.plan_ultra,
        Plan::Pro => l.plan_pro,
        Plan::Team => l.plan_team,
        Plan::Enterprise => l.plan_enterprise,
    };
    let named = orgs.len() > 1 && !org.name.is_empty() && !is_personal_org(&org.name);
    if named {
        format!("{label} · {}", org.name)
    } else {
        label.to_string()
    }
}

/// Text of the ring tooltip, minus the org/updated header.
pub fn tip_model(r: &RingView, now: DateTime<Utc>, tz: FixedOffset, lang: Lang) -> TipModel {
    let l = t(lang);
    let title = if r.key == LimitKey::Session { l.s } else { l.w }.to_string();
    if r.no_limit {
        return TipModel::NoLimit {
            title,
            line: l.no_limit_tip.to_string(),
        };
    }
    if r.pace == Pace::Idle {
        return TipModel::Idle {
            title,
            line: l.no_session.to_string(),
            sub: l.starts_next.to_string(),
        };
    }
    let mut usage = (l.used)(ring_number(r.used));
    if let Some(exp) = r.expected {
        usage.push_str(" · ");
        usage.push_str(&(l.expected)(js_round(exp)));
    }
    let pace = r.expected.map(|exp| {
        let diff = js_round(r.used - exp);
        let text = match r.pace {
            Pace::Ahead => (l.ahead)(diff),
            Pace::Room => (l.room)(-diff),
            _ => l.on_track.to_string(),
        };
        (text, colors::pace_color(r.pace))
    });
    let resets = r.resets_at.map(|at| {
        let local = at.with_timezone(&tz);
        let when = if r.key == LimitKey::Session {
            fmt_time(&local)
        } else {
            fmt_day(&local, lang)
        };
        (l.resets_long)(&when, &fmt_dur((at - now).num_milliseconds(), lang))
    });
    TipModel::Active {
        title,
        usage,
        pace,
        resets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};
    use limits_core::{LimitKey, OrgSummary, Pace, Plan, RingView};

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
    fn active(m: TipModel) -> (String, String, Option<(String, Color32)>, Option<String>) {
        match m {
            TipModel::Active {
                title,
                usage,
                pace,
                resets,
            } => (title, usage, pace, resets),
            other => panic!("expected active, got {other:?}"),
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
    fn ago_has_a_1s_floor() {
        assert_eq!(
            ago_text(Some(now()), now(), Lang::En).as_deref(),
            Some("updated 1 s ago")
        );
        assert_eq!(
            ago_text(Some(now() - chrono::Duration::seconds(12)), now(), Lang::Fr).as_deref(),
            Some("mis à jour il y a 12 s")
        );
        assert_eq!(ago_text(None, now(), Lang::En), None);
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

    #[test]
    fn tip_session_ahead_of_pace() {
        let (title, usage, pace, resets) = active(tip_model(&ring(), now(), cet(), Lang::En));
        assert_eq!(title, "5-hour session");
        assert_eq!(usage, "62% used · expected 48%");
        assert_eq!(
            pace,
            Some(("14 pts ahead of pace".to_string(), colors::RED))
        );
        assert_eq!(resets.as_deref(), Some("Resets 14:00 (in 2h30)"));
    }
    #[test]
    fn tip_weekly_uses_day_form_and_room_phrase_fr_spacing() {
        let r = RingView {
            key: LimitKey::Weekly,
            used: 34.0,
            expected: Some(45.4),
            pace: Pace::Room,
            resets_at: Some(local(11, 6, 23, 59)),
            ..ring()
        };
        let (title, usage, pace, resets) = active(tip_model(&r, now(), cet(), Lang::Fr));
        assert_eq!(title, "Hebdomadaire");
        assert_eq!(usage, "34 % utilisé · attendu 45 %");
        assert_eq!(pace, Some(("11 pts de marge".to_string(), colors::BLUE)));
        assert_eq!(resets.as_deref(), Some("Reset ven. 23:59 (dans 3 j 12 h)"));
    }
    #[test]
    fn tip_room_rounds_negative_half_like_js() {
        let r = RingView {
            used: 42.5,
            expected: Some(45.0),
            pace: Pace::Room,
            ..ring()
        };
        let (_, _, pace, _) = active(tip_model(&r, now(), cet(), Lang::En));
        assert_eq!(pace.unwrap().0, "2 pts of room");
    }
    #[test]
    fn tip_on_track() {
        let r = RingView {
            used: 50.0,
            expected: Some(48.0),
            pace: Pace::OnTrack,
            ..ring()
        };
        let (_, _, pace, _) = active(tip_model(&r, now(), cet(), Lang::En));
        assert_eq!(pace, Some(("on track".to_string(), colors::GREEN)));
    }
    #[test]
    fn tip_expected_unknown_has_no_expected_part_and_no_pace() {
        let r = RingView {
            expected: None,
            pace: Pace::OnTrack,
            ..ring()
        };
        let (_, usage, pace, _) = active(tip_model(&r, now(), cet(), Lang::En));
        assert_eq!(usage, "62% used");
        assert_eq!(pace, None);
    }
    #[test]
    fn tip_idle_session_variant() {
        let r = RingView {
            used: 0.0,
            ..idle_ring()
        };
        assert_eq!(
            tip_model(&r, now(), cet(), Lang::En),
            TipModel::Idle {
                title: "5-hour session".into(),
                line: "no session running".into(),
                sub: "Starts with your next message".into(),
            }
        );
    }
    #[test]
    fn tip_reset_in_the_past_clamps_duration_at_0() {
        let r = RingView {
            resets_at: Some(now() - chrono::Duration::hours(1)),
            ..ring()
        };
        let (_, _, _, resets) = active(tip_model(&r, now(), cet(), Lang::En));
        assert!(resets.unwrap().ends_with("(in 0 s)"));
    }

    fn org(name: &str, plan: Option<Plan>) -> OrgSummary {
        OrgSummary {
            uuid: "x".into(),
            name: name.into(),
            plan,
        }
    }
    #[test]
    fn title_shows_the_plan_only_for_a_single_org() {
        let personal = org("Jane's Organization", Some(Plan::Max));
        let team = org("Acme Studio", Some(Plan::Team));
        assert_eq!(
            tip_title(Some(&personal), std::slice::from_ref(&personal), Lang::En),
            "Max plan"
        );
        assert_eq!(
            tip_title(Some(&team), std::slice::from_ref(&team), Lang::Fr),
            "Forfait Team"
        );
    }
    #[test]
    fn title_adds_the_org_name_when_there_are_several_orgs() {
        let personal = org("Jane's Organization", Some(Plan::Max));
        let team = org("Acme Studio", Some(Plan::Team));
        let both = [personal, team.clone()];
        assert_eq!(
            tip_title(Some(&team), &both, Lang::En),
            "Team plan · Acme Studio"
        );
        let ent = org("Acme Studio", Some(Plan::Enterprise));
        assert_eq!(
            tip_title(Some(&ent), &both, Lang::Fr),
            "Forfait Enterprise · Acme Studio"
        );
    }
    #[test]
    fn title_never_repeats_a_personal_org_name() {
        let personal = org("Jane's Organization", Some(Plan::Max));
        let curly = org("Jane\u{2019}s Organization", Some(Plan::Pro));
        let team = org("Acme Studio", Some(Plan::Team));
        let both = [personal.clone(), team];
        assert_eq!(tip_title(Some(&personal), &both, Lang::En), "Max plan");
        assert_eq!(tip_title(Some(&curly), &both, Lang::Fr), "Forfait Pro");
    }
    #[test]
    fn title_falls_back_to_the_org_name_when_the_plan_is_unknown() {
        let team = org("Acme Studio", None);
        let personal = org("Jane's Organization", None);
        assert_eq!(
            tip_title(Some(&team), std::slice::from_ref(&team), Lang::En),
            "Acme Studio"
        );
        let both = [personal.clone(), team];
        assert_eq!(
            tip_title(Some(&personal), &both, Lang::Fr),
            "Jane's Organization"
        );
        assert_eq!(tip_title(None, &[], Lang::En), "");
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
    #[test]
    fn no_limit_has_its_own_tooltip_not_the_idle_one() {
        let nl = no_limit_ring();
        assert_eq!(
            tip_model(&nl, now(), cet(), Lang::En),
            TipModel::NoLimit {
                title: "5-hour session".into(),
                line: "No 5-hour limit on the Ultra plan".into()
            }
        );
    }
    #[test]
    fn no_limit_tip_both_langs() {
        let nl = RingView {
            key: LimitKey::Session,
            used: 0.0,
            resets_at: None,
            expected: None,
            pace: Pace::Idle,
            no_limit: true,
        };
        assert_eq!(
            tip_model(&nl, now(), cet(), Lang::Fr),
            TipModel::NoLimit {
                title: "Session 5 h".into(),
                line: "Pas de limite de 5 h sur le forfait Ultra".into()
            }
        );
    }
    #[test]
    fn title_for_ultra() {
        let ultra = org("Jane's Organization", Some(Plan::Ultra));
        assert_eq!(
            tip_title(Some(&ultra), std::slice::from_ref(&ultra), Lang::En),
            "Ultra plan"
        );
        assert_eq!(
            tip_title(Some(&ultra), std::slice::from_ref(&ultra), Lang::Fr),
            "Forfait Ultra"
        );
    }
}
