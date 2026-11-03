use crate::model::{Org, Usage, Window};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

fn parse_json(body: &str) -> Result<Value, ParseError> {
    serde_json::from_str(body).map_err(|e| ParseError::Json(e.to_string()))
}

fn clamp_pct(v: Option<f64>) -> f64 {
    match v {
        Some(x) if x.is_finite() => x.clamp(0.0, 100.0),
        _ => 0.0,
    }
}

fn parse_time(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn window_from(v: &Value, pct_key: &str) -> Window {
    Window {
        used: clamp_pct(v.get(pct_key).and_then(Value::as_f64)),
        resets_at: parse_time(v.get("resets_at")),
    }
}

/// A present key: null (or a non-object) means idle, an object is parsed leniently.
fn window_opt(v: &Value) -> Window {
    if v.is_object() {
        window_from(v, "utilization")
    } else {
        Window::idle()
    }
}

pub fn parse_usage(body: &str) -> Result<Usage, ParseError> {
    let root = parse_json(body)?;
    let obj = root
        .as_object()
        .ok_or_else(|| ParseError::Shape("usage body is not an object".into()))?;

    let five = obj.get("five_hour");
    let seven = obj.get("seven_day");
    if five.is_some() || seven.is_some() {
        return Ok(Usage {
            session: five.map(window_opt).unwrap_or_else(Window::idle),
            weekly: seven.map(window_opt).unwrap_or_else(Window::idle),
        });
    }

    // Fallback: structured `limits[]`, unscoped entries only.
    let limits = obj
        .get("limits")
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Shape("no five_hour, seven_day or limits".into()))?;
    let unscoped = |e: &&Value| e.get("scope").map_or(true, Value::is_null);
    let kind = |e: &Value| e.get("kind").and_then(Value::as_str).unwrap_or("").to_owned();

    let session = limits.iter().filter(unscoped).find(|e| {
        let k = kind(e);
        e.get("group").and_then(Value::as_str) == Some("session")
            || k.starts_with("session")
            || k.starts_with("five_hour")
    });
    let weekly = limits.iter().filter(unscoped).find(|e| kind(e) == "weekly_all");

    if session.is_none() && weekly.is_none() {
        return Err(ParseError::Shape("limits[] has no session or weekly entry".into()));
    }
    Ok(Usage {
        session: session.map(|e| window_from(e, "percent")).unwrap_or_else(Window::idle),
        weekly: weekly.map(|e| window_from(e, "percent")).unwrap_or_else(Window::idle),
    })
}

pub fn parse_orgs(body: &str) -> Result<Vec<Org>, ParseError> {
    let root = parse_json(body)?;
    let arr = root
        .as_array()
        .ok_or_else(|| ParseError::Shape("organizations body is not an array".into()))?;
    Ok(arr
        .iter()
        .filter_map(|o| {
            let uuid = o.get("uuid")?.as_str()?.to_owned();
            let name = o.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
            let capabilities = o
                .get("capabilities")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            Some(Org { uuid, name, capabilities })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../tests/fixtures/usage_real_2026-11-02.json");
    const BOTH_ACTIVE: &str = include_str!("../tests/fixtures/usage_both_active.json");
    const NULL_SESSION: &str = include_str!("../tests/fixtures/usage_null_session.json");
    const NULL_BOTH: &str = include_str!("../tests/fixtures/usage_null_both.json");
    const LIMITS_ONLY: &str = include_str!("../tests/fixtures/usage_limits_only.json");
    const UNKNOWN: &str = include_str!("../tests/fixtures/usage_unknown.json");
    const ORGS_TWO: &str = include_str!("../tests/fixtures/orgs_two.json");

    #[test]
    fn real_response_null_session_is_idle() {
        let u = parse_usage(REAL).unwrap();
        assert_eq!(u.session.used, 0.0);
        assert!(u.session.resets_at.is_none());
        assert_eq!(u.weekly.used, 1.0);
        assert_eq!(u.weekly.resets_at.unwrap().to_rfc3339(), "2026-11-06T23:59:59.657790+00:00");
    }
    #[test]
    fn both_null_is_idle_not_error() {
        let u = parse_usage(NULL_BOTH).unwrap();
        assert!(u.session.resets_at.is_none() && u.weekly.resets_at.is_none());
    }
    #[test]
    fn limits_only_fallback_ignores_scoped() {
        let u = parse_usage(LIMITS_ONLY).unwrap();
        assert_eq!(u.session.used, 40.0);
        assert_eq!(u.weekly.used, 12.0);
    }
    #[test]
    fn unknown_shape_is_shape_error() {
        assert!(matches!(parse_usage(UNKNOWN), Err(ParseError::Shape(_))));
    }
    #[test]
    fn not_json_is_json_error() {
        assert!(matches!(parse_usage("<html>"), Err(ParseError::Json(_))));
    }
    #[test]
    fn clamps_and_parses_odd_values() {
        let u = parse_usage(r#"{"five_hour":{"utilization":104,"resets_at":"2026-11-03T13:00:00Z"},"seven_day":{"utilization":-3.5,"resets_at":"2026-11-06T23:59:59.000001+00:00"}}"#).unwrap();
        assert_eq!(u.session.used, 100.0);
        assert_eq!(u.weekly.used, 0.0);
    }
    #[test]
    fn orgs_parse() {
        let o = parse_orgs(ORGS_TWO).unwrap();
        assert_eq!(o.len(), 2);
        assert!(o[0].capabilities.contains(&"chat".into()));
    }
    #[test]
    fn both_active_parses_session() {
        let u = parse_usage(BOTH_ACTIVE).unwrap();
        assert_eq!(u.session.used, 62.0);
        assert_eq!(u.session.resets_at.unwrap().to_rfc3339(), "2026-11-03T13:00:00+00:00");
    }
    #[test]
    fn null_session_fixture_is_idle_session() {
        let u = parse_usage(NULL_SESSION).unwrap();
        assert!(u.session.resets_at.is_none());
        assert!(u.weekly.resets_at.is_some());
    }
}
