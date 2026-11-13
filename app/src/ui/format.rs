//! Time and duration formatting (port of Phase 1 (git bcaa1bf) `ui/src/lib/format.ts`).

use chrono::{DateTime, Datelike, FixedOffset, TimeZone, Timelike, Utc};
use limits_core::LimitKey;

use super::Lang;

const MIN: i64 = 60_000;
const H: i64 = 60 * MIN;
const D: i64 = 24 * H;

// Deterministic tables (no locale lookup) so output is stable across machines and tests.
const DAYS_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAYS_FR: [&str; 7] = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."];

/// Time of day in the date's own zone, 24 h: "14:00".
pub fn fmt_time<Tz: TimeZone>(d: &DateTime<Tz>) -> String {
    format!("{:02}:{:02}", d.hour(), d.minute())
}

/// "Fri 23:59" / "ven. 23:59".
pub fn fmt_day<Tz: TimeZone>(d: &DateTime<Tz>, lang: Lang) -> String {
    let days = match lang {
        Lang::En => &DAYS_EN,
        Lang::Fr => &DAYS_FR,
    };
    format!(
        "{} {}",
        days[d.weekday().num_days_from_sunday() as usize],
        fmt_time(d)
    )
}

/// "45 s" / "12 min" / "2h29" / "3d 12h" / "3 j 12 h".
pub fn fmt_dur(ms: i64, lang: Lang) -> String {
    let ms = ms.max(0);
    if ms < MIN {
        return format!("{} s", (ms + 999) / 1000);
    }
    if ms >= D {
        let d = ms / D;
        let h = (ms % D) / H;
        return match lang {
            Lang::Fr => format!("{d} j {h} h"),
            Lang::En => format!("{d}d {h}h"),
        };
    }
    // Rounds first so 1h59m40s never prints "1h60".
    let total_min = (ms + MIN / 2) / MIN;
    let (h, m) = (total_min / 60, total_min % 60);
    if h > 0 {
        format!("{h}h{m:02}")
    } else {
        format!("{m} min")
    }
}

/// Short reset line under a ring. Day form only for weekly limits more than 24 h away.
pub fn reset_line(
    key: LimitKey,
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: FixedOffset,
    lang: Lang,
) -> String {
    let fr = lang == Lang::Fr;
    let Some(at) = resets_at else {
        return if fr { "inactive" } else { "idle" }.to_string();
    };
    let local = at.with_timezone(&tz);
    if key == LimitKey::Weekly && (at - now).num_milliseconds() > D {
        return format!(
            "{} {}",
            if fr { "reset" } else { "resets" },
            fmt_day(&local, lang)
        );
    }
    format!(
        "{} {}",
        if fr { "reset à" } else { "resets" },
        fmt_time(&local)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cet() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }
    fn local(m: u32, d: u32, h: u32, min: u32) -> DateTime<FixedOffset> {
        cet().with_ymd_and_hms(2026, m, d, h, min, 0).unwrap()
    }
    fn utc(d: DateTime<FixedOffset>) -> DateTime<Utc> {
        d.with_timezone(&Utc)
    }

    #[test]
    fn dur_seconds() {
        assert_eq!(fmt_dur(45_000, Lang::En), "45 s");
    }
    #[test]
    fn dur_minutes() {
        assert_eq!(fmt_dur(12 * 60_000, Lang::En), "12 min");
    }
    #[test]
    fn dur_hours_with_padded_minutes() {
        assert_eq!(fmt_dur((2 * 60 + 29) * 60_000, Lang::En), "2h29");
        assert_eq!(fmt_dur((60 + 5) * 60_000, Lang::Fr), "1h05");
    }
    #[test]
    fn dur_days() {
        assert_eq!(fmt_dur((3 * 24 + 12) * 3_600_000, Lang::En), "3d 12h");
        assert_eq!(fmt_dur((3 * 24 + 12) * 3_600_000, Lang::Fr), "3 j 12 h");
    }
    #[test]
    fn fmt_dur_rounds_before_splitting() {
        assert_eq!(fmt_dur((119 * 60 + 40) * 1000, Lang::En), "2h00");
    }
    #[test]
    fn dur_negative_clamps_to_zero() {
        assert_eq!(fmt_dur(-5, Lang::En), "0 s");
    }

    #[test]
    fn time_24h() {
        assert_eq!(fmt_time(&local(11, 3, 14, 0)), "14:00");
    }
    #[test]
    fn day_names() {
        let fri = local(11, 6, 23, 59);
        assert_eq!(fmt_day(&fri, Lang::En), "Fri 23:59");
        assert_eq!(fmt_day(&fri, Lang::Fr), "ven. 23:59");
    }

    #[test]
    fn weekly_far_away_uses_day_form() {
        let (fri, tue) = (utc(local(11, 6, 23, 59)), utc(local(11, 3, 11, 30)));
        assert_eq!(
            reset_line(LimitKey::Weekly, Some(fri), tue, cet(), Lang::Fr),
            "reset ven. 23:59"
        );
        assert_eq!(
            reset_line(LimitKey::Weekly, Some(fri), tue, cet(), Lang::En),
            "resets Fri 23:59"
        );
    }
    #[test]
    fn session_without_reset_is_idle() {
        let tue = utc(local(11, 3, 11, 30));
        assert_eq!(
            reset_line(LimitKey::Session, None, tue, cet(), Lang::En),
            "idle"
        );
        assert_eq!(
            reset_line(LimitKey::Session, None, tue, cet(), Lang::Fr),
            "inactive"
        );
    }
    #[test]
    fn weekly_within_24h_uses_time_only_form() {
        let tue = utc(local(11, 3, 11, 30));
        let in20h = tue + chrono::Duration::hours(20);
        assert_eq!(
            reset_line(LimitKey::Weekly, Some(in20h), tue, cet(), Lang::En),
            format!("resets {}", fmt_time(&in20h.with_timezone(&cet())))
        );
    }
    #[test]
    fn session_uses_time_only_form() {
        let (at, tue) = (utc(local(11, 3, 14, 0)), utc(local(11, 3, 11, 30)));
        assert_eq!(
            reset_line(LimitKey::Session, Some(at), tue, cet(), Lang::Fr),
            "reset à 14:00"
        );
        assert_eq!(
            reset_line(LimitKey::Session, Some(at), tue, cet(), Lang::En),
            "resets 14:00"
        );
    }
}
