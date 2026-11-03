use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitKey {
    Session,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Window {
    /// Percentage used, clamped to 0..=100.
    pub used: f64,
    /// `None` when the window is idle (no session running).
    pub resets_at: Option<DateTime<Utc>>,
}

impl Window {
    pub fn idle() -> Self {
        Self { used: 0.0, resets_at: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Usage {
    pub session: Window,
    pub weekly: Window,
}

impl Usage {
    pub fn get(&self, k: LimitKey) -> &Window {
        match k {
            LimitKey::Session => &self.session,
            LimitKey::Weekly => &self.weekly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Org {
    pub uuid: String,
    pub name: String,
    pub capabilities: Vec<String>,
}
