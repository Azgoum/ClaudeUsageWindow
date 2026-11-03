# Claude Limit Checker, Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship v1 of Claude Limit Checker as a Tauri 2 + React app: a glass rings widget on Windows that connects through Firefox, polls claude.ai every 30 s, shows pace, and animates planned and surprise resets. It replaces `token_monitor.py`.

**Architecture:** A UI-independent Rust crate (`crates/limits-core`) holds all the logic:
- parsing, pace, reset detection, org choice
- Firefox session reading, the HTTP client, settings and credentials
- an async `Engine` that receives `Command`s and publishes an `AppState` through a `tokio::sync::watch` channel

A thin Tauri shell (`src-tauri`) wires the engine to Windows (window, autostart, single instance, logging) and forwards state to a React 19 front end (`ui/`) built with beUI, Tailwind 4 and Motion. Phase 2 will replace `ui/` and the Tauri shell only.

**Tech Stack:**
- Rust 1.93, tokio, reqwest (rustls), serde, chrono, rusqlite (bundled), keyring, thiserror, tracing + file-rotate, wiremock (tests)
- Tauri 2 with the plugins single-instance and autostart, sys-locale
- React 19, TypeScript, Vite, Tailwind CSS 4, motion, beUI (shadcn CLI), lucide-react, vitest
- Node 22 for the dev fake server

**Spec:** `docs/superpowers/specs/2026-11-03-usage-widget-design.md` (design) and `docs/superpowers/specs/2026-11-03-usage-widget-prototype.html` (the visual and behavioral reference; open it in a browser). Read both before starting any task.

## Global Constraints

- Windows 10/11 only. Firefox is the only session source in v1, but `SessionSource` must stay a trait so Chrome can plug in later.
- **Internal ID `limitrings`** everywhere it persists:
  - Tauri identifier `dev.limitrings.app`
  - `%APPDATA%\limitrings\settings.json`
  - `%LOCALAPPDATA%\limitrings\logs\`
  - Credential Manager service `limitrings`
  - autostart entry name `limitrings`

  The display name **"Claude Limit Checker"** appears only in the window title, installer, README and About text.
- No Anthropic or Claude logo anywhere; the icon is the neutral rings motif. The README opens with "Unofficial, not affiliated with Anthropic".
- Network: only `https://claude.ai`. No telemetry. **Never log cookie values.**
- Poll every **30 s**. Back off after failures: **1 → 2 → 4 → 8 → 10 min max**, and return to 30 s after a success.
- Pace: `expected = clamp((now − (resets_at − W)) / W × 100, 0, 100)`, with W = 5 h (session) or 7 d (weekly). `diff = used − expected`.
  - **Ahead** if diff > 5 → `#fb7185`
  - **Room** if diff < −5 → `#7dd3fc`
  - otherwise **OnTrack** → `#34d399`
  - **Idle** (session with no `resets_at`) → `#5f5f69`
- Reset detection:
  - It's a reset if `prev.used ≥ 1` and `cur.used < prev.used − 0.5`.
  - It's **planned** if `now ≥ prev.resets_at − 60 s`; otherwise it's a surprise, with `early = prev.resets_at − now`.
  - Never on the first poll of a run, after an org switch, or after a reconnect.
- Reset-line text is **always white `#e7e7ea`** and keeps the same format at every level. **Nothing blinks or pulses.**
- All UI copy comes from the prototype's `I18N` object (FR and EN-GB), with these changes: "Waiting for your browser…" becomes "Waiting for Firefox…" / "En attente de Firefox…", and the Chrome strings are dropped.
- Close quits the process. Minimize keeps polling. No tray icon. Always on top and Start with Windows are both **on by default**.
- Releases are **unsigned**.
- Git: work on branch `rewrite/phase-1`. After every commit, `git push origin rewrite/phase-1` (use `-u` the first time). Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Fresh login only in the Firefox WAL file.** Right after a login, the new `sessionKey` sits in `cookies.sqlite-wal`, not in the main DB. Reading only `cookies.sqlite` would make Connect hang forever. Covered by `firefox::tests::reads_cookie_only_present_in_wal` (Task 5).
2. **The PC wakes from sleep hours later.** The first poll after waking sees the drop after `resets_at`. It must report one *planned* reset, not a surprise, and must not fire a burst of catch-up polls. Covered by `engine::tests::wake_after_sleep_single_poll_planned` (Task 8).
3. **Firefox rotated the session cookie while the app ran.** A 401 must first re-read Firefox and recover silently, before showing "Session expired". Covered by `engine::tests::expired_recovers_silently_from_newer_cookie` (Task 8).
4. **A monitor was unplugged.** A saved position off every monitor must snap to the primary monitor's top-right corner, not open invisible. Covered by `window_pos::tests::offscreen_snaps_to_primary_top_right` (Task 7).
5. **Odd API values.** `utilization` above 100, negative, or an integer instead of a float; `resets_at` with microseconds and `+00:00`. Values must clamp to 0–100 and parse; nothing may panic. Covered by `parse::tests::clamps_and_parses_odd_values` (Task 1).

---

## File Structure

```
Cargo.toml                         workspace: crates/limits-core, src-tauri
crates/limits-core/
  Cargo.toml
  src/lib.rs                       re-exports
  src/model.rs                     LimitKey, Window, Usage, Org
  src/parse.rs                     parse_usage, parse_orgs (+ fixtures in tests/fixtures/)
  src/pace.rs                      Pace, PaceInfo, pace(), backoff_delay()
  src/detect.rs                    ResetKind, ResetEvent, detect_resets()
  src/org.rs                       eligible_orgs(), choose_org()
  src/session/mod.rs               SessionCookies, SessionSource trait
  src/session/firefox.rs           FirefoxSource (profiles.ini + cookies.sqlite)
  src/api.rs                       ClaudeApi trait, HttpApi, ApiError
  src/store.rs                     Settings, SettingsStore, CredStore trait, KeyringCreds, MemCreds
  src/window_pos.rs                clamp_position()
  src/clock.rs                     Clock trait, SystemClock, test FakeClock
  src/engine.rs                    Command, Conn, RingView, AppState, Engine
  tests/fixtures/*.json
src-tauri/                         Tauri 2 shell (Cargo package `limitrings`)
  Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
  src/main.rs, src/lib.rs          setup, plugins, commands, state bridge
  src/launch.rs                    FirefoxLauncher (BrowserLauncher impl)
  src/logging.rs                   tracing → rotating file
ui/
  package.json, vite.config.ts, index.html, components.json, tsconfig*.json
  src/main.tsx, src/App.tsx, src/index.css
  src/lib/types.ts                 TS mirror of AppState
  src/lib/bridge.ts                invoke/listen wrappers
  src/lib/i18n.ts                  strings FR/EN (+ test)
  src/lib/format.ts                time/duration formatting (+ test)
  src/lib/colors.ts                pace → color
  src/components/Ring.tsx, Widget.tsx, RingTooltip.tsx, HoverControls.tsx,
                 Flags.tsx, WidgetMenu.tsx, ConnectPanel.tsx, Pill.tsx, Sparks.tsx
  src/anim/planned.ts, src/anim/surprise.ts, src/anim/spring.ts
tools/fake-claude/server.mjs       dev-only scripted claude.ai stand-in with a control page
.github/workflows/ci.yml, release.yml
README.md, README.fr.md, LICENSE
(deleted at the end: token_monitor.py, start.bat, requirements.txt, icons8-claude-48.png)
```

