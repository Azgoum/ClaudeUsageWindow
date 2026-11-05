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

/// claude.ai plan of an org, from its `rate_limit_tier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    /// Has the 5-hour session limit (formerly "Max 5x").
    Max,
    /// Weekly limit only. claude.ai still reports it as `default_claude_max_20x`.
    Ultra,
    Pro,
    Team,
    Enterprise,
}

impl Plan {
    /// Maps a raw `rate_limit_tier` (e.g. `default_claude_max_20x`); unknown tiers give `None`.
    pub fn from_tier(tier: &str) -> Option<Plan> {
        let t = tier.to_ascii_lowercase();
        if t.contains("max_20x") || t.contains("ultra") {
            Some(Plan::Ultra)
        } else if t.contains("max") {
            Some(Plan::Max)
        } else if t.contains("enterprise") {
            Some(Plan::Enterprise)
        } else if t.contains("team") || t == "raven" {
            Some(Plan::Team)
        } else if t.contains("pro") {
            Some(Plan::Pro)
        } else {
            None
        }
    }

    /// `false` only for Ultra, which has a weekly limit and no 5-hour one.
    pub fn has_session_limit(self) -> bool {
        self != Plan::Ultra
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Org {
    pub uuid: String,
    pub name: String,
    pub capabilities: Vec<String>,
    /// `None` when the tier is missing or not recognised.
    pub plan: Option<Plan>,
}
