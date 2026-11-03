use crate::clock::Clock;
use crate::{
    backoff_delay, choose_org, detect_resets, eligible_orgs, pace, ApiError, ClaudeApi, CredStore,
    LimitKey, Org, Pace, ResetEvent, SessionCookies, SessionSource, SettingsStore, Usage,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep_until, Instant};

pub const POLL_INTERVAL: Duration = Duration::from_secs(30);
pub const CONNECT_TICK: Duration = Duration::from_secs(2);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub trait BrowserLauncher: Send + Sync {
    fn firefox_installed(&self) -> bool;
    fn open_login(&self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Connect,
    CancelConnect,
    Refresh,
    SelectOrg(String),
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Conn {
    Disconnected { timed_out: bool },
    NeedsFirefox,
    Waiting,
    Connected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingView {
    pub key: LimitKey,
    pub used: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub expected: Option<f64>,
    pub pace: Pace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgSummary {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub retry_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub conn: Conn,
    pub org: Option<OrgSummary>,
    /// Eligible (chat) orgs only.
    pub orgs: Vec<OrgSummary>,
    /// `[session, weekly]`.
    pub rings: Option<[RingView; 2]>,
    pub last_ok: Option<DateTime<Utc>>,
    pub error: Option<ErrorInfo>,
    /// Events of the latest detection only; the UI animates when `reset_seq` changes.
    pub resets: Vec<ResetEvent>,
    pub reset_seq: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            conn: Conn::Disconnected { timed_out: false },
            org: None,
            orgs: Vec::new(),
            rings: None,
            last_ok: None,
            error: None,
            resets: Vec::new(),
            reset_seq: 0,
        }
    }
}

pub struct EngineDeps {
    pub api: Arc<dyn ClaudeApi>,
    pub source: Arc<dyn SessionSource>,
    pub creds: Arc<dyn CredStore>,
    pub settings: SettingsStore,
    pub clock: Arc<dyn Clock>,
    pub browser: Arc<dyn BrowserLauncher>,
}

pub fn spawn_engine(deps: EngineDeps) -> (mpsc::Sender<Command>, watch::Receiver<AppState>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (state_tx, state_rx) = watch::channel(AppState::default());
    tokio::spawn(Engine::new(deps, state_tx).run(cmd_rx));
    (cmd_tx, state_rx)
}

type PollFuture = Pin<Box<dyn Future<Output = Result<Usage, ApiError>> + Send>>;

enum Event {
    Cmd(Option<Command>),
    Polled(Result<Usage, ApiError>),
    PollDue,
    ConnectTick,
}

struct Engine {
    deps: EngineDeps,
    tx: watch::Sender<AppState>,
    state: AppState,
    cookies: Option<SessionCookies>,
    org_uuid: Option<String>,
    /// Previous successful usage for the current org in this connection (reset detection).
    prev: Option<Usage>,
    /// Latest successful usage, kept to recompute pace on failed ticks.
    last_usage: Option<Usage>,
    failures: u32,
    inflight: Option<PollFuture>,
    next_poll: Option<Instant>,
    next_tick: Option<Instant>,
    connect_deadline: Option<Instant>,
}

async fn sleep_opt(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

async fn poll_opt(f: &mut Option<PollFuture>) -> Result<Usage, ApiError> {
    match f.as_mut() {
        Some(f) => f.await,
        None => std::future::pending().await,
    }
}

fn summaries(orgs: &[Org]) -> Vec<OrgSummary> {
    eligible_orgs(orgs)
        .into_iter()
        .map(|o| OrgSummary { uuid: o.uuid.clone(), name: o.name.clone() })
        .collect()
}

fn ring(key: LimitKey, u: &Usage, now: DateTime<Utc>) -> RingView {
    let w = u.get(key);
    let p = pace(key, w, now);
    RingView { key, used: w.used, resets_at: w.resets_at, expected: p.expected, pace: p.pace }
}

fn describe(e: &ApiError) -> String {
    match e {
        ApiError::Expired => "session expired".into(),
        ApiError::Unavailable(m) => format!("unavailable: {m}"),
        ApiError::Unexpected { status, .. } => format!("unexpected response (HTTP {status})"),
    }
}

impl Engine {
    fn new(deps: EngineDeps, tx: watch::Sender<AppState>) -> Self {
        let org_uuid = deps.settings.load().org_uuid;
        Self {
            deps,
            tx,
            state: AppState::default(),
            cookies: None,
            org_uuid,
            prev: None,
            last_usage: None,
            failures: 0,
            inflight: None,
            next_poll: None,
            next_tick: None,
            connect_deadline: None,
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        self.start().await;
        self.publish();
        loop {
            let mut inflight = self.inflight.take();
            let event = tokio::select! {
                cmd = rx.recv() => Event::Cmd(cmd),
                res = poll_opt(&mut inflight) => Event::Polled(res),
                _ = sleep_opt(self.next_poll) => Event::PollDue,
                _ = sleep_opt(self.next_tick) => Event::ConnectTick,
            };
            if !matches!(event, Event::Polled(_)) {
                self.inflight = inflight;
            }
            match event {
                Event::Cmd(None) => break,
                Event::Cmd(Some(cmd)) => self.handle(cmd).await,
                Event::Polled(res) => self.on_poll_result(res).await,
                Event::PollDue => {
                    self.next_poll = None;
                    self.start_poll().await;
                }
                Event::ConnectTick => {
                    self.next_tick = None;
                    self.connect_tick().await;
                }
            }
            self.publish();
        }
        tracing::info!("engine stopped");
    }

    fn publish(&self) {
        self.tx.send_if_modified(|s| {
            if *s == self.state {
                false
            } else {
                *s = self.state.clone();
                true
            }
        });
    }

    fn set_conn(&mut self, conn: Conn) {
        if self.state.conn != conn {
            tracing::info!(from = ?self.state.conn, to = ?conn, "engine state transition");
            self.state.conn = conn;
        }
    }

    async fn start(&mut self) {
        match self.deps.creds.load() {
            Some(c) => {
                self.cookies = Some(c);
                self.set_conn(Conn::Connected);
                self.refresh_orgs().await;
                self.start_poll().await;
            }
            None => self.set_conn(Conn::Disconnected { timed_out: false }),
        }
    }

    fn save_org(&mut self, uuid: Option<String>) {
        let mut s = self.deps.settings.load();
        if s.org_uuid != uuid {
            s.org_uuid = uuid.clone();
            if let Err(e) = self.deps.settings.save(&s) {
                tracing::warn!("cannot save settings: {e}");
            }
        }
        self.org_uuid = uuid;
    }

    /// Applies an org list: eligible summaries, selected org (saved -> lastActiveOrg -> first).
    fn apply_orgs(&mut self, orgs: &[Org]) -> bool {
        self.state.orgs = summaries(orgs);
        let last_active = self.cookies.as_ref().and_then(|c| c.last_active_org());
        let chosen = choose_org(orgs, self.org_uuid.as_deref(), last_active)
            .map(|o| OrgSummary { uuid: o.uuid.clone(), name: o.name.clone() });
        match chosen {
            Some(o) => {
                self.save_org(Some(o.uuid.clone()));
                self.state.org = Some(o);
                true
            }
            None => false,
        }
    }

    /// Fetches the org list with the current cookies (start, or when no org is known yet).
    async fn refresh_orgs(&mut self) {
        let Some(c) = self.cookies.clone() else { return };
        match self.deps.api.orgs(&c).await {
            Ok(orgs) => {
                if !self.apply_orgs(&orgs) {
                    tracing::warn!("no eligible organization");
                }
            }
            Err(e) => tracing::warn!("cannot list organizations: {}", describe(&e)),
        }
    }

    async fn start_poll(&mut self) {
        if self.inflight.is_some() {
            return;
        }
        self.next_poll = None;
        let Some(c) = self.cookies.clone() else { return };
        if self.org_uuid.is_none() {
            self.refresh_orgs().await;
        }
        let Some(org) = self.org_uuid.clone() else {
            self.on_failure(ApiError::Unavailable("no organization selected".into()));
            return;
        };
        let api = self.deps.api.clone();
        self.inflight = Some(Box::pin(async move { api.usage(&c, &org).await }));
    }

    fn read_source(&self) -> Option<SessionCookies> {
        match self.deps.source.read() {
            Ok(c) => c.filter(|c| c.has_session()),
            Err(e) => {
                tracing::warn!("session source read failed: {e}");
                None
            }
        }
    }

    fn refresh_rings(&mut self) {
        let now = self.deps.clock.now();
        self.state.rings = self
            .last_usage
            .as_ref()
            .map(|u| [ring(LimitKey::Session, u, now), ring(LimitKey::Weekly, u, now)]);
    }

    async fn on_poll_result(&mut self, res: Result<Usage, ApiError>) {
        match res {
            Ok(cur) => {
                let now = self.deps.clock.now();
                if let Some(prev) = &self.prev {
                    let events = detect_resets(prev, &cur, now);
                    if !events.is_empty() {
                        tracing::info!(count = events.len(), "reset detected");
                        self.state.resets = events;
                        self.state.reset_seq += 1;
                    }
                }
                self.prev = Some(cur.clone());
                self.last_usage = Some(cur);
                self.failures = 0;
                self.refresh_rings();
                self.state.last_ok = Some(now);
                self.state.error = None;
                self.next_poll = Some(Instant::now() + POLL_INTERVAL);
            }
            Err(ApiError::Expired) => {
                let current = self.cookies.as_ref().map(|c| c.fingerprint());
                match self.read_source().filter(|c| Some(c.fingerprint()) != current) {
                    Some(fresh) => {
                        tracing::info!("session expired but the browser has a newer cookie; retrying");
                        if let Err(e) = self.deps.creds.save(&fresh) {
                            tracing::warn!("cannot save credentials: {e}");
                        }
                        self.cookies = Some(fresh);
                        self.start_poll().await;
                    }
                    None => {
                        self.prev = None;
                        self.next_poll = None;
                        self.state.error = None;
                        self.set_conn(Conn::Expired);
                    }
                }
            }
            Err(e) => self.on_failure(e),
        }
    }

    fn on_failure(&mut self, e: ApiError) {
        self.failures += 1;
        let delay = backoff_delay(self.failures);
        tracing::warn!(failures = self.failures, retry_in_secs = delay.as_secs(), "poll failed: {}", describe(&e));
        let retry_at = self.deps.clock.now() + chrono::Duration::from_std(delay).unwrap_or_default();
        self.state.error = Some(ErrorInfo { retry_at });
        self.refresh_rings();
        self.next_poll = Some(Instant::now() + delay);
    }

    fn stop_activity(&mut self) {
        self.inflight = None;
        self.next_poll = None;
        self.next_tick = None;
        self.connect_deadline = None;
    }

    async fn handle(&mut self, cmd: Command) {
        tracing::info!(command = ?cmd, "command");
        match cmd {
            Command::Connect => {
                self.stop_activity();
                self.prev = None;
                self.failures = 0;
                self.state.error = None;
                if !self.deps.browser.firefox_installed() {
                    self.set_conn(Conn::NeedsFirefox);
                    return;
                }
                if let Err(e) = self.deps.browser.open_login() {
                    tracing::warn!("cannot open the login page: {e}");
                }
                self.set_conn(Conn::Waiting);
                let now = Instant::now();
                self.next_tick = Some(now);
                self.connect_deadline = Some(now + CONNECT_TIMEOUT);
            }
            Command::CancelConnect => {
                if self.state.conn == Conn::Waiting {
                    self.stop_activity();
                    self.set_conn(Conn::Disconnected { timed_out: false });
                }
            }
            Command::Refresh => {
                if self.state.conn == Conn::Connected && self.inflight.is_none() {
                    self.start_poll().await;
                }
            }
            Command::SelectOrg(uuid) => {
                let Some(o) = self.state.orgs.iter().find(|o| o.uuid == uuid).cloned() else {
                    tracing::warn!("ignoring selection of an unknown organization");
                    return;
                };
                self.save_org(Some(uuid));
                self.state.org = Some(o);
                self.prev = None;
                if self.state.conn == Conn::Connected {
                    self.inflight = None;
                    self.start_poll().await;
                }
            }
            Command::Disconnect => {
                self.stop_activity();
                if let Err(e) = self.deps.creds.clear() {
                    tracing::warn!("cannot clear credentials: {e}");
                }
                self.save_org(None);
                self.cookies = None;
                self.prev = None;
                self.last_usage = None;
                self.failures = 0;
                self.state = AppState { reset_seq: self.state.reset_seq, ..AppState::default() };
                tracing::info!("disconnected by user");
            }
        }
    }

    /// One Connect read: every `CONNECT_TICK` until a session shows up or the timeout.
    async fn connect_tick(&mut self) {
        let now = Instant::now();
        if self.connect_deadline.is_some_and(|d| now >= d) {
            self.stop_activity();
            self.set_conn(Conn::Disconnected { timed_out: true });
            return;
        }
        self.next_tick = Some(now + CONNECT_TICK);
        let Some(c) = self.read_source() else { return };
        let orgs = match self.deps.api.orgs(&c).await {
            Ok(orgs) => orgs,
            Err(e) => {
                tracing::warn!("cannot list organizations yet: {}", describe(&e));
                return;
            }
        };
        let saved = self.org_uuid.clone();
        if choose_org(&orgs, saved.as_deref(), c.last_active_org()).is_none() {
            tracing::warn!("no eligible organization yet; still waiting");
            return;
        }
        if let Err(e) = self.deps.creds.save(&c) {
            tracing::warn!("cannot save credentials: {e}");
        }
        self.cookies = Some(c);
        self.apply_orgs(&orgs);
        self.next_tick = None;
        self.connect_deadline = None;
        self.set_conn(Conn::Connected);
        self.start_poll().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::{MemCreds, Org, ResetKind, SessionCookies, Usage, Window};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    // ---------- fakes ----------

    struct FakeApi {
        usage: Mutex<VecDeque<Result<Usage, ApiError>>>,
        orgs: Mutex<Result<Vec<Org>, ApiError>>,
        usage_calls: AtomicUsize,
        usage_orgs: Mutex<Vec<String>>,
        delay: Duration,
    }

    #[async_trait]
    impl ClaudeApi for FakeApi {
        async fn orgs(&self, _c: &SessionCookies) -> Result<Vec<Org>, ApiError> {
            self.orgs.lock().unwrap().clone()
        }
        async fn usage(&self, _c: &SessionCookies, org_uuid: &str) -> Result<Usage, ApiError> {
            self.usage_calls.fetch_add(1, Ordering::SeqCst);
            self.usage_orgs.lock().unwrap().push(org_uuid.to_string());
            let next = self
                .usage
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(usage(10.0, Some("2026-11-03T20:00:00Z"), 5.0)));
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            next
        }
    }

    #[derive(Default)]
    struct FakeSource(Mutex<Option<SessionCookies>>);
    impl FakeSource {
        fn set(&self, c: Option<SessionCookies>) {
            *self.0.lock().unwrap() = c;
        }
    }
    impl SessionSource for FakeSource {
        fn read(&self) -> anyhow::Result<Option<SessionCookies>> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct FakeBrowser {
        installed: bool,
        open_calls: AtomicUsize,
    }
    impl BrowserLauncher for FakeBrowser {
        fn firefox_installed(&self) -> bool {
            self.installed
        }
        fn open_login(&self) -> anyhow::Result<()> {
            self.open_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    // ---------- fixtures ----------

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn usage(session_used: f64, session_resets: Option<&str>, weekly_used: f64) -> Usage {
        Usage {
            session: Window { used: session_used, resets_at: session_resets.map(t) },
            weekly: Window { used: weekly_used, resets_at: Some(t("2026-11-09T00:00:00Z")) },
        }
    }

    fn cookies(session_key: &str, last_active: Option<&str>) -> SessionCookies {
        let mut c = vec![("sessionKey".to_string(), session_key.to_string())];
        if let Some(o) = last_active {
            c.push(("lastActiveOrg".to_string(), o.to_string()));
        }
        SessionCookies { cookies: c }
    }

    fn org(uuid: &str, caps: &[&str]) -> Org {
        Org { uuid: uuid.into(), name: uuid.into(), capabilities: caps.iter().map(|s| s.to_string()).collect() }
    }

    fn summary(uuid: &str) -> OrgSummary {
        OrgSummary { uuid: uuid.into(), name: uuid.into() }
    }

    struct Setup {
        start: &'static str,
        creds: Option<SessionCookies>,
        saved_org: Option<&'static str>,
        installed: bool,
        usage: Vec<Result<Usage, ApiError>>,
        source: Option<SessionCookies>,
        delay: Duration,
    }

    impl Default for Setup {
        fn default() -> Self {
            Self {
                start: "2026-11-03T12:00:00Z",
                creds: None,
                saved_org: None,
                installed: true,
                usage: Vec::new(),
                source: None,
                delay: Duration::ZERO,
            }
        }
    }

    struct H {
        tx: mpsc::Sender<Command>,
        rx: watch::Receiver<AppState>,
        api: Arc<FakeApi>,
        source: Arc<FakeSource>,
        creds: Arc<MemCreds>,
        clock: Arc<FakeClock>,
        browser: Arc<FakeBrowser>,
        settings: SettingsStore,
        _dir: tempfile::TempDir,
    }

    impl H {
        fn state(&self) -> AppState {
            self.rx.borrow().clone()
        }
        fn calls(&self) -> usize {
            self.api.usage_calls.load(Ordering::SeqCst)
        }
        fn push_usage(&self, r: Result<Usage, ApiError>) {
            self.api.usage.lock().unwrap().push_back(r);
        }
        async fn send(&mut self, c: Command) {
            self.tx.send(c).await.unwrap();
            settle(&mut self.rx).await;
        }
        /// Advance both clocks one second at a time.
        async fn step(&mut self, secs: u64) {
            for _ in 0..secs {
                self.jump(Duration::from_secs(1)).await;
            }
        }
        /// Advance both clocks at once (e.g. the PC sleeping).
        async fn jump(&mut self, d: Duration) {
            self.clock.advance(chrono::Duration::from_std(d).unwrap());
            tokio::time::advance(d).await;
            settle(&mut self.rx).await;
        }
    }

    /// Yields until the watch channel has been quiet for a while.
    async fn settle(rx: &mut watch::Receiver<AppState>) {
        let mut quiet = 0;
        while quiet < 20 {
            tokio::task::yield_now().await;
            if rx.has_changed().unwrap_or(false) {
                rx.borrow_and_update();
                quiet = 0;
            } else {
                quiet += 1;
            }
        }
    }

    async fn start(s: Setup) -> H {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = SettingsStore::new(path.clone());
        if let Some(o) = s.saved_org {
            settings.save(&crate::Settings { org_uuid: Some(o.into()), ..Default::default() }).unwrap();
        }
        let api = Arc::new(FakeApi {
            usage: Mutex::new(s.usage.into_iter().collect()),
            orgs: Mutex::new(Ok(vec![org("org-a", &["chat"]), org("org-b", &["chat"]), org("org-api", &["api"])])),
            usage_calls: AtomicUsize::new(0),
            usage_orgs: Mutex::new(Vec::new()),
            delay: s.delay,
        });
        let source = Arc::new(FakeSource::default());
        source.set(s.source);
        let creds = Arc::new(MemCreds::default());
        if let Some(c) = &s.creds {
            creds.save(c).unwrap();
        }
        let clock = Arc::new(FakeClock::new(t(s.start)));
        let browser = Arc::new(FakeBrowser { installed: s.installed, open_calls: AtomicUsize::new(0) });
        let (tx, mut rx) = spawn_engine(EngineDeps {
            api: api.clone(),
            source: source.clone(),
            creds: creds.clone(),
            settings: SettingsStore::new(path.clone()),
            clock: clock.clone(),
            browser: browser.clone(),
        });
        settle(&mut rx).await;
        H { tx, rx, api, source, creds, clock, browser, settings, _dir: dir }
    }

    fn with_creds() -> Setup {
        Setup { creds: Some(cookies("sk-old", Some("org-b"))), saved_org: Some("org-a"), ..Default::default() }
    }

    const DISCONNECTED: Conn = Conn::Disconnected { timed_out: false };

    // ---------- tests ----------

    #[tokio::test(start_paused = true)]
    async fn starts_disconnected_without_creds() {
        let h = start(Setup::default()).await;
        assert_eq!(h.state().conn, DISCONNECTED);
        assert_eq!(h.calls(), 0);
        assert!(h.state().rings.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn starts_connected_and_polls_with_creds() {
        let h = start(with_creds()).await;
        let s = h.state();
        assert_eq!(s.conn, Conn::Connected);
        assert_eq!(h.calls(), 1);
        assert_eq!(*h.api.usage_orgs.lock().unwrap(), vec!["org-a"]);
        assert_eq!(s.org, Some(summary("org-a")));
        assert_eq!(s.orgs, vec![summary("org-a"), summary("org-b")]);
        let rings = s.rings.expect("rings");
        assert_eq!(rings[0].key, LimitKey::Session);
        assert_eq!(rings[1].key, LimitKey::Weekly);
        assert_eq!(s.last_ok, Some(t("2026-11-03T12:00:00Z")));
        assert!(s.error.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn start_without_saved_org_picks_last_active_and_saves_it() {
        let h = start(Setup { saved_org: None, ..with_creds() }).await;
        assert_eq!(h.state().org, Some(summary("org-b")));
        assert_eq!(h.settings.load().org_uuid.as_deref(), Some("org-b"));
        assert_eq!(*h.api.usage_orgs.lock().unwrap(), vec!["org-b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_without_firefox_needs_firefox() {
        let mut h = start(Setup { installed: false, ..Default::default() }).await;
        h.send(Command::Connect).await;
        assert_eq!(h.state().conn, Conn::NeedsFirefox);
        assert_eq!(h.browser.open_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_flow_waits_then_connects_when_cookie_appears() {
        let mut h = start(Setup::default()).await;
        h.send(Command::Connect).await;
        assert_eq!(h.state().conn, Conn::Waiting);
        assert_eq!(h.browser.open_calls.load(Ordering::SeqCst), 1);
        h.step(2).await;
        assert_eq!(h.state().conn, Conn::Waiting);
        h.step(3).await; // t = 5 s
        let fresh = cookies("sk-new", Some("org-b"));
        h.source.set(Some(fresh.clone()));
        h.step(1).await; // t = 6 s: tick reads the cookie
        let s = h.state();
        assert_eq!(s.conn, Conn::Connected);
        assert_eq!(h.creds.load(), Some(fresh));
        assert_eq!(h.settings.load().org_uuid.as_deref(), Some("org-b"));
        assert_eq!(s.org, Some(summary("org-b")));
        assert_eq!(s.orgs, vec![summary("org-a"), summary("org-b")]);
        assert_eq!(*h.api.usage_orgs.lock().unwrap(), vec!["org-b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_times_out_after_10_min() {
        let mut h = start(Setup::default()).await;
        h.send(Command::Connect).await;
        h.step(599).await;
        assert_eq!(h.state().conn, Conn::Waiting);
        h.step(2).await;
        assert_eq!(h.state().conn, Conn::Disconnected { timed_out: true });
        // no more reads scheduled: a cookie appearing later changes nothing
        h.source.set(Some(cookies("sk-late", None)));
        h.step(10).await;
        assert_eq!(h.state().conn, Conn::Disconnected { timed_out: true });
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_connect_returns_to_disconnected() {
        let mut h = start(Setup::default()).await;
        h.send(Command::Connect).await;
        h.send(Command::CancelConnect).await;
        assert_eq!(h.state().conn, DISCONNECTED);
        h.source.set(Some(cookies("sk-new", None)));
        h.step(10).await;
        assert_eq!(h.state().conn, DISCONNECTED);
        assert!(h.creds.load().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn polls_every_30s() {
        let mut h = start(with_creds()).await;
        h.step(61).await;
        assert_eq!(h.calls(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn first_poll_never_reports_reset() {
        let mut h = start(Setup {
            usage: vec![Ok(usage(80.0, Some("2026-11-03T14:00:00Z"), 30.0))],
            ..with_creds()
        })
        .await;
        assert_eq!(h.state().reset_seq, 0);
        assert!(h.state().resets.is_empty());
        // reconnect: the first poll of the new connection must not compare with the old one
        h.source.set(Some(cookies("sk-old", Some("org-a"))));
        h.push_usage(Ok(usage(0.0, None, 0.0)));
        h.send(Command::Connect).await;
        assert_eq!(h.state().conn, Conn::Connected);
        assert_eq!(h.calls(), 2);
        assert_eq!(h.state().reset_seq, 0);
        assert!(h.state().resets.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn surprise_reset_increments_seq() {
        let mut h = start(Setup {
            start: "2026-11-03T11:46:00Z",
            usage: vec![Ok(usage(78.0, Some("2026-11-03T14:00:00Z"), 34.0)), Ok(usage(0.0, None, 34.0))],
            ..with_creds()
        })
        .await;
        assert_eq!(h.state().reset_seq, 0);
        h.step(30).await;
        let s = h.state();
        assert_eq!(s.reset_seq, 1);
        assert_eq!(s.resets.len(), 1);
        assert_eq!(s.resets[0].key, LimitKey::Session);
        assert_eq!(s.resets[0].kind, ResetKind::Surprise { early_secs: 2 * 3600 + 13 * 60 + 30 });
        assert_eq!(s.resets[0].from_used, 78.0);
        // a following poll without a drop keeps seq and the last events
        h.push_usage(Ok(usage(3.0, Some("2026-11-03T16:47:00Z"), 34.0)));
        h.step(30).await;
        assert_eq!(h.calls(), 3);
        assert_eq!(h.state().reset_seq, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn wake_after_sleep_single_poll_planned() {
        let mut h = start(Setup {
            start: "2026-11-03T13:59:00Z",
            usage: vec![Ok(usage(78.0, Some("2026-11-03T14:00:00Z"), 34.0)), Ok(usage(0.0, None, 34.0))],
            ..with_creds()
        })
        .await;
        assert_eq!(h.calls(), 1);
        h.jump(Duration::from_secs(6 * 3600)).await;
        assert_eq!(h.calls(), 2, "exactly one catch-up poll");
        let s = h.state();
        assert_eq!(s.reset_seq, 1);
        assert_eq!(s.resets.len(), 1);
        assert_eq!(s.resets[0].kind, ResetKind::Planned);
        assert_eq!(s.last_ok, Some(t("2026-11-03T19:59:00Z")));
        // pace recomputed at the new time: the idle session has no expected value
        assert_eq!(s.rings.as_ref().unwrap()[0].pace, Pace::Idle);
        h.step(1).await;
        assert_eq!(h.calls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_then_recovers() {
        let mut h = start(Setup {
            usage: vec![
                Ok(usage(20.0, Some("2026-11-03T14:00:00Z"), 5.0)),
                Err(ApiError::Unavailable("HTTP 503".into())),
                Err(ApiError::Unexpected { status: 200, body_head: "{}".into() }),
                Ok(usage(21.0, Some("2026-11-03T14:00:00Z"), 5.0)),
            ],
            ..with_creds()
        })
        .await;
        h.step(30).await; // first failure at 12:00:30
        let s = h.state();
        assert_eq!(s.error, Some(ErrorInfo { retry_at: t("2026-11-03T12:01:30Z") }));
        assert!(s.rings.is_some(), "rings kept on failure");
        assert_eq!(s.conn, Conn::Connected);
        h.step(59).await;
        assert_eq!(h.calls(), 2);
        h.step(1).await; // second failure at 12:01:30
        assert_eq!(h.state().error, Some(ErrorInfo { retry_at: t("2026-11-03T12:03:30Z") }));
        h.step(120).await; // success at 12:03:30
        let s = h.state();
        assert_eq!(h.calls(), 4);
        assert!(s.error.is_none());
        assert_eq!(s.rings.unwrap()[0].used, 21.0);
        h.step(30).await; // back to 30 s
        assert_eq!(h.calls(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn expired_recovers_silently_from_newer_cookie() {
        let fresh = cookies("sk-new", Some("org-a"));
        let h = start(Setup {
            usage: vec![Err(ApiError::Expired), Ok(usage(20.0, Some("2026-11-03T14:00:00Z"), 5.0))],
            source: Some(fresh.clone()),
            ..with_creds()
        })
        .await;
        let s = h.state();
        assert_eq!(s.conn, Conn::Connected);
        assert_eq!(h.calls(), 2);
        assert_eq!(h.creds.load(), Some(fresh));
        assert!(s.rings.is_some());
        assert!(s.error.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn expired_without_newer_cookie_goes_expired() {
        let mut h = start(Setup {
            usage: vec![Err(ApiError::Expired)],
            source: Some(cookies("sk-old", Some("org-b"))),
            ..with_creds()
        })
        .await;
        assert_eq!(h.state().conn, Conn::Expired);
        assert_eq!(h.calls(), 1);
        h.step(120).await;
        assert_eq!(h.calls(), 1, "no polling while expired");
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_polls_now_but_not_while_in_flight() {
        let mut h = start(Setup { delay: Duration::from_secs(5), ..with_creds() }).await;
        assert_eq!(h.calls(), 1);
        h.step(1).await;
        h.send(Command::Refresh).await;
        h.send(Command::Refresh).await;
        assert_eq!(h.calls(), 1, "refresh ignored while a poll is in flight");
        h.step(5).await; // first poll done at 5 s
        assert!(h.state().last_ok.is_some());
        h.send(Command::Refresh).await;
        assert_eq!(h.calls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn select_org_resets_prev_no_animation() {
        let mut h = start(Setup {
            usage: vec![Ok(usage(78.0, Some("2026-11-03T14:00:00Z"), 34.0)), Ok(usage(0.0, None, 0.0))],
            ..with_creds()
        })
        .await;
        h.send(Command::SelectOrg("org-b".into())).await;
        let s = h.state();
        assert_eq!(*h.api.usage_orgs.lock().unwrap(), vec!["org-a", "org-b"]);
        assert_eq!(s.reset_seq, 0);
        assert!(s.resets.is_empty());
        assert_eq!(s.org, Some(summary("org-b")));
        assert_eq!(h.settings.load().org_uuid.as_deref(), Some("org-b"));
    }

    #[tokio::test(start_paused = true)]
    async fn disconnect_clears_creds() {
        let mut h = start(with_creds()).await;
        h.send(Command::Disconnect).await;
        let s = h.state();
        assert_eq!(s.conn, DISCONNECTED);
        assert!(h.creds.load().is_none());
        assert!(h.settings.load().org_uuid.is_none());
        assert!(s.rings.is_none());
        h.step(60).await;
        assert_eq!(h.calls(), 1, "no polling after disconnect");
    }

    #[test]
    fn app_state_serializes_camel_case() {
        let s = AppState {
            conn: Conn::Connected,
            org: Some(summary("org-a")),
            orgs: vec![summary("org-a")],
            rings: Some([
                RingView {
                    key: LimitKey::Session,
                    used: 62.0,
                    resets_at: Some(t("2026-11-03T14:00:00Z")),
                    expected: Some(48.0),
                    pace: Pace::Ahead,
                },
                RingView { key: LimitKey::Weekly, used: 0.0, resets_at: None, expected: None, pace: Pace::Idle },
            ]),
            last_ok: Some(t("2026-11-03T12:00:00Z")),
            error: Some(ErrorInfo { retry_at: t("2026-11-03T12:01:00Z") }),
            resets: vec![],
            reset_seq: 3,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["conn"], serde_json::json!({"state": "connected"}));
        assert_eq!(v["lastOk"], "2026-11-03T12:00:00Z");
        assert_eq!(v["resetSeq"], 3);
        assert_eq!(v["error"]["retryAt"], "2026-11-03T12:01:00Z");
        assert_eq!(v["rings"][0]["resetsAt"], "2026-11-03T14:00:00Z");
        assert_eq!(v["rings"][0]["pace"], "ahead");
        assert_eq!(v["rings"][1]["key"], "weekly");
        assert!(v.get("last_ok").is_none() && v.get("reset_seq").is_none());

        let j = |c: Conn| serde_json::to_string(&c).unwrap();
        assert_eq!(j(Conn::Disconnected { timed_out: false }), r#"{"state":"disconnected","timedOut":false}"#);
        assert_eq!(j(Conn::NeedsFirefox), r#"{"state":"needsFirefox"}"#);
        assert_eq!(j(Conn::Waiting), r#"{"state":"waiting"}"#);
        assert_eq!(j(Conn::Expired), r#"{"state":"expired"}"#);
    }
}