---

### Task 1: Workspace, core model and defensive parsing

**Files:**
- Create: `Cargo.toml`, `crates/limits-core/Cargo.toml`, `crates/limits-core/src/{lib.rs,model.rs,parse.rs}`, `crates/limits-core/tests/fixtures/{usage_real_2026-11-02.json,usage_both_active.json,usage_null_session.json,usage_null_both.json,usage_limits_only.json,usage_unknown.json,orgs_two.json}`

**Interfaces:**
- Produces:
  - `enum LimitKey { Session, Weekly }` (Copy, Eq, Serialize as `"session"` / `"weekly"`)
  - `struct Window { used: f64, resets_at: Option<DateTime<Utc>> }`
  - `struct Usage { session: Window, weekly: Window }` with `fn get(&self, k: LimitKey) -> &Window`
  - `struct Org { uuid: String, name: String, capabilities: Vec<String> }`
  - `enum ParseError { Json(String), Shape(String) }`
  - `fn parse_usage(body: &str) -> Result<Usage, ParseError>`
  - `fn parse_orgs(body: &str) -> Result<Vec<Org>, ParseError>`

- [ ] **Step 1: Create the branch and the workspace.**

  Run `git checkout -b rewrite/phase-1`. In the root `Cargo.toml`, add `[workspace] members = ["crates/limits-core"]` with `resolver = "2"`. `src-tauri` is added in Task 10.

  `limits-core` deps:
  - `serde` (derive), `serde_json`, `chrono` (serde), `thiserror`
  - `tokio` (rt-multi-thread, macros, sync, time)
  - `reqwest` (`default-features = false`, features `json`, `rustls-tls`, `cookies`)
  - `async-trait`, `rusqlite` (bundled), `keyring` (`windows-native`), `tracing`, `tempfile`

  Dev-deps: `wiremock`, `tokio` (`test-util`).

- [ ] **Step 2: Write the fixtures.**

  `usage_real_2026-11-02.json` is the response captured in the spec §4, keeping all keys: `five_hour: null`, `seven_day {utilization 1.0, resets_at "2026-11-06T23:59:59.657790+00:00"}`, and `limits[]`. The others vary it:
  - `usage_both_active`: five_hour 62.0, resets `2026-11-03T13:00:00+00:00`
  - `usage_null_both`: both `null`
  - `usage_limits_only`: no five_hour or seven_day keys; `limits:[{"kind":"session","group":"session","percent":40,"resets_at":"…"},{"kind":"weekly_all","group":"weekly","percent":12,"resets_at":"…","scope":null},{"kind":"weekly_scoped","group":"weekly","percent":0,"scope":{"model":{"display_name":"Fable"}}}]`
  - `usage_unknown`: `{"hello":"world"}`
  - `orgs_two`: two orgs shaped like the real ones (one with `["chat","claude_max"]`, one with `["api","api_individual"]`)

- [ ] **Step 3: Write the failing tests** in `parse.rs` (`#[cfg(test)] mod tests`, loading fixtures via `include_str!`):

```rust
#[test] fn real_response_null_session_is_idle() {
    let u = parse_usage(REAL).unwrap();
    assert_eq!(u.session.used, 0.0);
    assert!(u.session.resets_at.is_none());
    assert_eq!(u.weekly.used, 1.0);
    assert_eq!(u.weekly.resets_at.unwrap().to_rfc3339(), "2026-11-06T23:59:59.657790+00:00");
}
#[test] fn both_null_is_idle_not_error() { let u = parse_usage(NULL_BOTH).unwrap(); assert!(u.session.resets_at.is_none() && u.weekly.resets_at.is_none()); }
#[test] fn limits_only_fallback_ignores_scoped() { let u = parse_usage(LIMITS_ONLY).unwrap(); assert_eq!(u.session.used, 40.0); assert_eq!(u.weekly.used, 12.0); }
#[test] fn unknown_shape_is_shape_error() { assert!(matches!(parse_usage(UNKNOWN), Err(ParseError::Shape(_)))); }
#[test] fn not_json_is_json_error() { assert!(matches!(parse_usage("<html>"), Err(ParseError::Json(_)))); }
#[test] fn clamps_and_parses_odd_values() {
    let u = parse_usage(r#"{"five_hour":{"utilization":104,"resets_at":"2026-11-03T13:00:00Z"},"seven_day":{"utilization":-3.5,"resets_at":"2026-11-06T23:59:59.000001+00:00"}}"#).unwrap();
    assert_eq!(u.session.used, 100.0); assert_eq!(u.weekly.used, 0.0);
}
#[test] fn orgs_parse() { let o = parse_orgs(ORGS_TWO).unwrap(); assert_eq!(o.len(), 2); assert!(o[0].capabilities.contains(&"chat".into())); }
```

- [ ] **Step 4: Run** `cargo test -p limits-core parse`. Expected: FAIL (functions not defined).
- [ ] **Step 5: Implement `parse_usage` and `parse_orgs`.**

  Parse into `serde_json::Value`, not strict structs. The usage rules:
  - A present-but-null `five_hour` or `seven_day` → `Window { used: 0, resets_at: None }`.
  - If *both* keys are absent, fall back to `limits[]`:
    - session = the first entry with `group == "session"` or a `kind` starting with `"session"` or `"five_hour"`
    - weekly = the `kind == "weekly_all"` entry
    - ignore entries whose `scope` is non-null
  - If neither path yields anything → `Shape`.
  - `used` = `utilization` (or `percent` in the fallback) as f64, clamped to 0..=100.
- [ ] **Step 6: Run** `cargo test -p limits-core`. Expected: all PASS.
- [ ] **Step 7: Commit and push.** Message: `feat(core): usage and org parsing with null-safe fallback`.

### Task 2: Pace and back-off

**Files:**
- Create: `crates/limits-core/src/pace.rs`

**Interfaces:**
- Consumes: `LimitKey`, `Window` (Task 1)
- Produces:
  - `enum Pace { Ahead, OnTrack, Room, Idle }` (Serialize as lowercase)
  - `struct PaceInfo { expected: Option<f64>, diff: Option<f64>, pace: Pace }`
  - `fn window_len(k: LimitKey) -> chrono::Duration`
  - `fn pace(k: LimitKey, w: &Window, now: DateTime<Utc>) -> PaceInfo`
  - `fn backoff_delay(consecutive_failures: u32) -> std::time::Duration`

- [ ] **Step 1: Write the failing tests.**

