pub mod api;
pub mod clock;
pub mod detect;
pub mod engine;
pub mod model;
pub mod org;
pub mod pace;
pub mod parse;
pub mod session;
pub mod store;
pub mod window_pos;

pub use model::{LimitKey, Org, Usage, Window};
pub use parse::{parse_orgs, parse_usage, ParseError};
pub use pace::{backoff_delay, pace, window_len, Pace, PaceInfo};
pub use detect::{detect_resets, ResetEvent, ResetKind};
pub use org::{choose_org, eligible_orgs};
pub use session::{SessionCookies, SessionSource};
pub use api::{ApiError, ClaudeApi, HttpApi};
pub use store::{CredStore, KeyringCreds, MemCreds, Settings, SettingsStore};
pub use window_pos::{clamp_position, expand_bounds, Bounds, PopupRoom, Rect};
pub use clock::{Clock, SystemClock};
#[cfg(any(test, feature = "test-util"))]
pub use clock::FakeClock;
pub use engine::{
    spawn_engine, AppState, BrowserLauncher, Command, Conn, EngineDeps, ErrorInfo, OrgSummary, RingView,
};
