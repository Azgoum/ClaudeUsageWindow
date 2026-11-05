pub mod firefox;

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionCookies {
    pub cookies: Vec<(String, String)>,
}

impl SessionCookies {
    fn get(&self, name: &str) -> Option<&str> {
        self.cookies.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// `Cookie` header value: `a=1; b=2`.
    pub fn header(&self) -> String {
        self.cookies
            .iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn has_session(&self) -> bool {
        self.get("sessionKey").is_some_and(|v| !v.is_empty())
    }

    pub fn last_active_org(&self) -> Option<&str> {
        self.get("lastActiveOrg").filter(|v| !v.is_empty())
    }

    /// Hash of the `sessionKey` value only; with no sessionKey it hashes the empty string.
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.get("sessionKey").unwrap_or("").hash(&mut h);
        h.finish()
    }
}

impl fmt::Debug for SessionCookies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.cookies.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("SessionCookies").field("names", &names).finish()
    }
}

pub trait SessionSource: Send + Sync {
    fn read(&self) -> anyhow::Result<Option<SessionCookies>>;
}

/// Must equal the first org uuid in `tools/fake-claude/server.mjs`.
#[cfg(debug_assertions)]
pub const FAKE_ORG_UUID: &str = "0b5a1f6e-3c2d-4e8a-9a41-7d6c5b4a3f21";

/// Debug-only session source: yields a fake session a fixed delay after `arm()`.
#[cfg(debug_assertions)]
pub struct FixedSource {
    org_uuid: String,
    delay: std::time::Duration,
    armed_at: std::sync::Mutex<Option<std::time::Instant>>,
}

#[cfg(debug_assertions)]
impl FixedSource {
    pub fn new(org_uuid: &str) -> Self {
        Self::with_delay(org_uuid, std::time::Duration::from_secs(5))
    }

    pub fn with_delay(org_uuid: &str, delay: std::time::Duration) -> Self {
        Self {
            org_uuid: org_uuid.to_string(),
            delay,
            armed_at: std::sync::Mutex::new(None),
        }
    }

    pub fn arm(&self) {
        *self.armed_at.lock().unwrap() = Some(std::time::Instant::now());
    }
}

#[cfg(debug_assertions)]
impl SessionSource for FixedSource {
    fn read(&self) -> anyhow::Result<Option<SessionCookies>> {
        let armed = *self.armed_at.lock().unwrap();
        Ok(armed.filter(|t| t.elapsed() >= self.delay).map(|_| SessionCookies {
            cookies: vec![
                ("sessionKey".into(), "dev".into()),
                ("lastActiveOrg".into(), self.org_uuid.clone()),
            ],
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(pairs: &[(&str, &str)]) -> SessionCookies {
        SessionCookies {
            cookies: pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn header_joins_with_semicolon_space() {
        assert_eq!(c(&[("a", "1"), ("b", "2")]).header(), "a=1; b=2");
    }

    #[test]
    fn has_session_false_for_empty_or_missing() {
        assert!(!c(&[("sessionKey", "")]).has_session());
        assert!(!c(&[("other", "x")]).has_session());
        assert!(c(&[("sessionKey", "k")]).has_session());
    }

    #[test]
    fn debug_does_not_leak_values() {
        let s = format!("{:?}", c(&[("sessionKey", "SECRET123")]));
        assert!(!s.contains("SECRET123"));
        assert!(s.contains("sessionKey"));
    }

    #[test]
    fn last_active_org_and_fingerprint() {
        let a = c(&[("sessionKey", "k1"), ("lastActiveOrg", "o1")]);
        assert_eq!(a.last_active_org(), Some("o1"));
        let b = c(&[("sessionKey", "k1"), ("x", "y")]);
        let d = c(&[("sessionKey", "k2")]);
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_ne!(a.fingerprint(), d.fingerprint());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn fixed_source_yields_session_only_after_arm_and_delay() {
        use std::time::Duration;
        let src = FixedSource::with_delay(FAKE_ORG_UUID, Duration::from_millis(150));
        assert!(src.read().unwrap().is_none(), "never armed");
        src.arm();
        assert!(src.read().unwrap().is_none(), "before delay");
        std::thread::sleep(Duration::from_millis(200));
        let s = src.read().unwrap().expect("after delay");
        assert!(s.has_session());
        assert_eq!(s.last_active_org(), Some(FAKE_ORG_UUID));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn fixed_source_default_delay_is_five_seconds() {
        let src = FixedSource::new(FAKE_ORG_UUID);
        src.arm();
        assert!(src.read().unwrap().is_none());
    }
}
