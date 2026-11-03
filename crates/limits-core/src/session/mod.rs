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
}