```rust
// session resets 14:00 UTC; window started 09:00
#[test] fn ahead_when_more_than_5_over() { let p = pace(Session, &w(62.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:24:00Z")); assert_eq!(p.expected.unwrap().round(), 48.0); assert_eq!(p.pace, Pace::Ahead); }
#[test] fn on_track_within_5() { assert_eq!(pace(Session, &w(52.0, "2026-11-03T14:00:00Z"), t("2026-11-03T11:24:00Z")).pace, Pace::OnTrack); }
#[test] fn room_when_more_than_5_under() { assert_eq!(pace(Weekly, &w(34.0, "2026-11-06T23:59:00Z"), t("2026-11-03T11:30:00Z")).pace, Pace::Room); }
#[test] fn exactly_plus_5_is_on_track() { /* used = expected + 5.0 */ }
#[test] fn idle_session() { let p = pace(Session, &Window{used:0.0,resets_at:None}, t("2026-11-03T11:00:00Z")); assert_eq!(p.pace, Pace::Idle); assert!(p.expected.is_none()); }
#[test] fn expected_clamped_after_reset_time() { assert_eq!(pace(Session, &w(10.0, "2026-11-03T14:00:00Z"), t("2026-11-03T15:00:00Z")).expected, Some(100.0)); }
#[test] fn backoff_schedule() { let s: Vec<u64> = (0..7).map(|n| backoff_delay(n).as_secs()).collect(); assert_eq!(s, vec![30, 60, 120, 240, 480, 600, 600]); }
```

- [ ] **Step 2: Run** `cargo test -p limits-core pace`. Expected: FAIL.
- [ ] **Step 3: Implement** using the formula and thresholds from Global Constraints. The tolerance is inclusive (|diff| ≤ 5 → OnTrack). For a weekly window with no `resets_at`, return `Idle` as well.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): pace computation and back-off schedule`.

### Task 3: Reset detection

**Files:**
- Create: `crates/limits-core/src/detect.rs`

**Interfaces:**
- Consumes: `Usage`, `LimitKey` (Task 1)
- Produces:
  - `enum ResetKind { Planned, Surprise { early_secs: i64 } }` (Serialize tagged `{"kind":"planned"}` / `{"kind":"surprise","early_secs":…}`)
  - `struct ResetEvent { key: LimitKey, kind: ResetKind, from_used: f64 }`
  - `fn detect_resets(prev: &Usage, cur: &Usage, now: DateTime<Utc>) -> Vec<ResetEvent>`

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn planned_at_reset_time() { let e = detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("14:00:20")); assert_eq!(e.len(), 1); assert_eq!(e[0].kind, ResetKind::Planned); assert_eq!(e[0].from_used, 78.0); }
#[test] fn planned_within_60s_tolerance() { assert_eq!(detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("13:59:10"))[0].kind, ResetKind::Planned); }
#[test] fn surprise_reports_early_seconds() { let e = detect_resets(&u(78.0, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("11:46:00")); assert_eq!(e[0].kind, ResetKind::Surprise { early_secs: 2*3600 + 14*60 }); }
#[test] fn both_at_once() { assert_eq!(detect_resets(&u(78.0, Some("14:00"), 60.0), &u(0.0, None, 0.0), at("12:00:00")).len(), 2); }
#[test] fn ignores_tiny_drop_and_low_prev() { assert!(detect_resets(&u(50.0, Some("14:00"), 34.0), &u(49.6, Some("14:00"), 34.0), at("12:00:00")).is_empty()); assert!(detect_resets(&u(0.8, Some("14:00"), 34.0), &u(0.0, None, 34.0), at("12:00:00")).is_empty()); }
#[test] fn prev_without_resets_at_counts_as_surprise_zero() { /* prev idle-with-usage edge: early_secs 0 */ }
```

- [ ] **Step 2: Run** `cargo test -p limits-core detect`. Expected: FAIL.
- [ ] **Step 3: Implement** with the rule from Global Constraints. If `prev.resets_at` is `None`, classify the reset as `Surprise { early_secs: 0 }`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): planned vs surprise reset detection`.

### Task 4: Org choice and session cookies

**Files:**
- Create: `crates/limits-core/src/org.rs`, `crates/limits-core/src/session/mod.rs`

**Interfaces:**
- Consumes: `Org` (Task 1)
- Produces:
  - `fn eligible_orgs(orgs: &[Org]) -> Vec<&Org>` (those with `"chat"`)
  - `fn choose_org<'a>(orgs: &'a [Org], saved: Option<&str>, last_active: Option<&str>) -> Option<&'a Org>`, with priority **saved** (the user picked it in the menu) → **lastActiveOrg** → first eligible, considering eligible orgs only
  - `struct SessionCookies { pub cookies: Vec<(String, String)> }` with `fn header(&self) -> String` (`"a=1; b=2"`), `fn has_session(&self) -> bool` (a non-empty `sessionKey` exists), `fn last_active_org(&self) -> Option<&str>`, `fn fingerprint(&self) -> u64` (hash of the `sessionKey` value only, used to tell whether Firefox has a newer session)
  - `trait SessionSource: Send + Sync { fn read(&self) -> anyhow::Result<Option<SessionCookies>>; }` (add the `anyhow` dep)
  - `impl Debug for SessionCookies` that prints names only, never values

- [ ] **Step 1: Write the failing tests.** `choose_org`:
  - saved eligible wins
  - saved but API-only is ignored, so lastActive wins
  - unknown lastActive → first eligible
  - no eligible → `None`

  `SessionCookies`:
  - `header()` joins with `"; "`
  - `has_session()` is false for an empty `sessionKey`
  - `format!("{:?}", c)` does not contain the secret value
- [ ] **Step 2: Run** `cargo test -p limits-core org session`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): org selection and session cookie type`.

### Task 5: Firefox session source

**Files:**
- Create: `crates/limits-core/src/session/firefox.rs`

**Interfaces:**
- Consumes: `SessionCookies`, `SessionSource` (Task 4)
- Produces:
  - `struct FirefoxSource { root: PathBuf }`, with `FirefoxSource::default_root() -> Option<PathBuf>` (`%APPDATA%\Mozilla\Firefox`) and `FirefoxSource::new(root)`
  - `fn profile_dirs(root: &Path) -> Vec<PathBuf>`: parse `profiles.ini`, every `[Profile*]` `Path=`, honoring `IsRelative`
  - `impl SessionSource for FirefoxSource`: `read` collects claude.ai cookies from every profile and returns the profile whose `sessionKey` row has the greatest `lastAccessed`. A missing root → `Ok(None)`.

- [ ] **Step 1: Write the failing tests.** Build fixtures in `tempfile::TempDir`:
  - A `profiles.ini` with one relative profile (`Profiles/abc.default`) and one absolute profile.
  - A `cookies.sqlite` created with the real Firefox schema subset: `CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER, creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER)`.

  Tests:
  - `parses_relative_and_absolute_profiles`
  - `reads_claude_cookies_only`: rows for `.claude.ai`, `claude.ai` and `.example.com`; only claude rows are returned, and `lastActiveOrg` is included
  - `picks_profile_with_most_recent_session`: two profiles, the newer `lastAccessed` wins
  - `no_session_returns_none`: claude cookies but no `sessionKey`
  - `reads_cookie_only_present_in_wal`: open the DB with `PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;`, insert `sessionKey`, **keep that connection open (simulating Firefox holding the DB)**, then call `read()`, which must find it
- [ ] **Step 2: Run** `cargo test -p limits-core firefox`. Expected: FAIL.
- [ ] **Step 3: Implement `read`.**
  1. Copy `cookies.sqlite` and, if present, `cookies.sqlite-wal` into a fresh `tempfile::TempDir`, keeping the base names.
  2. Open the copy with `rusqlite` (`SQLITE_OPEN_READ_ONLY` is not enough for WAL recovery, so open read-write on the *copy*).
  3. Query: `SELECT name, value, lastAccessed FROM moz_cookies WHERE host IN ('claude.ai','.claude.ai')`.
  4. A missing or locked file in one profile → skip it with a `tracing::warn!` (path only).
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Check against your real Firefox, read-only.** Add an `#[ignore]` test `real_firefox_smoke` that runs `FirefoxSource::new(default_root())` and asserts that `read()` returns `Ok(_)`. Print only `has_session()` and the cookie *names*. Run `cargo test -p limits-core real_firefox_smoke -- --ignored --nocapture`. Expected: `has_session: true` while you're logged in to claude.ai in Firefox.
- [ ] **Step 6: Commit and push.** Message: `feat(core): Firefox cookie session source with WAL support`.

