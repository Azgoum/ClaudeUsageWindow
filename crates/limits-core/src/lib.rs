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

pub use model::{LimitKey, Org, Plan, Usage, Window};
pub use parse::{parse_orgs, parse_usage, ParseError};
pub use pace::{backoff_delay, pace, window_len, Pace, PaceInfo};
pub use detect::{detect_resets, detect_resets_for, ResetEvent, ResetKind};
pub use org::{choose_org, eligible_orgs};
pub use session::{SessionCookies, SessionSource};
pub use api::{ApiError, ClaudeApi, HttpApi};
pub use store::{Blobs, ChunkedCreds, CredStore, KeyringBlobs, KeyringCreds, MemCreds, Settings, SettingsStore};
pub use window_pos::{clamp_position, expand_bounds, persisted_position, Bounds, PopupRoom, Rect};
pub use clock::{Clock, SystemClock};
#[cfg(any(test, feature = "test-util"))]
pub use clock::FakeClock;
pub use engine::{
    spawn_engine, AppState, BrowserLauncher, Command, Conn, EngineDeps, ErrorInfo, OrgSummary, RingView,
};