### Task 6: HTTP client and error classification

**Files:**
- Create: `crates/limits-core/src/api.rs`

**Interfaces:**
- Consumes: `parse_usage`, `parse_orgs`, `Org`, `Usage` (Task 1), `SessionCookies` (Task 4)
- Produces:
  - `enum ApiError { Expired, Unavailable(String), Unexpected { status: u16, body_head: String } }` (`body_head` = first 2048 bytes)
  - `#[async_trait] trait ClaudeApi: Send + Sync { async fn orgs(&self, c: &SessionCookies) -> Result<Vec<Org>, ApiError>; async fn usage(&self, c: &SessionCookies, org_uuid: &str) -> Result<Usage, ApiError>; }`
  - `struct HttpApi`, with `HttpApi::new(base: url::Url) -> Self` (timeout 15 s; headers `Accept: application/json`, `Referer: {base}/settings/usage`, `Origin: {base}`, a browser-like `User-Agent`) and `HttpApi::claude()` (base `https://claude.ai`)

- [ ] **Step 1: Write the failing tests** with `wiremock`:
  - 200 + the real fixture → `Ok(Usage)`
  - 401 → `Expired`
  - 403 with `content-type: application/json` and body `{"type":"error","error":{"type":"permission_error"}}` → `Expired`
  - 403 HTML body containing `cf-` / "Just a moment" → `Unavailable`
  - 503 → `Unavailable`
  - 200 with body `{"hello":"world"}` → `Unexpected { status: 200, .. }` with `body_head` starting with `{"hello"`
  - the request carries the `Cookie` header from `SessionCookies::header()` (wiremock `header` matcher)
  - a 16 s delay → `Unavailable` (use a 1 s timeout in tests via `HttpApi::with_timeout`)
- [ ] **Step 2: Run** `cargo test -p limits-core api`. Expected: FAIL.
- [ ] **Step 3: Implement.** On error paths, log status and `body_head`, **never the cookie header**.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): claude.ai HTTP client with error classification`.

### Task 7: Settings, credentials, window position

**Files:**
- Create: `crates/limits-core/src/store.rs`, `crates/limits-core/src/window_pos.rs`

**Interfaces:**
- Consumes: `SessionCookies` (Task 4)
- Produces:
  - `struct Settings { position: Option<(i32, i32)>, language: Option<String>, org_uuid: Option<String>, always_on_top: bool /*default true*/, start_with_windows: bool /*default true*/ }` (serde, `#[serde(default)]`)
  - `struct SettingsStore { path: PathBuf }` with `fn load(&self) -> Settings` (missing or corrupt file → defaults plus a warn) and `fn save(&self, s: &Settings) -> std::io::Result<()>` (write to `*.tmp`, then rename)
  - `trait CredStore: Send + Sync { fn load(&self) -> Option<SessionCookies>; fn save(&self, c: &SessionCookies) -> anyhow::Result<()>; fn clear(&self) -> anyhow::Result<()>; }`
  - `struct KeyringCreds` (service `limitrings`, user `session`, value = the JSON of the cookies) and `struct MemCreds` (a `Mutex<Option<…>>`, for tests)
  - `struct Rect { x: i32, y: i32, w: i32, h: i32 }`
  - `fn clamp_position(saved: (i32, i32), size: (i32, i32), monitors: &[Rect], primary: Rect) -> (i32, i32)`

- [ ] **Step 1: Write the failing tests.**
  - `settings_defaults_when_missing`
  - `settings_roundtrip`
  - `settings_corrupt_file_gives_defaults`
  - `memcreds_roundtrip_and_clear`
  - `window_pos::tests`:
    - `onscreen_kept`
    - `partially_offscreen_kept_if_at_least_40px_visible`
    - `offscreen_snaps_to_primary_top_right`: expect `(primary.x + primary.w − size.0 − 16, primary.y + 16)`
  - `#[ignore] keyring_roundtrip`: real Credential Manager; save, load, clear
- [ ] **Step 2: Run** `cargo test -p limits-core store window_pos`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, plus `cargo test -p limits-core keyring_roundtrip -- --ignored`. Expected: PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): settings, credential store and window position clamp`.

### Task 8: Engine (state machine)

**Files:**
- Create: `crates/limits-core/src/clock.rs`, `crates/limits-core/src/engine.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–7
- Produces:
  - `trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }`, `SystemClock`, and, under `#[cfg(any(test, feature = "test-util"))]`, `FakeClock` with `fn advance(&self, d: chrono::Duration)`
  - `trait BrowserLauncher: Send + Sync { fn firefox_installed(&self) -> bool; fn open_login(&self) -> anyhow::Result<()>; }`
  - `enum Command { Connect, CancelConnect, Refresh, SelectOrg(String), Disconnect }`
  - `enum Conn { Disconnected { timed_out: bool }, NeedsFirefox, Waiting, Connected, Expired }`, serialized with `#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]`. JSON: `{"state":"disconnected","timedOut":false}`, `{"state":"waiting"}`, `{"state":"connected"}`, and so on.
  - `struct RingView { key: LimitKey, used: f64, resets_at: Option<DateTime<Utc>>, expected: Option<f64>, pace: Pace }`
  - `struct OrgSummary { uuid: String, name: String }`
  - `struct ErrorInfo { retry_at: DateTime<Utc> }`
  - `struct AppState`:
    ```
    { conn: Conn,
      org: Option<OrgSummary>, orgs: Vec<OrgSummary>,
      rings: Option<[RingView; 2]>,          // [session, weekly]
      last_ok: Option<DateTime<Utc>>,
      error: Option<ErrorInfo>,
      resets: Vec<ResetEvent>, reset_seq: u64 }  // UI animates when reset_seq changes
    ```
    All of these are Serialize, with camelCase field names.
  - `struct EngineDeps { api, source, creds, settings: SettingsStore, clock, browser }`, each an `Arc<dyn …>` except `settings`
  - `fn spawn_engine(deps: EngineDeps) -> (tokio::sync::mpsc::Sender<Command>, tokio::sync::watch::Receiver<AppState>)`

Behavior, which defines the tests:
- **Start:** credentials stored → `Connected` and poll immediately; otherwise `Disconnected{timed_out:false}`.
- **Connect:**
  - `!browser.firefox_installed()` → `NeedsFirefox`.
  - Otherwise `open_login()` → `Waiting`, then call `source.read()` every **2 s**. Once `has_session()`:
    1. `api.orgs` → `choose_org(saved, last_active)`
    2. save the credentials, and the org into settings
    3. `Connected`, then poll
  - No session after **10 min** → `Disconnected{timed_out:true}`.
  - `CancelConnect` → `Disconnected{timed_out:false}`.
- **Poll, while Connected:** one in flight at a time.
  - Success:
    1. Run `detect_resets(prev, cur, now)` only if `prev` exists for this org in this run. If any resets → set `resets` and `reset_seq += 1`.
    2. Store `prev = cur`, set `rings`, `last_ok`, `error = None`.
    3. Next poll in 30 s.
  - `Unavailable` / `Unexpected` → `error = Some(retry_at)` per `backoff_delay(failures)`. Keep `rings`.
  - `Expired`:
    1. Re-read the source. If it returns a session with a **different** `fingerprint`, save it and retry immediately.
    2. Otherwise → `Expired`, and clear `prev`.
- **Timing:** schedule each poll with a `tokio::time::sleep_until` deadline. A late deadline (after sleep) fires **once**, never one per missed interval. The rings' `expected` and `pace` are recomputed on every poll tick.
- **Refresh** → poll now. If a poll is already in flight, ignore the command.
- **SelectOrg(uuid)** → save it to settings, set `prev = None`, poll now.
- **Disconnect** → `creds.clear()`, clear `org_uuid`, `prev = None` → `Disconnected{timed_out:false}`.
- **Logging:** log every state transition at info level (no cookie values).

- [ ] **Step 1: Write the failing tests.**

  Write them in `engine.rs` with `#[tokio::test(start_paused = true)]`, plus these fakes:
  - `FakeApi`: scripted `VecDeque<Result<Usage, ApiError>>`, plus a counter of calls
  - `FakeSource`: settable `Option<SessionCookies>`
  - `MemCreds`
  - `FakeClock`, advanced together with `tokio::time::advance`
  - `FakeBrowser`: `installed: bool`, records `open_login` calls

  Helper: `async fn settle()`, which yields until the watch channel is quiet.

  Tests:
  - `starts_disconnected_without_creds`
  - `starts_connected_and_polls_with_creds` (expect 1 usage call)
  - `connect_without_firefox_needs_firefox`
  - `connect_flow_waits_then_connects_when_cookie_appears`: the source returns a session after 6 s; assert `Waiting` at 2 s, `Connected` after, creds saved, and the org chosen by `lastActiveOrg`
  - `connect_times_out_after_10_min`
  - `polls_every_30s` (3 calls after 61 s)
  - `first_poll_never_reports_reset`
  - `surprise_reset_increments_seq`
  - `wake_after_sleep_single_poll_planned`: last poll at 13:59 with session resetting at 14:00 and 78% used; advance both clocks by 6 h at once; expect **exactly 1** new usage call, plus one `Planned` event
  - `backoff_then_recovers`: 2 failures → `retry_at` = +60 s then +120 s; a success clears `error`
  - `expired_recovers_silently_from_newer_cookie`: the API returns `Expired` once; the source has a different `sessionKey` → stays `Connected`, the new creds are saved, and a poll is retried
  - `expired_without_newer_cookie_goes_expired`
  - `select_org_resets_prev_no_animation`
  - `disconnect_clears_creds`
- [ ] **Step 2: Run** `cargo test -p limits-core engine`. Expected: FAIL.
- [ ] **Step 3: Implement `spawn_engine`.** Structure it as one `tokio::select!` loop over: the command receiver, the poll deadline, and the waiting-tick deadline. Don't add other tasks or locks.
- [ ] **Step 4: Run** `cargo test -p limits-core`. Expected: all PASS.
- [ ] **Step 5: Commit and push.** Message: `feat(core): engine state machine with polling, connect flow and recovery`.

### Task 9: Dev fake claude.ai server

**Files:**
- Create: `tools/fake-claude/server.mjs`, `tools/fake-claude/README.md`
- Modify: `crates/limits-core/src/session/mod.rs` (add `FixedSource`)

**Interfaces:**
- Produces:
  - **The server:** `node tools/fake-claude/server.mjs [--port 8787]`.
    - It serves `GET /api/organizations` and `GET /api/organizations/:uuid/usage` with the same JSON shapes as the fixtures. It accepts any cookie.
    - Control page `GET /_ctl` (HTML), with buttons that mirror the prototype panel:
      - "use Claude" (+N% per minute)
      - "jump to 45 s before session reset"
      - "surprise reset session / weekly / both"
      - "go 92%"
      - "fail next 3 (503)", "next is 401", "next is Cloudflare 403", "unknown shape"
      - toggle 1 or 2 orgs
    - `POST /_ctl/:action` drives the same actions.
    - Time is real; "jump" shifts the server's notion of `resets_at` instead.
  - **Dev hooks**, honored **only in debug builds** (`cfg!(debug_assertions)`) and read by the Tauri shell in Task 10:
    - `LIMITRINGS_BASE_URL=http://127.0.0.1:8787` makes `HttpApi` use that base.
    - `LIMITRINGS_FAKE_SESSION=1` replaces `FirefoxSource` with a `FixedSource` that returns `sessionKey=dev; lastActiveOrg=<first org uuid>` 5 s after `open_login`. The launcher becomes a no-op.

- [ ] **Step 1: Write the server.** No dependencies, Node 22 `http` only.
- [ ] **Step 2: Check it.** Run `node tools/fake-claude/server.mjs` and `curl -s http://127.0.0.1:8787/api/organizations/x/usage`. Expected: JSON with `five_hour.utilization` 62. After `curl -X POST http://127.0.0.1:8787/_ctl/surprise-session`, the next GET shows `five_hour: null`.
- [ ] **Step 3: Add `FixedSource`.** Put it in `session/mod.rs` behind `#[cfg(debug_assertions)]`, along with a unit test showing it returns a session only after `arm()` + 5 s.
- [ ] **Step 4: Commit and push.** Message: `chore(dev): scripted fake claude.ai server and fixed session source`.

### Task 10: Tauri shell

**Files:**
- Create: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,src/main.rs,src/lib.rs,src/launch.rs,src/logging.rs,icons/*}`, plus a placeholder `ui/` (scaffolded for real in Task 11)
- Modify: root `Cargo.toml` (add `src-tauri` to members)

**Interfaces:**
- Consumes: `spawn_engine`, `EngineDeps`, `Command`, `AppState`, `SettingsStore`, `KeyringCreds`, `FirefoxSource`, `HttpApi`, `clamp_position`
- Produces, for the UI in Tasks 11–14:
  - Tauri event **`state`** with payload = the `AppState` JSON (camelCase), emitted on every watch change.
  - Commands:
    - `get_state() -> AppState`
    - `send_command(cmd: CommandDto)` where `CommandDto = { type: "connect" | "cancelConnect" | "refresh" | "selectOrg" | "disconnect", uuid?: string }`
    - `get_prefs() -> Prefs` with `{ language: "fr" | "en", alwaysOnTop: bool, startWithWindows: bool }`
    - `set_language(lang: String)`
    - `set_always_on_top(on: bool)`
    - `set_start_with_windows(on: bool)`
    - `quit()`: `app.exit(0)`
  - Language default: settings `language`, else the `sys-locale` value starting with `fr` → `"fr"`, otherwise `"en"`.

- [ ] **Step 1: Scaffold.**
  - `src-tauri` package name `limitrings`, binary `limitrings`.
  - Deps: `tauri` 2, `tauri-plugin-single-instance`, `tauri-plugin-autostart`, `sys-locale`, `tracing-subscriber`, `file-rotate`, `winreg`, `limits-core` (path).
  - `tauri.conf.json`:
    - `productName` "Claude Limit Checker", `identifier` `dev.limitrings.app`
    - `build.devUrl` `http://localhost:5173`, `frontendDist` `../ui/dist`, `beforeDevCommand` `npm --prefix ../ui run dev`, `beforeBuildCommand` `npm --prefix ../ui run build`
    - one window `main`: 242×150, `decorations:false`, `transparent:true`, `resizable:false`, `alwaysOnTop:true`, `skipTaskbar:false`, `shadow:false`, `windowEffects:{effects:["acrylic"]}`, title "Claude Limit Checker"
    - bundle targets `nsis`
  - Placeholder `ui/index.html` with `<pre id=s>` showing `state` events, so this task can be verified before Task 11.
- [ ] **Step 2: Implement `launch.rs`: `FirefoxLauncher`, which implements `BrowserLauncher`.**
  - Find the exe from `HKLM\SOFTWARE\Mozilla\Mozilla Firefox` (`CurrentVersion`, then `<ver>\Main\PathToExe`), the same under `HKCU`, and finally `C:\Program Files\Mozilla Firefox\firefox.exe`.
  - `open_login` = `Command::new(exe).arg("https://claude.ai/login").spawn()`.
- [ ] **Step 3: Implement `logging.rs`.** Use `tracing-subscriber` with a writer to `%LOCALAPPDATA%\limitrings\logs\limitrings.log` through `file-rotate` (`ContentLimit::Bytes(1_000_000)`, 3 files). Level `info`, or `debug` in debug builds.
- [ ] **Step 4: Implement `lib.rs` setup.**
  - Plugins: single-instance (a second launch focuses and shows `main`), and autostart (`MacosLauncher::LaunchAgent`, ignored on Windows) under the name `limitrings`.
  - On the first run, apply `start_with_windows` from settings (default true); `set_start_with_windows` enables or disables it.
  - Build `EngineDeps`, honoring the Task 9 dev env vars in debug builds only.
  - Spawn the engine on the Tauri async runtime. Forward watch changes as `state` events.
  - Window:
    - Restore the position via `clamp_position` against `available_monitors()` / `primary_monitor()`. The default is the primary monitor's top-right.
    - Save the position on `WindowEvent::Moved`, debounced 500 ms.
    - Apply `always_on_top` from settings.
  - Close request → `app.exit(0)`.
- [ ] **Step 5: Verify with the placeholder UI.**
  1. In one terminal, run `node tools/fake-claude/server.mjs`.
  2. In another, run `set LIMITRINGS_BASE_URL=http://127.0.0.1:8787 && set LIMITRINGS_FAKE_SESSION=1 && cargo tauri dev` (install the CLI with `cargo install tauri-cli --version ^2` if missing).

  Expected:
  - The window shows state JSON with `conn.state: "disconnected"`.
  - `window.__TAURI__.core.invoke('send_command',{cmd:{type:'connect'}})` in devtools → `Waiting` → `Connected` after about 5 s, and `rings` populated.
  - "surprise-session" on `/_ctl` → `resetSeq` goes up within 30 s.
  - A second `limitrings.exe` launch focuses the first.
  - A log file appears.
- [ ] **Step 6: Commit and push.** Message: `feat(shell): Tauri shell wiring engine, window, autostart, logging`.

### Task 11: UI scaffold, types, i18n, formatting

**Files:**
- Create: `ui/*` (Vite React TS), `ui/src/lib/{types.ts,bridge.ts,i18n.ts,format.ts,colors.ts}`, `ui/src/lib/__tests__/{i18n.test.ts,format.test.ts}`

**Interfaces:**
- Consumes: the Task 10 events and commands
- Produces:
  - `types.ts`: TS types mirroring `AppState`, `RingView`, `ResetEvent`, `Conn`, `Prefs`, with the exact camelCase names from Task 8.
  - `bridge.ts`: `onState(cb): Promise<UnlistenFn>`, `getState()`, `send(cmd: CommandDto)`, `getPrefs()`, `setLanguage(l)`, `setAlwaysOnTop(b)`, `setStartWithWindows(b)`, `quit()`, `minimize()` (`getCurrentWindow().minimize()`), `startDrag()` (`getCurrentWindow().startDragging()`)
  - `i18n.ts`: `type Lang = 'fr' | 'en'` and `t(lang)`, which returns the string table. Keys and copy come verbatim from the prototype's `I18N`, with the Global Constraints changes. Add the keys:
    - `needFirefoxTitle`, `needFirefoxText`, `getFirefox`
      - EN: "Firefox needed" / "Claude Limit Checker needs Firefox for now. Chrome support is coming." / "Get Firefox"
      - FR: "Firefox requis" / "Claude Limit Checker a besoin de Firefox pour l'instant. Le support de Chrome arrive." / "Installer Firefox"
    - `timedOut`
      - EN: "Didn't see a login. Try again?"
      - FR: "Aucune connexion détectée. Réessayer ?"
    - `chipBoth`
      - EN: "★ Surprise reset · both limits"
      - FR: "★ Reset surprise · les deux limites"
    - `chipBothPlanned`
      - EN: "✓ Both limits reset"
      - FR: "✓ Les deux limites remises à zéro"
  - `format.ts`:
    - `fmtTime(d: Date): string` → "14:00" (24 h)
    - `fmtDay(d: Date, lang): string` → "Fri 23:59" / "ven. 23:59"
    - `fmtDur(ms: number, lang): string` → "45 s" / "2h29" / "3d 12h" / "3 j 12 h"
    - `resetLine(key, resetsAt: Date | null, now: Date, lang): string` → "resets 14:00" / "reset à 14:00" / "resets Fri 23:59" / "reset ven. 23:59" / "idle" / "inactive". Use the day form only for weekly limits more than 24 h away.
  - `colors.ts`: `PACE_COLOR: Record<Pace, string>` using the Global Constraints hex values, plus `GOLD = '#facc15'`.

- [ ] **Step 1: Scaffold.**
  - `npm create vite@latest ui -- --template react-ts`
  - Install `tailwindcss@4 @tailwindcss/vite motion lucide-react @tauri-apps/api@2`, and `vitest` as a dev dependency.
  - Initialize shadcn/beUI per beui.dev (`npx shadcn@latest init`, registry `@beui`).
  - Delete the Task 10 placeholder.
  - Font: Inter (`@fontsource-variable/inter`), falling back to Segoe UI Variable.
- [ ] **Step 2: Write the failing tests.**

  `format.test.ts`:
  - `fmtDur(45_000,'en')==='45 s'`
  - `fmtDur((2*60+29)*60_000,'en')==='2h29'`
  - `fmtDur((3*24+12)*3_600_000,'fr')==='3 j 12 h'`
  - `resetLine('weekly', fri2359, tue1130, 'fr')==='reset ven. 23:59'`
  - `resetLine('session', null, now, 'en')==='idle'`
  - `resetLine('weekly', in20h, now, 'en')` uses the time-only form

  `i18n.test.ts`:
  - The FR and EN tables have identical key sets.
  - No value contains "Chrome", except `needFirefoxText`.
- [ ] **Step 3: Run** `npm --prefix ui test -- --run`. Expected: FAIL.
- [ ] **Step 4: Implement** `types.ts`, `bridge.ts`, `i18n.ts`, `format.ts`, `colors.ts`.
- [ ] **Step 5: Run** `npm --prefix ui test -- --run`. Expected: PASS.
- [ ] **Step 6: Commit and push.** Message: `feat(ui): scaffold with beUI, i18n and formatting`.

### Task 12: Widget, rings, tooltip, hover controls, menu

**Files:**
- Create: `ui/src/components/{Widget.tsx,Ring.tsx,RingTooltip.tsx,HoverControls.tsx,Flags.tsx,WidgetMenu.tsx}`, `ui/src/App.tsx`, `ui/src/index.css`

**Interfaces:**
- Consumes: `bridge.ts`, `i18n.ts`, `format.ts`, `colors.ts` (Task 11)
- Produces:
  - `<Ring view={RingView} lang ref>` exposing `RingHandle { svg, arc, glint, tick, num, ok }` refs, for Task 14 animations
  - `<Widget state prefs onLang>`
  - `App` holds `state` (from `onState`), `prefs` and `lang`

Match the prototype pixel-for-pixel. The values to copy are in its CSS block:
- **Glass:** `rgba(22,22,28,.66)`, 1 px `rgba(255,255,255,.09)` border, 16 px radius. The body background is transparent because Tauri acrylic sits behind it.
- **Rings padding:** `24px 14px 22px`.
- **Ring:** 64 px SVG, r=23, stroke 6, round caps, rotated −90°, track `rgba(255,255,255,.08)`. White tick at `expected × 3.6°`, `x1=32 y1=3 x2=32 y2=12`, width 2.
- **Number:** 13.5 px / 750 weight in the pace color, with "%" as a 9 px `tspan`. The French "%" gets a space before it **only in text** (tooltip), not inside the ring.
- **Title:** 11 px / 650 `#c9c9d1`. **Reset line:** 10.5 px `#e7e7ea`. Both centered. The ✓ is `position:absolute` so it never shifts centering.
- **Near limit (any `used ≥ 90`):** a red glow `box-shadow`. No pulse.
- **Error:** `filter: grayscale(1); opacity: .5` on the rings, plus a red line using `err(fmtDur(retryAt − now))`, updated every second.
- **Hover-only controls** (opacity transition .2 s):
  - Flags top-left, as SVG copied from the prototype.
  - Top-right: `Minus` and `X` from lucide-react, 12 px, stroke 2, in 22×18 hit boxes. X hovers to `#e81123`.
  - Bottom-right: `RefreshCw`.
  - Refresh click: the button scales to .86 and back, and the icon makes one 360° turn using the prototype's `SPRING` easing over 900 ms via motion's `animate`. Clicks are ignored while it turns. Then `send({type:'refresh'})`.
- **Drag:** `onPointerDown` on the widget root calls `startDrag()` unless the target is inside `button, [data-nodrag]`.
- **Tooltip** (`RingTooltip`), on ring hover:
  - Title: `org.name · ago(lastOk)`.
  - Lines: `used · expected`, the pace phrase in its color, then `resetsLong`. Idle variant included.
  - Refresh it every 1 s while open, and position it below the ring, flipping above if it doesn't fit.
  - Use beUI's tooltip if its API allows custom content; otherwise a Motion `AnimatePresence` div.
- **Right-click menu** (`WidgetMenu`): Refresh now, then Organization (only if `orgs.length > 1`, with a check on the current one) → `selectOrg`, then Always on top ✓, Start with Windows ✓, Disconnect, Quit. Use beUI's context or dropdown menu if available, styled like the prototype's `.menu`.

- [ ] **Step 1: Implement the components.**
- [ ] **Step 2: Run** `npx tsc --noEmit -p ui` and `npm --prefix ui test -- --run`. Expected: no errors; tests PASS.
- [ ] **Step 3: Check it visually against the fake server.** Run it as in Task 10 Step 5, connect, then compare with the prototype opened side by side. Check:
  - Ring sizes, colors, tick position, "%" size, white reset lines, centering.
  - Flags switch every string instantly and persist across a restart.
  - Hover reveals all controls; X quits the process (no `limitrings.exe` left in Task Manager); "–" minimizes, the taskbar icon restores, and polling continues (fake server log shows requests).
  - Right-click menu items work; "go 92%" shows the red glow and nothing blinks.
- [ ] **Step 4: Commit and push.** Message: `feat(ui): rings widget, tooltip, hover controls, menu`.

### Task 13: Connection screens and states

**Files:**
- Create: `ui/src/components/ConnectPanel.tsx`
- Modify: `ui/src/components/Widget.tsx`

**Interfaces:**
- Consumes: `AppState.conn`, `send()`, `t()`
- Produces: `<ConnectPanel conn lang>`, rendered by `Widget` whenever `conn.state !== 'connected'`

Screens (copy from i18n; layout per the prototype's `.st` block, padding `26px 18px 14px`, width 242):
- `{state:'disconnected', timedOut:false}`: `cTitle`, `cText`, a gradient button `cBtn` → `connect`.
- `{state:'disconnected', timedOut:true}`: the same, with `timedOut` as the text.
- `{state:'waiting'}`: spinner, "Waiting for Firefox…", the Firefox line from `wOther('Firefox')`, links `again` (→ `connect`) and `cancel` (→ `cancelConnect`).
- `{state:'needsFirefox'}`: `needFirefoxTitle`, `needFirefoxText`, button `getFirefox`, which opens `https://www.mozilla.org/firefox/` via `@tauri-apps/plugin-opener` (add the plugin in `src-tauri`, with the capability limited to that URL).
- `{state:'expired'}`: `eTitle`, `eText`, button `eBtn` → `connect`.

Hover controls (flags, –, ✕) stay available on every screen; refresh only shows when Connected.

- [ ] **Step 1: Implement.**
- [ ] **Step 2: Check each screen.**
  - With the fake server: start without creds; **Disconnect** from the menu → the first-launch screen; Connect → Waiting → Connected about 5 s later.
  - `/_ctl` "next is 401" → Expired → Reconnect works.
  - Rename the Firefox exe lookup with a temporary env var `LIMITRINGS_FAKE_NO_FIREFOX=1` (debug only; add it to `launch.rs`) → NeedsFirefox.
  - Cancel works.
- [ ] **Step 3: Commit and push.** Message: `feat(ui): connect, waiting, needs-Firefox and expired screens`.

### Task 14: Animations

**Files:**
- Create: `ui/src/anim/{spring.ts,planned.ts,surprise.ts}`, `ui/src/components/{Pill.tsx,Sparks.tsx}`
- Modify: `ui/src/components/Widget.tsx`, `ui/src/components/Ring.tsx`

**Interfaces:**
- Consumes: `RingHandle` (Task 12), `AppState.resets` / `resetSeq`, `GOLD`
- Produces:
  - `SPRING`: the exact `linear(…)` string from the prototype
  - `playPlanned(h: RingHandle, fromUsed: number): Promise<void>`
  - `playSurprise(h: RingHandle, widgetEl: HTMLElement, fromUsed: number, sparks: SparksApi): Promise<void>`
  - `<Pill>` with an imperative `show(kind: 'calm' | 'gold', text, holdMs)`
  - `<Sparks>` with an imperative `burst(center, radius, arcFraction, n = 30)`

Port the timings, keyframes and colors verbatim from the prototype's `animPlanned`, `animSurprise`, `sparks` and `chip` functions, using motion's `animate` instead of WAAPI.

Rules:
- When `resetSeq` changes, lock the affected rings: `Ring` ignores new `view` props until its animation's promise resolves, then applies the latest view. Then play the animations in parallel.
- Show **one** pill:
  - one planned reset → `chipS` / `chipW`
  - one surprise reset → `surprise(fmtDur(early))`
  - two resets → `chipBoth` if any is a surprise, otherwise `chipBothPlanned`
- Entrance: when `rings` first goes from null to set, arcs spring from empty (prototype `IntersectionObserver` block: 1200 ms, `cubic-bezier(.2,.9,.25,1.05)`, 90 ms stagger).
- Respect `prefers-reduced-motion`: skip sweeps and sparks, and just crossfade the number and show the pill.

- [ ] **Step 1: Implement.**
- [ ] **Step 2: Check them against the fake server**, with the prototype open side by side:
  - `/_ctl` "jump to 45 s before session reset" with "use Claude" on → within about 75 s the P2 sweep plays, then the green pill.
  - "surprise reset session" → gold burst + glow + gold pill with the right "early" duration.
  - "surprise both" → one pill, `chipBoth`.
  - Minimize during a reset, then restore: no stuck locked ring.
  - Check timings by eye, frame by frame against the prototype's ▶ Replay.
- [ ] **Step 3: Commit and push.** Message: `feat(ui): planned and surprise reset animations, entrance, pill`.

### Task 15: Packaging, CI, README, retire the Python app

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `README.fr.md`, `LICENSE` (MIT, © 2026 Aziz Goumiri), `src-tauri/icons/*` (generated)
- Delete: `token_monitor.py`, `start.bat`, `requirements.txt`, `icons8-claude-48.png`, `state.json` (untracked; leave it on disk but stop referencing it)
- Modify: `.gitignore` (add `target/`, `ui/node_modules/`, `ui/dist/`)

- [ ] **Step 1: Icon.** Draw a 1024×1024 PNG of the neutral rings motif: two concentric arcs, coral `#f0b38f` on dark, like the prototype's taskbar icon. Generate the set with `cargo tauri icon icon.png`.
- [ ] **Step 2: CI.**
  - `ci.yml`: on push and PR, `windows-latest`: `cargo test --workspace`, `npm ci && npm test -- --run && npx tsc --noEmit` in `ui`, and `cargo tauri build --debug` (smoke).
  - `release.yml`: on tag `v*`, `cargo tauri build` → upload the NSIS installer and a zip of `limitrings.exe` to a GitHub Release. Unsigned, with a comment marking where signing would plug in.
- [ ] **Step 3: README.md (EN) and README.fr.md (FR).** In this order:
  1. "Unofficial, not affiliated with Anthropic"
  2. a screenshot (taken from the running app with fake data)
  3. what it shows (rings, tick, colors)
  4. requirements: **Windows 10/11 + Firefox**
  5. install: the SmartScreen warning and "More info → Run anyway", with an explanation of why releases are unsigned
  6. Connect, and exactly what is read: claude.ai cookies from the Firefox profile, stored in Windows Credential Manager, sent only to claude.ai
  7. that it uses an undocumented claude.ai endpoint, may break, and sits in a terms-of-service gray area
  8. privacy: no telemetry
  9. build from source: `cargo tauri build`, plus the fake server for development
  10. license
- [ ] **Step 4: Delete the Python app files.** Run `cargo test --workspace` and `cargo tauri build`. Expected: tests PASS and `src-tauri/target/release/bundle/nsis/*.exe` exists. Record its size in the PR description (target < 10 MB).
- [ ] **Step 5: Commit and push.** Message: `chore: packaging, CI, README; retire Python app`.

### Task 16: Acceptance on the real account

**Files:** none (a checklist); fix-up commits as needed

- [ ] **Step 1:** Install the built NSIS installer and launch it. SmartScreen appears as the README describes.
- [ ] **Step 2: Connect with real Firefox.**
  - Already logged in → Connected within about 2 s of clicking Connect.
  - Log out of claude.ai in Firefox, Disconnect in the app, Connect again, log in → Connected.
- [ ] **Step 3:** Rings match claude.ai/settings/usage (same % and reset times, in local time). Tooltip shows "Max plan · Personal" or the real org name.
- [ ] **Step 4:** Leave it running for over 30 min, minimized for part of it. Then check:
  - idle CPU ~0% in Task Manager
  - record RAM (expected 50–100 MB with WebView2)
  - the log shows a poll every 30 s
  - no cookie values in the log
- [ ] **Step 5:** Reboot: it starts with Windows at the saved position. Turn off "Start with Windows" → it doesn't start after the next reboot.
- [ ] **Step 6:** Cut Wi-Fi for 3 min → grayed rings and the retry message; Wi-Fi back → recovers within the back-off delay.
- [ ] **Step 7:** Open a PR from `rewrite/phase-1` to `master` with the checklist results, RAM and installer size. Merge only after the owner approves.
