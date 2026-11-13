# Claude Limit Checker, Phase 2 (native egui UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tauri + React shell with a single statically linked Rust `.exe` (egui) that draws the same widget as Phase 1, idles under 25 MB private bytes and ≈ 0 % CPU, and downloads in under 6 MB.

**Architecture:** A new crate `app/` (lib `limitrings_app`, bin `limitrings`) talks to the unchanged `limits-core` engine through `spawn_engine` (commands in, `watch<AppState>` out), exactly as `src-tauri` does today. The UI is renderer-agnostic: `Widget::show(ctx, &FrameInput) -> FrameOutput` paints with an `egui::Context` and returns *actions* (send a command, minimize, start a drag, ...) plus the size it needs; a thin host (eframe, or a layered-window host if the spike picks the CPU path) applies them to the real window. Everything visual and time-driven is a pure function of `(state, now, animation clock)`, so it is unit-tested and snapshot-tested headless with `egui_kittest`. Windows-only code sits in `app/src/platform/windows.rs`.

**Tech Stack:** Rust 1.93 (MSVC), egui / eframe 0.37 (renderer chosen by Task 2), chrono, tokio, `windows-sys` 0.61, `winreg` 0.55, `winresource` (build), `sys-locale`, `tracing` + `file-rotate`; dev: `egui_kittest` 0.37 (`wgpu`, `snapshot` features), `ttf-parser`, `tempfile`. Inter (OFL) static TTFs, subset with `pyftsubset` (fonttools 4.62 is installed).

**Spec:** `docs/superpowers/specs/2026-11-05-egui-native-ui-design.md`. It builds on `docs/superpowers/specs/2026-11-03-usage-widget-design.md` (data, pace, states, wording) and the visual reference `docs/superpowers/specs/2026-11-03-usage-widget-prototype.html`. **Phase 1's code is the behavioral reference:** `ui/src/**` and `src-tauri/src/**` stay in the tree until Task 16; read the file a task names before porting it.

## Global Constraints

- Internal ID **`limitrings`** everywhere it persists: `%APPDATA%\limitrings\settings.json` (`limitrings-dev` for `LIMITRINGS_FAKE_SESSION=1`), `%LOCALAPPDATA%\limitrings\logs\limitrings.log` (1 MB, 3 files), Credential Manager service `limitrings`, Run value `limitrings`, mutex `Local\limitrings-single-instance`. Display name **"Claude Limit Checker"** is the window title, exe metadata and README only.
- Zero runtime dependencies: one `.exe`, CRT linked statically (`+crt-static`), no WebView2, Node, .NET or VC++ redistributable. Release artifact `ClaudeLimitChecker.exe` + `ClaudeLimitChecker-windows-x64.zip`. No installer.
- Budgets: idle private bytes **< 25 MB** (whole process), idle CPU **≈ 0 %**, download **< 6 MB**.
- `limits-core` behavior is unchanged except `Settings::default().start_with_windows == false` (Task 3). Additive helpers are allowed, with tests.
- Glass: fill **`rgba(22,22,28,.80)`**, 1 px border `rgba(255,255,255,.09)`, radius 16, top inner highlight `rgba(255,255,255,.06)` 1 px. No OS backdrop, no OS shadow (rule R11). Every effect (glow, sparks, pill, button shadow) is clipped to the widget's rounded box (rule R13).
- No continuous repaint when nothing moves. Only these repaint without input or state change: running animations, the Waiting spinner, the error countdown and an open tooltip (1 s), a pending position save (500 ms).
- Copy is exactly Phase 1's `ui/src/lib/i18n.ts` (EN and FR). Nothing blinks. No telemetry, never log cookie values, no Anthropic branding.
- Release builds ignore every `LIMITRINGS_*` variable; only release builds touch the Run value unless `LIMITRINGS_DEV_AUTOSTART=1` (rule R14).
- Windows-only code lives behind `cfg(windows)` in `app/src/platform/windows.rs`; `app/src/platform/mod.rs` exposes plain functions with a non-Windows stub so a Mac port adds a module.
- Snapshots and fixtures contain no real account data and no email: fixture orgs are `"Jane's Organization"` (plan max/ultra) and `"Acme Studio"` (team).
- Git: branch `rewrite/phase-2-egui`. After every commit, `git push origin rewrite/phase-2-egui` (`-u` the first time). Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never `--force`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass at the end of every task.

## Review Focus

1. **Glyphs missing from the embedded font.** "✓", "★", "⚠", "∞", "·", "…", "’" and French accents must not render as tofu boxes once egui's default fonts are dropped. Pinned by `fonts::tests::embedded_fonts_cover_every_ui_string` (Task 8).
2. **125 % / 150 % display scaling.** Logical-to-physical rounding in the window fit can clip the widget's bottom border or leave a 1 px gap. Pinned by `host::tests::fit_rounds_up_at_fractional_scale` (Task 15) and the `connected_ontrack_en@1.25x` / `@1.5x` snapshots (Task 9).
3. **Idle CPU creeping above zero.** A hover fade, a tween whose `done` never turns true, or a forgotten `request_repaint` keeps egui painting every frame. Pinned by `widget::tests::settles_to_no_repaint` (Task 13) and `anim::tween::tests::done_exactly_at_duration` (Task 6).
4. **State changes mid-animation.** Disconnect during a surprise reset, a second reset batch during a planned one, or a minimized window must never leave a locked ring, a gold glow, sparks or a pill over the connect panel. Pinned by `widget::tests::disconnect_mid_surprise_clears_effects`, `newer_batch_takes_over_ring` and `stalled_lock_released_by_safety` (Task 13).
5. **Exe path with spaces, moved after enabling Start with Windows.** The owner's own profile path has a space; the Run value must be quoted, and rewritten when the exe has moved. Pinned by `platform::autostart::tests::run_value_quotes_path_with_spaces` and `rewrites_when_exe_moved` (Task 14).

---

## File Structure

```
app/
  Cargo.toml                 package `limitrings`, lib `limitrings_app`, bin `limitrings`
  build.rs                   embeds assets/icon.ico + version info (winresource)
  assets/icon.ico            copied from src-tauri/icons/icon.ico
  assets/icon-64.png         copied from src-tauri/icons/64x64.png (window icon)
  assets/fonts/Inter-{Regular,SemiBold,Bold}.subset.ttf + OFL.txt
  examples/spike_*.rs        Task 2 measurements (deleted in Task 16)
  src/main.rs                windows_subsystem, calls limitrings_app::run()
  src/lib.rs                 run(): logging, single instance, engine, host
  src/shell.rs               resolve_language, initial_position, DevEnv, manages_autostart, settings_path, build_deps
  src/logging.rs             moved from src-tauri
  src/launch.rs              moved from src-tauri
  src/engine_thread.rs       tokio runtime thread + state forwarder
  src/host.rs                window creation, fit, move saver, applying Actions, quit
  src/platform/mod.rs        plain functions, non-Windows stubs
  src/platform/windows.rs    mutex, Run key, monitors, SetWindowPos, focus, ShellExecute, reduced motion
  src/ui/mod.rs              Lang, Prefs, FrameInput, FrameOutput, Action, Repaint
  src/ui/i18n.rs  format.rs  colors.rs  ring.rs  room.rs  menu.rs  fixtures.rs
  src/ui/fonts.rs            Inter install + text styles
  src/ui/draw.rs             glass, inner glow, arc mesh, gradient button, icons, flags, spinner
  src/ui/widget.rs           Widget state machine and layout
  src/ui/rings.rs  connect.rs  controls.rs  tooltip.rs  menu_view.rs  pill.rs  sparks.rs
  src/anim/mod.rs  easing.rs  tween.rs  track.rs  lock.rs  pill.rs  planned.rs  surprise.rs  entrance.rs
  tests/snapshots/*.png      egui_kittest snapshots (committed)
tools/screenshot-window.ps1  captures a real window by title to PNG (dev only)
docs/superpowers/phase1-reference/*.png   Phase 1 captures (Task 1)
.cargo/config.toml           +crt-static for x86_64-pc-windows-msvc
```

## Shared interfaces (defined in Task 5 and Task 9, used everywhere)

```rust
// app/src/ui/mod.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Lang { En, Fr }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Prefs { pub lang: Lang, pub always_on_top: bool, pub start_with_windows: bool }

pub struct FrameInput<'a> {
    pub state: &'a limits_core::AppState,
    pub prefs: &'a Prefs,
    pub now: chrono::DateTime<chrono::Utc>,   // wall clock (countdowns, "updated X ago")
    pub tz: chrono::FixedOffset,              // local offset for "14:00" / "Fri 23:59"
    pub t: f64,                               // monotonic animation clock, seconds
    pub reduced_motion: bool,                 // Windows "Show animations" off
    pub offset_y: f32,                        // points the widget sits below the window top (window grown upward)
    pub focused: bool,                        // window has focus (menu closes on blur)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Send(limits_core::Command),
    SetLang(Lang), SetAlwaysOnTop(bool), SetStartWithWindows(bool),
    OpenUrl(&'static str), Minimize, StartDrag, Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Repaint { Idle, After(std::time::Duration), Now }

pub struct FrameOutput {
    pub actions: Vec<Action>,
    pub size: egui::Vec2,   // widget box incl. border, points
    pub extra: f32,         // popup room below the widget, points (0 = rest)
    pub repaint: Repaint,
}
```

`Widget::new() -> Widget` and `Widget::show(&mut self, ctx: &egui::Context, input: &FrameInput) -> FrameOutput` (Task 9) is the only entry point the host uses.

---

### Task 1: Phase 1 reference captures and the window screenshot tool

**Files:**
- Create: `docs/superpowers/phase1-reference/*.png`, `docs/superpowers/phase1-reference/README.md`
- Create: `tools/screenshot-window.ps1`

These captures are what every later visual check compares against; they must exist before `ui/` is deleted.

- [ ] **Step 1: Run the Phase 1 UI in the browser mock.** `npm --prefix ui run dev` (background). Open `http://localhost:5173/?mock=<kind>` with the Playwright MCP tools at device scale factor 1 and 1.5.

- [ ] **Step 2: Capture each state at 1× and 1.5×.** Before each capture run `document.documentElement.classList.add('tauri')` so the glass is the shipped `.80` look. Clip to the `.wd` element (plus the popup for tooltip/menu captures). Kinds: `connected`, `idle`, `ultra`, `near`, `error`, `expired`, `waiting`, `disconnected`, `timedout`, `needsFirefox`, `twoOrgs`; in both `en` and `fr` (switch with the flag, or set `localStorage['mock.prefs']`). Also: tooltip open on each ring (`connected`, `idle`, `ultra`), menu open (`connected`, `twoOrgs`, `disconnected`), hover controls visible, and frames of `window.__mock.reset('planned')` and `reset('surprise')` at ≈ 300, 900, 1500, 2500 ms (`page.waitForTimeout`). Name files `<kind>_<lang>[_<extra>]@<scale>x.png`. Replace the email in the mock org name before capturing (edit nothing in `ui/`; use `window.__mock.push({org:{uuid:'p',name:"Jane's Organization",plan:'max'}, orgs:[...]})`).

- [ ] **Step 3: Write `tools/screenshot-window.ps1 -Title <string> -Out <path>`.** Finds the top-level window by exact title (`FindWindowW` via `Add-Type` P/Invoke), reads `GetWindowRect` (physical px, DPI-aware process: call `SetProcessDpiAwarenessContext(-4)` first), and saves a PNG with `System.Drawing.Graphics.CopyFromScreen`. Exit code 1 with a message when no window has that title.

- [ ] **Step 4: Verify the tool on a known window.** Run `powershell -File tools/screenshot-window.ps1 -Title "<title of any open window>" -Out $env:TEMP\t.png` and open the PNG with the Read tool. Expected: the window's pixels, not black.

- [ ] **Step 5: Commit** `docs/superpowers/phase1-reference`, `tools/screenshot-window.ps1` — `docs: Phase 1 reference captures and window screenshot tool`. Push.

---

### Task 2: Renderer spike and `app/` skeleton

**Files:**
- Modify: `Cargo.toml` (workspace members add `app`)
- Create: `app/Cargo.toml`, `app/src/main.rs`, `app/src/lib.rs`, `app/examples/spike_glow.rs`, `app/examples/spike_wgpu.rs`, `app/examples/spike_layered.rs`
- Modify: this plan's **Ledger** section

**Interfaces:**
- Produces: `app/` crate compiling with the chosen renderer's dependencies; `limitrings_app::run()` opening a borderless transparent 244×146 window with the glass and nothing else. The Ledger entry naming the renderer every later task uses.

- [ ] **Step 1: Write the three spike binaries.** Each opens a borderless, transparent, non-resizable, always-on-top 244×146 window (title "Claude Limit Checker spike") that draws the glass (Global Constraints values) and one ring (r 23, width 6, track `rgba(255,255,255,.08)`, arc `#34d399`) whose arc animates 0 → 62 % over 1.2 s, then stops requesting repaints.
  1. `spike_glow`: `eframe` with `default-features = false, features = ["glow"]`, `clear_color` all zeros.
  2. `spike_wgpu`: `eframe` with `features = ["wgpu"]`, `wgpu` backends limited to DX12.
  3. `spike_layered`: `winit` + `egui` (no eframe): `ctx.run` → `ctx.tessellate` → rasterize the meshes on the CPU into a premultiplied BGRA buffer (a small barycentric triangle rasterizer sampling the font texture, or tiny-skia), presented with `UpdateLayeredWindow` on a `WS_EX_LAYERED` window. `softbuffer` presents through GDI without per-pixel alpha on Win32, so it cannot give a transparent rounded window; note this in the Ledger if you confirm it.

- [ ] **Step 2: Measure each in release.** `cargo run --release -p limitrings --example spike_<x>`, wait 60 s, then in PowerShell: `(Get-Process spike_<x>).PrivateMemorySize64 / 1MB` and `(Get-Counter "\Process(spike_<x>)\% Processor Time" -SampleInterval 1 -MaxSamples 10).CounterSamples.CookedValue | Measure-Object -Average`. Screenshot each with `tools/screenshot-window.ps1` over a busy background and inspect the corners: no black or white rectangle, no fringe.

- [ ] **Step 3: Pick.** The first of glow → wgpu → layered that is < 25 MB and draws a clean transparent rounded window. If none meets 25 MB, the lightest that looks right. Write the numbers, screenshots' verdicts and the choice into the **Ledger** below.

- [ ] **Step 4: Build the skeleton with the chosen renderer.** `app/Cargo.toml`: `[package] name = "limitrings"`, `version = "0.2.0"`, `[lib] name = "limitrings_app"`, `[[bin]] name = "limitrings"`. `egui` and `eframe` with `default-features = false` (no default fonts, no accesskit, no persistence) plus only the renderer feature. `main.rs`: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` and `limitrings_app::run()`. `run()` opens the glass window (no ring).

- [ ] **Step 5: Verify.** `cargo build -p limitrings && cargo test --workspace` pass; `cargo run -p limitrings` shows the empty glass window.

- [ ] **Step 6: Commit** `feat(app): renderer spike and app skeleton (<renderer>)`. Push.

**If the layered host is chosen**, Task 15's host is the spike's winit loop promoted to `host.rs` instead of an `eframe::App`; everything else in the plan is unchanged because the UI only sees `egui::Context`.

---

### Task 3: Start with Windows off by default

**Files:**
- Modify: `crates/limits-core/src/store.rs:16-26` (Default), tests at `:345-352` and `:405-418`

- [ ] **Step 1: Update the tests first.** In `settings_defaults_when_missing`: `assert!(s.always_on_top && !s.start_with_windows);`. Rename `settings_partial_file_keeps_true_defaults` to `settings_partial_file_keeps_defaults` and assert `!s.start_with_windows`. Add:

```rust
#[test]
fn existing_file_keeps_start_with_windows_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"start_with_windows":true}"#).unwrap();
    assert!(SettingsStore::new(path).load().start_with_windows);
}
```

- [ ] **Step 2: Run** `cargo test -p limits-core settings_` — expected: the two default tests FAIL.
- [ ] **Step 3: Set `start_with_windows: false` in `impl Default for Settings`.**
- [ ] **Step 4: Run** `cargo test -p limits-core` — all pass.
- [ ] **Step 5: Commit** `feat(core): Start with Windows is off by default`. Push.

---

### Task 4: Port the shell logic

**Files:**
- Create: `app/src/shell.rs`, `app/src/logging.rs`, `app/src/launch.rs`
- Modify: `app/Cargo.toml` (deps: `limits-core`, `tokio`, `url`, `anyhow`, `tracing`, `tracing-subscriber`, `file-rotate`, `winreg`, `sys-locale`, `serde`; dev `tempfile`)

**Interfaces:**
- Produces (in `shell.rs`): `resolve_language(saved: Option<&str>, locale: Option<&str>) -> Lang`, `prefs_from_settings(s: &Settings, locale: Option<&str>) -> Prefs`, `initial_position(saved: Option<(i32,i32)>, size: (i32,i32), monitors: &[Rect], primary: Rect) -> (i32,i32)`, `DevEnv { base_url, fake_session, autoconnect, no_firefox, autostart }` with `DevEnv::from_vars(get)` and `DevEnv::current()`, `manages_autostart(debug: bool, dev: &DevEnv) -> bool`, `settings_path(appdata: &Path, dev: bool) -> PathBuf`, `build_deps(dev: &DevEnv, settings: SettingsStore) -> EngineDeps` (with the debug-only `FakeLauncher`). `logging::init() -> Option<PathBuf>`. `launch::FirefoxLauncher { pretend_missing }`, `launch::firefox_exe()`.
- `Lang` and `Prefs` are defined here in `app/src/ui/mod.rs` (create the file with just these two types; Task 5 adds the rest).

- [ ] **Step 1: Move the tests.** Copy every test from `src-tauri/src/lib.rs:512-647`, `launch.rs` and `logging.rs` except the two `command_dto_*` tests (the DTO is gone: the UI sends `limits_core::Command` directly). `language_saved_then_locale_then_en` compares against `Lang::Fr`/`Lang::En`. Replace `prefs_serialize_camel_case` with:

```rust
#[test]
fn prefs_from_settings_resolves_language() {
    let s = Settings { always_on_top: false, ..Settings::default() };
    assert_eq!(prefs_from_settings(&s, Some("fr-FR")),
               Prefs { lang: Lang::Fr, always_on_top: false, start_with_windows: false });
}
```

- [ ] **Step 2: Run** `cargo test -p limitrings shell` — expected: FAIL to compile (functions missing).
- [ ] **Step 3: Port the functions** from `src-tauri/src/lib.rs:57-195` and move `launch.rs`/`logging.rs` unchanged except the log filter, which names `limitrings_app` instead of `limitrings_lib`.
- [ ] **Step 4: Run** `cargo test -p limitrings` — all pass.
- [ ] **Step 5: Commit** `feat(app): port shell logic, logging and Firefox launcher`. Push.

---

### Task 5: Pure UI logic (i18n, format, colors, ring text, room, menu model, fixtures)

**Files:**
- Create: `app/src/ui/i18n.rs`, `format.rs`, `colors.rs`, `ring.rs`, `room.rs`, `menu.rs`, `fixtures.rs`; extend `app/src/ui/mod.rs` with the Shared interfaces above.

**Interfaces:**
- Produces:
  - `i18n::Strings` (a struct of `&'static str` fields plus `fn` fields for parametrized strings, one `static EN` and `static FR`), `i18n::t(lang) -> &'static Strings`. Field names are the TS keys in snake_case (`s, w, idle, resets, resets_day, resets_in, limit, used, expected, ahead, room, on_track, resets_long, no_session, starts_next, ago, plan_max … m_quit`). Parametrized ones are `fn(&str) -> String` or `fn(i64) -> String`.
  - `format::{fmt_time, fmt_day, fmt_dur, reset_line}`: `fmt_time<Tz: TimeZone>(d: &DateTime<Tz>) -> String`, `fmt_day<Tz>(d, lang) -> String`, `fmt_dur(ms: i64, lang) -> String`, `reset_line(key: LimitKey, resets_at: Option<DateTime<Utc>>, now: DateTime<Utc>, tz: FixedOffset, lang) -> String`.
  - `colors`: `Color32` consts `GREEN #34d399`, `RED #fb7185`, `BLUE #7dd3fc`, `IDLE #5f5f69`, `GOLD #facc15`, `INK #e7e7ea`, `MUTE #8b8b95`, `pace_color(Pace) -> Color32`, plus every color named in Tasks 8-12.
  - `ring`: `CIRC: f32 = 2π·23`, `arc_fraction(used) -> f32` (0..1 filled, clamped; replaces `arcOffset`), `tick_angle_deg(expected) -> f32`, `tick_opacity(expected) -> f32`, `ring_number(used) -> i64`, `num_fill(&RingView) -> Color32`, `arc_stroke(&RingView) -> Color32` (no-limit `rgba(255,255,255,.18)`), `is_near_limit(Option<&[RingView;2]>) -> bool`, `ago_text(last_ok, now, lang) -> Option<String>`, `err_line(retry_at, now, lang) -> String`, `tip_title(org, orgs, lang) -> String`, `tip_model(&RingView, now, tz, lang) -> TipModel` with `enum TipModel { NoLimit{title,line}, Idle{title,line,sub}, Active{title, usage, pace: Option<(String, Color32)>, resets: Option<String>} }`.
  - `room::{tip_room(trigger_bottom: f32, tip_h: f32, widget_h: f32) -> f32, menu_room(cursor_y, menu_h, widget_h) -> f32, RingRooms (two Option<f32> slots), with_ring_room, max_room}`, `PAD = 6.0`, results `ceil`ed.
  - `menu::connected_items(&Conn) -> bool`.
  - `fixtures::scenario(kind: &str, now: DateTime<Utc>) -> AppState` porting `ui/src/lib/mockBridge.ts:53-124` (kinds: connected, waiting, disconnected, timedout, needsFirefox, expired, error, near, idle, ultra, twoOrgs) with the fixture org names from Global Constraints; `fixtures::reset_state(base: &AppState, kind: &str, seq: u64) -> AppState` for planned / surprise / both / bothPlanned / plannedWeekly / surpriseWeekly (port `mockBridge.ts` `reset`). `pub` but `#[cfg(test)]`-only module.

- [ ] **Step 1: Port the tests.** Translate every case in `ui/src/lib/__tests__/{i18n,format,ring,room,menu}.test.ts` into `#[cfg(test)] mod tests` in the matching file, keeping the exact expected strings. Local times in the TS tests (`new Date(2026, 10, 3, 11, 30)`) become `FixedOffset::east_opt(3600)` (CET, November) datetimes. The i18n "identical key sets" test becomes a `strings_non_empty_and_chrome_only_in_need_firefox_text` test over both tables (the struct guarantees identical keys). Add:

```rust
#[test]
fn no_limit_tip_both_langs() {
    let nl = RingView { key: LimitKey::Session, used: 0.0, resets_at: None, expected: None, pace: Pace::Idle, no_limit: true };
    assert_eq!(tip_model(&nl, now(), cet(), Lang::Fr),
        TipModel::NoLimit { title: "Session 5 h".into(), line: "Pas de limite de 5 h sur le forfait Ultra".into() });
}
#[test]
fn fmt_dur_rounds_before_splitting() { assert_eq!(fmt_dur((119 * 60 + 40) * 1000, Lang::En), "2h00"); }
```

- [ ] **Step 2: Run** `cargo test -p limitrings ui::` — FAIL (missing items).
- [ ] **Step 3: Implement** by porting `ui/src/lib/{i18n,format,ring,room,menu,colors}.ts` line by line. FR `resets_long` keeps the "à " only before a digit. `tip_title` keeps the `'s Organization$` / `’s Organization$` rule and returns the org name when the plan is `None`.
- [ ] **Step 4: Run** `cargo test -p limitrings` — all pass.
- [ ] **Step 5: Commit** `feat(app): port UI text, formatting and ring logic with Phase 1 tests`. Push.

---

### Task 6: Animation math (easing, tween, keyframe track, locks, pill text)

**Files:**
- Create: `app/src/anim/mod.rs`, `easing.rs`, `tween.rs`, `track.rs`, `lock.rs`, `pill.rs`

**Interfaces:**
- Produces:
  - `easing::Ease` enum `{ Linear, CubicBezier(f32,f32,f32,f32), Spring }` with `fn apply(self, t: f32) -> f32`; consts `EASE_DEFAULT = CubicBezier(.2,.8,.2,1)`, `EASE_CSS = CubicBezier(.25,.1,.25,1)` (CSS `ease`), `EASE_IN_OUT = CubicBezier(.42,0,.58,1)`, `EASE_OUT = CubicBezier(0,0,.58,1)`, `EASE_RING = CubicBezier(.2,.9,.25,1.05)`; `LinearEasing::parse(spec: &str) -> LinearEasing` with `fn at(&self, t: f32) -> f32`; `SPRING: &str` (the exact string from `ui/src/lib/easing.ts:2-3`), `spring(t) -> f32`. Cubic-bezier is solved for x by Newton iterations with a bisection fallback (y may overshoot 1).
  - `tween::Tween<T: Lerp>`: `new(v: T)`, `retarget(&mut self, now: f64, to: T, dur: f64, ease: Ease)` (starts from the current *displayed* value), `snap(&mut self, v)`, `value(&self, now) -> T`, `done(&self, now) -> bool` (true when `now >= start + dur`). `Lerp` for `f32` and `Color32` (linear in sRGB-unmultiplied, like CSS).
  - `track::Track { dur_ms: f64, delay_ms: f64, keys: Vec<(f32 /*time 0..1*/, f32 /*value*/)>, ease: Ease }` with `sample(&self, elapsed_ms: f64) -> f32` (before start → first value; after end → last; the ease applies per segment, motion's semantics). `Track::new(dur, &[values], times: Option<&[f32]>, ease)`; evenly spaced when `times` is `None`.
  - `lock::{Locks, lock_keys, release}` — `Locks` is `[Option<u64>; 2]` indexed by `LimitKey` (session 0, weekly 1).
  - `pill::{PillKind {Calm, Gold}, PillSpec {kind, text, hold_ms}, pill_for(&[ResetEvent], Lang) -> Option<PillSpec>, PILL_FONT = 10.0, PILL_FONT_MIN = 8.5, pill_font_size(width, avail, chrome) -> Option<f32>}`.

- [ ] **Step 1: Port the tests** of `ui/src/lib/__tests__/easing.test.ts`, `ui/src/anim/__tests__/{lock,pill,pillFont}.test.ts` with the same values, and add:

```rust
#[test] fn cubic_bezier_endpoints_and_known_point() {
    assert_eq!(EASE_CSS.apply(0.0), 0.0); assert_eq!(EASE_CSS.apply(1.0), 1.0);
    assert!((EASE_CSS.apply(0.5) - 0.8024).abs() < 1e-3);
    assert!(EASE_RING.apply(0.8) > 1.0); // overshoots
}
#[test] fn done_exactly_at_duration() {
    let mut tw = Tween::new(0.0f32); tw.retarget(10.0, 1.0, 0.8, EASE_RING);
    assert!(!tw.done(10.79)); assert!(tw.done(10.8)); assert_eq!(tw.value(11.0), 1.0);
}
#[test] fn retarget_mid_flight_starts_from_shown_value() {
    let mut tw = Tween::new(0.0f32); tw.retarget(0.0, 1.0, 1.0, Ease::Linear);
    tw.retarget(0.5, 0.0, 1.0, Ease::Linear);
    assert!((tw.value(0.5) - 0.5).abs() < 1e-6);
}
#[test] fn track_samples_per_segment_with_times() {
    let tr = Track::new(4200.0, &[0.0, 1.0, 1.0, 0.0], Some(&[0.0, 0.12, 0.8, 1.0]), Ease::Linear);
    assert_eq!(tr.sample(-5.0), 0.0); assert!((tr.sample(252.0) - 0.5).abs() < 1e-3);
    assert_eq!(tr.sample(2000.0), 1.0); assert_eq!(tr.sample(9999.0), 0.0);
}
```

- [ ] **Step 2: Run** `cargo test -p limitrings anim::` — FAIL.
- [ ] **Step 3: Implement.** Port `linearEasing` from `ui/src/lib/easing.ts:6-40` exactly (same position fill rules).
- [ ] **Step 4: Run** `cargo test -p limitrings` — pass.
- [ ] **Step 5: Commit** `feat(app): easing, tweens, keyframe tracks, locks and pill text`. Push.

---

### Task 7: Reset, entrance and button timelines as pure functions

**Files:**
- Create: `app/src/anim/planned.rs`, `surprise.rs`, `entrance.rs`; extend `anim/mod.rs`

**Interfaces:**
- Consumes: `Track`, `Ease`, `spring`, `ring::CIRC`, colors (Task 5-6).
- Produces:

```rust
/// What a ring looks like at one instant of a reset animation (overrides the live view).
#[derive(Clone, Debug, PartialEq)]
pub struct RingFrame {
    pub arc_fraction: f32,        // 0..1 drawn arc
    pub arc_color: Color32,
    pub arc_alpha: f32,           // 0..1
    pub arc_flash: f32,           // 0..1 gold brightness + glow strength (surprise)
    pub glint: Option<(f32 /*position along arc 0..1*/, f32 /*alpha*/)>,
    pub tick_alpha: f32,
    pub scale: f32,               // whole ring
    pub num: i64, pub num_color: Color32,
    pub num_alpha: f32, pub num_dy: f32, pub num_scale: f32, pub num_blur: f32, // blur 0..6 px
    pub ok_alpha: f32, pub ok_scale: f32, // the ✓ after the name
}
pub fn planned_frame(ms: f64, from_used: f64, live_num: i64, live_color: Color32) -> RingFrame;
pub fn surprise_frame(ms: f64, from_used: f64, live_num: i64) -> RingFrame;
pub fn crossfade_frame(ms: f64, live_num: i64, live_color: Color32) -> RingFrame; // reduced motion
pub const PLANNED_PILL_AT: f64 = 1450.0; pub const PLANNED_END: f64 = 2150.0;
pub const SURPRISE_PILL_AT: f64 = 640.0; pub const SURPRISE_END: f64 = 3040.0;
pub const CROSSFADE_PILL_AT: f64 = 200.0; pub const CROSSFADE_END: f64 = 500.0;
pub const OK_END: f64 = 950.0 + 3200.0; // ✓ keeps its own timeline after the ring unlocks
pub fn glow_alpha(ms: f64) -> f32;                           // 4200 ms, [0,1,1,0] @ [0,.12,.8,1], EASE_DEFAULT
pub struct PillFrame { pub alpha: f32, pub dy: f32, pub scale: f32 }
pub fn pill_frame(ms: f64, hold_ms: f64, reduced: bool) -> Option<PillFrame>; // None once gone
pub struct Spark { pub x: f32, pub y: f32, pub dx: f32, pub dy: f32, pub dur_ms: f32, pub color: Color32 }
pub fn sparks(seed: u64, center: Pos2, radius: f32, arc_fraction: f32, n: usize) -> Vec<Spark>;
pub fn spark_at(s: &Spark, ms: f64) -> Option<(Pos2, f32 /*scale*/, f32 /*alpha*/)>;
pub fn entrance_fraction(ms: f64, index: usize, target: f32) -> f32; // 1200 ms, 90 ms stagger, EASE_RING
pub fn refresh_press(ms: f64) -> (f32 /*button scale*/, f32 /*icon turns 0..1*/); // 500 / 900 ms, spring
pub const REFRESH_SEND_AT: f64 = 350.0; pub const REFRESH_END: f64 = 900.0;
```

Timelines (ms from the event; port of `ui/src/anim/planned.ts`, `surprise.ts`, `components/Pill.tsx`, `Sparks.tsx`, `HoverControls.tsx`, `Ring.tsx` entrance):
- **Planned:** glint 0-750 runs from 0 to `from` along the arc, alpha `[0,1,1,0]` @ `[0,.15,.85,1]`, `EASE_IN_OUT`. 750-1200 arc `from → 0`, `CubicBezier(.6,0,.4,1)`. Tick alpha → 0 over 300 ms from 750 (`EASE_CSS`). Scale `[1,.9,1]` @ `[0,.35,1]` over 750-1550, spring. Number 750-950 `dy 0→-8`, alpha `1→0`, `EASE_DEFAULT`; at 950 number `0`, number and arc color `GREEN`; 950-1450 `dy 8→0`, alpha `0→1`, spring. ✓ from 950 for 3200 ms: alpha `[0,1,1,0]` @ `[0,.45,.9,1]`, scale `.3→1` over the first 45 %, `EASE_OUT`.
- **Surprise:** at 0 arc and number `GOLD`; `arc_flash` 0-420 `[0,1,.25]` (brightness 1 → 1.9 → 1.2), stays .25 until the end. 380-760 arc `from → 0`, alpha `1 → .2`, `CubicBezier(.4,0,.2,1)`; tick alpha → 0 at 380. Number 380-640 blur `0→6`, alpha `1→0`; at 640 number `0`; 640-1340 blur `6→0`, alpha `0→1`, `num_scale 1.4→1`, spring. Sparks burst at 380.
- **Crossfade:** arc 0 and tick hidden at once; number alpha `1→0` over 200, then `0`, `0→1` over 300.
- **Pill:** in 650 ms spring (`alpha 0→1`, `dy 6→0`, `scale .85→1`), hold `hold_ms`, out 400 ms `EASE_DEFAULT` (`alpha 1→0`, `dy 0→-4`, `scale 1→.96`). Reduced motion: 300 ms fade in, 400 ms fade out.
- **Sparks:** 30, colors cycle `#facc15 #fde68a #fb923c #fff`; start angle `-π/2 + r·2π·arc_fraction` on the circle; distance `24 + r·50`; duration `700 + r·600`; `CubicBezier(.1,.7,.3,1)`; translate to the distance, scale `1→.2`, alpha `1→0`. `r` from a xorshift64 seeded with `seed` (deterministic, no `rand` dependency). `arc_fraction = max(.15, from_used/100)`.
- **Refresh:** button scale `pressScale(spring(p))` over 500 ms with `pressScale(p) = p<.25 ? 1-.14·p/.25 : .86+.14·(p-.25)/.75`; icon one turn over 900 ms, spring.

- [ ] **Step 1: Write the tests** (one module per file), e.g.:

```rust
#[test] fn planned_key_moments() {
    let f = planned_frame(0.0, 62.0, 40, BLUE);
    assert_eq!((f.num, f.arc_fraction), (40, 0.62)); assert_eq!(f.glint.unwrap().1, 0.0);
    let f = planned_frame(1300.0, 62.0, 40, BLUE);
    assert_eq!((f.num, f.num_color, f.arc_fraction), (0, GREEN, 0.0)); assert_eq!(f.tick_alpha, 0.0);
    assert!(planned_frame(2000.0, 62.0, 40, BLUE).ok_alpha > 0.8);
}
#[test] fn surprise_number_is_zero_after_640() { assert_eq!(surprise_frame(639.0, 62.0, 7).num, 7); assert_eq!(surprise_frame(640.0, 62.0, 7).num, 0); }
#[test] fn sparks_are_deterministic_and_start_on_circle() {
    let a = sparks(9, pos2(60.0, 56.0), 23.0, 0.62, 30); assert_eq!(a, sparks(9, pos2(60.0, 56.0), 23.0, 0.62, 30));
    for s in &a { assert!(((s.x - 60.0).hypot(s.y - 56.0) - 23.0).abs() < 1e-3); }
}
#[test] fn pill_gone_after_hold() { assert!(pill_frame(650.0 + 2200.0 + 401.0, 2200.0, false).is_none()); }
#[test] fn entrance_staggers() { assert_eq!(entrance_fraction(90.0, 1, 0.34), 0.0); assert_eq!(entrance_fraction(1290.0, 1, 0.34), 0.34); }
```

- [ ] **Step 2: Run** `cargo test -p limitrings anim::` — FAIL.
- [ ] **Step 3: Implement** the timelines with `Track`s.
- [ ] **Step 4: Run** `cargo test -p limitrings` — pass.
- [ ] **Step 5: Commit** `feat(app): reset, pill, sparks, entrance and refresh timelines`. Push.

---

### Task 8: Fonts and drawing primitives

**Files:**
- Create: `app/assets/fonts/Inter-Regular.subset.ttf`, `Inter-SemiBold.subset.ttf`, `Inter-Bold.subset.ttf`, `OFL.txt`; `app/src/ui/fonts.rs`, `app/src/ui/draw.rs`
- Modify: `app/Cargo.toml` (dev: `ttf-parser`, `egui_kittest = { version = "0.37", features = ["wgpu", "snapshot"] }`)

**Interfaces:**
- Produces:
  - `fonts::install(ctx: &egui::Context)`: registers families `"inter"` (Regular), `"inter-semibold"`, `"inter-bold"`, sets `Proportional` to `inter`, and nothing else (egui's `default_fonts` feature is off). `fonts::regular(size) / semibold(size) / bold(size) -> FontId`. Weight map from the CSS: 400 → regular, 600/650 → semibold, 700/750/800 → bold.
  - `draw::glass(p: &Painter, rect: Rect, warn: bool)`: fill, border, 1 px top highlight; with `warn` the border is `rgba(251,113,133,.55)` and an inner glow of `rgba(251,113,133,.22)`, 22 px.
  - `draw::inner_glow(p, rect, radius: f32, color: Color32, width: f32)`: a band inside the rounded rect, alpha falling off like a Gaussian (σ = width/2) from the edge, built as a mesh of concentric rounded-rect rings (≥ 12 steps).
  - `draw::arc_mesh(center: Pos2, r: f32, width: f32, from_turn: f32, to_turn: f32, color: Color32, px: f32 /*one physical pixel in points*/) -> Mesh`: round-capped arc starting at 12 o'clock clockwise. A triangle strip between `r ± width/2`, a half-disc fan at each end, and a feather of `px` fading to transparent on every outer edge. Segment count from the arc length (≥ 1 segment per 2 px). Empty mesh when `to_turn - from_turn <= 0`.
  - `draw::gradient_button(p, rect, radius, top: Color32, bottom: Color32)` (vertex-colored rounded rect) and `draw::soft_shadow(p, rect, radius, offset_y, blur, color)`.
  - `draw::icon(p, kind: Icon, rect, color, stroke: f32)` with `Icon::{RefreshCw, Minus, X}` from Lucide (24-unit viewBox, stroke 2, round caps/joins): Minus `M5 12h14`; X `M18 6 6 18`, `m6 6 12 12`; RefreshCw `M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8`, `M21 3v5h-5`, `M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16`, `M8 16H3v5`. Arcs flattened to polylines, rotation parameter for the spin: `icon_rotated(p, kind, rect, color, stroke, turns: f32)`.
  - `draw::flag_fr(p, rect)` and `draw::flag_uk(p, rect)` ported from `ui/src/components/Flags.tsx` (UK: blue field, white diagonals 6, red diagonals 4 clipped to the counter-changed triangles, white cross 10, red cross 6, in a 60×30 box).
  - `draw::spinner(p, center, t: f64, reduced: bool)`: 12 px, 2 px ring `rgba(255,255,255,.2)`, white top quarter, one turn per 0.8 s (static when reduced).

- [ ] **Step 1: Build the subset fonts.** Download Inter 4.1 static TTFs (`https://github.com/rsms/inter/releases`, `Inter-Regular.ttf`, `Inter-SemiBold.ttf`, `Inter-Bold.ttf`) into the scratchpad, then for each: `pyftsubset <in> --unicodes="U+0020-007E,U+00A0-00FF,U+0152-0153,U+2018-201F,U+2026,U+00B7,U+221E,U+2713,U+2605,U+26A0,U+2192" --layout-features='kern,liga,calt,tnum' --output-file=app/assets/fonts/<name>.subset.ttf`. Copy Inter's `LICENSE.txt` as `OFL.txt`.

- [ ] **Step 2: Write the coverage test** in `fonts.rs`:

```rust
#[test]
fn embedded_fonts_cover_every_ui_string() {
    // Every char of every EN/FR string (call fn fields with sample args) plus the ring symbols.
    let mut chars: BTreeSet<char> = all_ui_text().chars().collect();
    chars.extend(['∞', '✓', '★', '⚠', '·', '…', '’', '%']);
    for (name, bytes) in FONTS {
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        let missing: Vec<_> = chars.iter().filter(|c| !c.is_whitespace() && face.glyph_index(**c).is_none()).collect();
        assert!(missing.is_empty(), "{name} lacks {missing:?}");
    }
}
```

`all_ui_text()` concatenates every field of `i18n::EN` and `FR`. If Inter lacks a symbol (check ✓ ★ ⚠ first), draw that symbol as a vector path in `draw.rs` instead and exclude it from the set with a comment naming the path function.

- [ ] **Step 3: Write the primitive snapshot test** `draw::tests::primitives_sheet`: a 260×120 kittest harness at `pixels_per_point = 1.0` drawing, on a `#2a2a33` background, an arc at 0, 3, 62 and 100 % (green), the no-limit grey arc, the three icons, both flags, a warn glass and the gradient button. `harness.snapshot("primitives_sheet")`. Also `arc_mesh_empty_at_zero` (`arc_mesh(..., 0.0, 0.0, ...)` has no vertices) and `arc_mesh_has_no_nan`.

- [ ] **Step 4: Run** `UPDATE_SNAPSHOTS=1 cargo test -p limitrings draw::` to create the PNG, then open `app/tests/snapshots/primitives_sheet.png` with the Read tool and compare with `docs/superpowers/phase1-reference/connected_en@1x.png`: smooth arcs with round caps, no seams where the caps meet the strip, crisp 1 px border. Fix and re-run until it matches. Then `cargo test -p limitrings` passes without `UPDATE_SNAPSHOTS`.

- [ ] **Step 5: Commit** `feat(app): embedded Inter subset and drawing primitives`. Push.

---

### Task 9: Widget shell and the rings view

**Files:**
- Create: `app/src/ui/widget.rs`, `app/src/ui/rings.rs`; snapshot PNGs

**Interfaces:**
- Consumes: Tasks 5-8.
- Produces: `Widget::new()`, `Widget::show(ctx, &FrameInput) -> FrameOutput` (Shared interfaces). In this task `show` paints the glass and, when `conn == Connected && rings.is_some()`, the rings view; other states paint an empty glass of the connect-panel width (Task 10 fills them). `FrameOutput.size` is the widget box; `extra` is 0; actions empty. Test hook: `#[cfg(test)] pub fn debug_rects(&self) -> &BTreeMap<&'static str, egui::Rect>`, filled during `show` with the rect of every clickable part (`"connect_button"`, `"link_0"`, `"link_1"`, `"ctl_min"`, `"ctl_close"`, `"refresh"`, `"flag_fr"`, `"flag_en"`, `"ring_session"`, `"ring_weekly"`, `"menu_item_<label>"`); Tasks 10-12 click through it.

Layout (CSS px = egui points, from `ui/src/index.css` `.wd`): border 1; rings row padding 24 top, 14 sides, 22 bottom, gap 6, ring column 104 wide → box width **244**. Ring SVG 64×64 centered in the column, 6 below it the name (11 px semibold `#c9c9d1`), 2 below that the reset line (10.5 px regular `INK`). Box height is the laid-out content (≈ 146 at 1×; take it from the layout, never hard-code). Ring: track `rgba(255,255,255,.08)` width 6 r 23 centered at (32,32) of the SVG box; arc per `arc_stroke`; tick from (32,3) to (32,12), white, 2 px, round caps, rotated by `tick_angle_deg` about (32,32), alpha `tick_opacity × .95`; number `ring_number` bold 13.5 px with a "%" bold 9 px 1 px after it, baseline y 36.5, centered as a whole; no-limit: "∞" bold 24 px baseline y 39, no "%". Name = `no_limit ? no_limit label : s / w`; reset line = `no_limit ? "" : reset_line(..)`. **Error** (`state.error.is_some()`): the rings row is drawn in grayscale (luma of each color) at 50 % alpha, and a foot line `err_line` (10.5 px `#fda4af`, centered, padding 0 12 9) is added under the rings. **Near limit** (`is_near_limit` and no error): `draw::glass(.., warn = true)`. Live values tween as CSS transitions did: arc fraction and tick angle 0.8 s `EASE_RING`, arc and number colors 0.4 s `EASE_CSS`, tick alpha 0.3 s `EASE_CSS`; the number text changes at once. The cursor is `Grab` over the glass.

- [ ] **Step 1: Write the snapshot tests** in `widget.rs` `#[cfg(test)] mod snapshot_tests`, with a helper `fn render(kind: &str, lang: Lang, ppp: f32) -> Harness` that installs fonts, builds `fixtures::scenario(kind, NOW)` with `NOW = 2026-11-03T10:30:00Z`, `tz = +01:00`, `t` far past every animation (so tweens are settled and entrances done), and sizes the harness to the returned `size`. Snapshots: `connected_{en,fr}` (ahead + room), `connected_ontrack_{en,fr}`, `idle_{en,fr}`, `ultra_{en,fr}`, `near_{en,fr}`, `error_{en,fr}`, `connected_ontrack_en@1.25x`, `connected_ontrack_en@1.5x`. Plus unit tests:

```rust
#[test] fn rings_box_is_244_wide() { assert_eq!(show_size("connected").x, 244.0); }
#[test] fn error_adds_the_foot_line() { assert!(show_size("error").y > show_size("connected").y + 15.0); }
#[test] fn no_actions_when_idle() { assert!(show("connected").actions.is_empty()); }
```

The `connected` fixture uses session ahead (62 / 48), weekly room (34 / 45); add a `connected` variant with session on track for `connected_ontrack_*` (used 50, expected 48).

- [ ] **Step 2: Run** `cargo test -p limitrings widget::` — FAIL.
- [ ] **Step 3: Implement** `widget.rs` (state struct holding tweens per ring, layout, glass, clip rect = rounded widget box) and `rings.rs` (one ring column; takes an optional `RingFrame` override that Task 13 will feed).
- [ ] **Step 4: Create and inspect the snapshots.** `UPDATE_SNAPSHOTS=1 cargo test -p limitrings widget::`; open each new PNG with Read next to the matching `phase1-reference` capture. Match spacing, sizes, colors and text by eye; differences in font hinting are fine, layout shifts of ≥ 2 px are not. Then run without `UPDATE_SNAPSHOTS`: pass.
- [ ] **Step 5: Commit** `feat(app): widget layout and rings view with snapshots`. Push.

---

### Task 10: Connect panels

**Files:**
- Create: `app/src/ui/connect.rs`; snapshot PNGs

**Interfaces:**
- Consumes: `Widget` (Task 9).
- Produces: the panel for every non-ring state, emitting `Action::Send(Command::Connect | CancelConnect)` and `Action::OpenUrl("https://www.mozilla.org/firefox/")`.

Layout from `index.css` `.st`, `.btn-c`, `.lnk`, `.spin` and `ui/src/components/ConnectPanel.tsx`: content width 242 (box 244), padding 26 top, 18 sides, 14 bottom; title 13 px bold `INK`, 3 below; body 11 px regular `MUTE`, line height 1.45, wrapping, 10 below; error body color `#fda4af`. Button: full width, 9 px vertical padding, radius 10, gradient `#f3c3a9 → #d97757`, label 12.5 px semibold `#1a1a1a`, shadow `0 6px 18px rgba(217,119,87,.35)` (clipped by the widget), hover brightness ×1.06. Waiting: spinner + 6 px + title on one line, links row (11 px, `#f0b38f`, underlined 2 px below the baseline) with "again" left and "cancel" right. Connected with no rings: empty when `error` is none, else just the error line as body. Disconnected uses `timed_out ? timed_out : c_text`.

- [ ] **Step 1: Write the snapshot tests** `disconnected_{en,fr}`, `timedout_en`, `waiting_{en,fr}` (spinner at `t = 0`), `needs_firefox_{en,fr}`, `expired_{en,fr}`, `connected_no_rings_error_en`, and click tests with kittest (`harness.get_by_label(..)` needs accesskit; instead click by position: `harness.input_mut().events.push(PointerButton{..})` at the button's rect returned by a test hook `Widget::debug_rects()`):

```rust
#[test] fn connect_button_sends_connect() { assert_eq!(click_button("disconnected"), vec![Action::Send(Command::Connect)]); }
#[test] fn waiting_links() { assert_eq!(click_link("waiting", 1), vec![Action::Send(Command::CancelConnect)]); }
#[test] fn get_firefox_opens_download_page() { assert_eq!(click_button("needsFirefox"), vec![Action::OpenUrl("https://www.mozilla.org/firefox/")]); }
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement** `connect.rs`.
- [ ] **Step 4: Create, inspect against `phase1-reference/{disconnected,timedout,waiting,needsFirefox,expired}_*`, re-run** — pass.
- [ ] **Step 5: Commit** `feat(app): connect, waiting, Firefox-needed and expired panels`. Push.

---

### Task 11: Hover controls, flags and dragging

**Files:**
- Create: `app/src/ui/controls.rs`; snapshot PNGs

**Interfaces:**
- Produces: `Action::Minimize`, `Action::Quit`, `Action::SetLang(_)`, `Action::Send(Command::Refresh)` (sent `REFRESH_SEND_AT` ms after the click, clicks ignored until `REFRESH_END`), `Action::StartDrag` on a primary press anywhere on the glass that is not a control, flag, button, link or menu (never on right-click).

From `index.css` `.ctl .ib .rf .flags`: controls fade in over 0.2 s while the pointer is inside the window and out when it leaves (`ctx.animate_bool_with_time`). `.ctl` top 4 right 5, two buttons 22×18 radius 5 with gap 2; icon 12 px stroke 2 `MUTE`; hover bg `rgba(255,255,255,.09)` icon white; close hover bg `#e81123`. Refresh button bottom 5 right 5, only when rings are shown. Flags top 6 left 8, gap 5, 16×11 radius 2: inactive alpha .4 and outline 1 px `rgba(255,255,255,.15)`, hover alpha .8 and scale 1.1 (0.15 s), active alpha 1 and outline 1.5 px `rgba(255,255,255,.75)`. Clicking the active flag does nothing. No hover text on these buttons (Phase 1 had native `title` tooltips; an egui tooltip would need window growth, so it is dropped).

- [ ] **Step 1: Write tests:** snapshots `hover_controls_connected_en` (pointer inside, `t` past the fade) and `hover_controls_disconnected_fr` (no refresh button); and

```rust
#[test] fn close_quits() { assert_eq!(click_ctl("connected", Ctl::Close), vec![Action::Quit]); }
#[test] fn refresh_sends_after_350ms_once() {
    let (at_0, at_400, again_at_500) = refresh_sequence("connected");
    assert!(at_0.is_empty()); assert_eq!(at_400, vec![Action::Send(Command::Refresh)]); assert!(again_at_500.is_empty());
}
#[test] fn press_on_glass_starts_drag_but_not_on_flags() { assert_eq!(press_at("connected", RINGS_CENTER), vec![Action::StartDrag]); assert_eq!(press_at("connected", FLAG_FR), vec![Action::SetLang(Lang::Fr)]); }
#[test] fn right_click_never_drags() { assert!(!right_press_at("connected", RINGS_CENTER).contains(&Action::StartDrag)); }
```

- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4:** create snapshots, inspect against `phase1-reference/*_hover*`, re-run — pass.
- [ ] **Step 5: Commit** `feat(app): hover controls, flags, refresh spin and drag`. Push.

---

### Task 12: Ring tooltip and right-click menu

**Files:**
- Create: `app/src/ui/tooltip.rs`, `app/src/ui/menu_view.rs`; snapshot PNGs

**Interfaces:**
- Consumes: `room::*`, `ring::tip_model`, `tip_title`, `ago_text`, `menu::connected_items`.
- Produces: `FrameOutput.extra` = `max(tip room, menu room)` in points; menu actions `Send(Refresh)`, `Send(SelectOrg(uuid))`, `SetAlwaysOnTop(!cur)`, `SetStartWithWindows(!cur)`, `Send(Disconnect)`, `Quit`; the menu closes after any item, on Escape, on a click outside it, and when `input.focused` turns false.

**Tooltip** (`index.css` `.tip`, `RingTooltip.tsx`): opens while the pointer is over a ring column's hit box (the whole `.rg` column: SVG, name, reset line), never while the menu is open or a drag is under way; fades 0.15 s. Placed 16 below the hit box bottom, horizontally centered on the column and shifted to stay 6 inside the window. Box: bg `rgba(14,14,18,.94)`, border 1 `rgba(255,255,255,.1)`, radius 9, padding 7×10, text 11 px `#ddd`, line height 1.5, max width = window width − 12, no wrapping. Title row: `tip_title` semibold white, then " · " + `ago_text` regular `MUTE`, ellipsized; 5 below it a 1 px `rgba(255,255,255,.1)` rule, 5 more. Body per `TipModel`: bold white ring title + " · " + line; pace phrase bold in its color; resets line. While open its text refreshes every second (`Repaint::After(1 s)`). Room = `tip_room(hit_bottom, tip_height, widget_height)`; the first frame may lay out before the window grew: that frame draws at alpha 0.

**Menu** (`.menu`, `WidgetMenu.tsx`): opens on right-click at the cursor (top-left at the cursor, shifted to stay 6 inside the window, never flipped). Box: bg `rgba(32,32,38,.96)`, border 1 `rgba(255,255,255,.1)`, radius 10, padding 5, min width 200, shadow `0 16px 40px rgba(0,0,0,.5)` clipped to the window. Items 12 px `INK`, padding 6 10 6 26, radius 6, hover bg `rgba(255,255,255,.08)`; checked items show "✓" `#f0b38f` at x 9; danger item `#fda4af`; separators 1 px `rgba(255,255,255,.08)` with 4 px margins; group header 10 px uppercase, letter spacing .06 em, `IDLE`, padding 6 10 2. Order: [connected: Refresh, sep, (orgs > 1: header "Organization" + one item per org, checked = current, sep)], Always on top (checked), Start with Windows (checked), sep, [connected: Disconnect], Quit (danger). Room = `menu_room(cursor_y, menu_height, widget_height)`.

- [ ] **Step 1: Write tests.** Snapshots (rendered at the window size `size + extra`): `tooltip_session_{en,fr}`, `tooltip_weekly_en`, `tooltip_idle_en`, `tooltip_ultra_{en,fr}`, `menu_connected_en`, `menu_two_orgs_{en,fr}`, `menu_disconnected_en`. Unit tests:

```rust
#[test] fn tooltip_room_grows_window() { let o = hover_ring("connected", LimitKey::Session); assert!(o.extra > 60.0); }
#[test] fn closing_tooltip_returns_to_rest() { assert_eq!(hover_then_leave("connected").extra, 0.0); }
#[test] fn menu_items_for_disconnected() { assert_eq!(menu_labels("disconnected", Lang::En), ["Always on top", "Start with Windows", "Quit"]); }
#[test] fn select_org_sends_uuid() { assert_eq!(click_menu_item("twoOrgs", "Acme Studio"), vec![Action::Send(Command::SelectOrg("org-team".into()))]); }
#[test] fn menu_closes_on_blur() { assert_eq!(open_menu_then_blur("connected").extra, 0.0); }
#[test] fn no_tooltip_while_menu_open() { assert!(!tooltip_visible_with_menu_open("connected")); }
#[test] fn tooltip_ticks_every_second() { assert_eq!(hover_ring("connected", LimitKey::Weekly).repaint, Repaint::After(Duration::from_secs(1))); }
```

- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4:** create snapshots, inspect against `phase1-reference/*tooltip*` and `*menu*`, re-run — pass.
- [ ] **Step 5: Commit** `feat(app): ring tooltip and right-click menu inside the window`. Push.

---

### Task 13: Reset animations, entrance and the repaint policy in the widget

**Files:**
- Create: `app/src/ui/pill.rs`, `app/src/ui/sparks.rs`
- Modify: `app/src/ui/widget.rs`, `app/src/ui/rings.rs`

**Interfaces:**
- Consumes: Task 6-7 timelines and locks.
- Produces: `FrameOutput.repaint` per this policy, as a pure function tested on its own:

```rust
pub fn repaint_policy(animating: bool, spinner: bool, countdown: bool, tooltip_open: bool) -> Repaint
// animating || spinner → Now; countdown || tooltip_open → After(1 s); else Idle
```

Behavior (port of `ui/src/components/Widget.tsx:127-204`): when `state.reset_seq` differs from the last seen one and the rings are shown with `resets` non-empty, start a batch at the current `t`: lock each event's ring with `lock_keys`, remember the batch's `PillSpec` (`pill_for`), and play `planned_frame` / `surprise_frame` / `crossfade_frame` (reduced motion) per event; a locked ring keeps drawing the view it had before the batch (the "held" view) with the `RingFrame` overriding it. Pill shows once at the batch's first `*_PILL_AT` (only while the batch still owns a ring). At `*_END` (or `LOCK_SAFETY_MS = 4500` of animation time, whichever first) release the ring; the arc, colors and tick then tween to the live values with the normal transitions (from empty and green / gold). A newer batch takes over a ring still locked. Surprise: gold glow over the whole widget (inner glow `rgba(250,204,21,.32)` 26 px + 1.5 px ring `rgba(250,204,21,.85)`, alpha `glow_alpha`), sparks from the ring center at radius 23 seeded with `reset_seq`. The pill sits 2 below the widget top, centered, one line, 10 px semibold, padding 3×8, fully rounded; calm bg `rgba(20,40,32,.9)` text `#a7f3d0` border `rgba(52,211,153,.45)`; gold bg `rgba(48,40,10,.92)` text `#fde68a` border `rgba(250,204,21,.55)` glow `rgba(250,204,21,.3)` 12 px; font from `pill_font_size(width, 242 - 8, 18)`. When the rings go away (any non-connected state), drop locks, batch, glow, sparks and pill at once. **Entrance:** each time the rings view appears (first state, reconnect), arcs grow with `entrance_fraction` and the tick turns in from 0° over 0.8 s; skipped under reduced motion. The surprise number blur is approximated by drawing the number 3 times at ±`num_blur/3` px horizontal offsets with alpha / 3 (egui cannot blur text).

- [ ] **Step 1: Write the tests.**

```rust
#[test] fn policy() {
    assert_eq!(repaint_policy(false, false, false, false), Repaint::Idle);
    assert_eq!(repaint_policy(false, false, true, false), Repaint::After(Duration::from_secs(1)));
    assert_eq!(repaint_policy(true, false, true, false), Repaint::Now);
}
#[test] fn settles_to_no_repaint() { let mut h = render_live("connected"); h.run(); /* past entrance + tweens */ assert_eq!(h.state().last_output.repaint, Repaint::Idle); }
#[test] fn held_view_until_release() { /* planned on session: at t+1.0 s the name/reset line still show the old resets_at; at t+2.2 s the live one */ }
#[test] fn pill_only_once_per_batch() { assert_eq!(pill_shows(&reset_run("both")), 1); }
#[test] fn newer_batch_takes_over_ring() { /* planned seq 1, surprise seq 2 at +0.5 s: ring follows surprise_frame; seq 1's release at 2.15 s changes nothing */ }
#[test] fn stalled_lock_released_by_safety() { /* no frames between t0 and t0+5 s, then one frame: ring unlocked, pill was shown */ }
#[test] fn disconnect_mid_surprise_clears_effects() { /* surprise at t0, disconnected at t0+0.5: no glow, no sparks, no pill drawn; locks empty */ }
#[test] fn reduced_motion_crossfades() { /* reduced: no sparks, no glow, number 0 after 200 ms, unlocked at 500 ms */ }
```

Snapshots at fixed animation times: `planned_t300_en`, `planned_t900_en`, `planned_t1600_en` (pill visible), `surprise_t200_en` (flash + glow), `surprise_t500_en` (sparks), `surprise_t1500_fr` (gold pill, shrunk font), `entrance_t400_en`.

- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4:** create snapshots, inspect against the `phase1-reference/*planned*` / `*surprise*` frames, re-run — pass.
- [ ] **Step 5: Commit** `feat(app): planned and surprise reset animations, entrance, repaint policy`. Push.

---

### Task 14: Windows platform module

**Files:**
- Create: `app/src/platform/mod.rs`, `app/src/platform/windows.rs`
- Modify: `app/Cargo.toml` (`windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_System_Threading", "Win32_UI_WindowsAndMessaging", "Win32_Graphics_Gdi", "Win32_UI_Shell", "Win32_System_Registry"] }`)

**Interfaces:**
- Produces (in `platform`, each with a non-Windows stub):
  - `single_instance(name: &str) -> Option<InstanceGuard>` (named mutex; `None` when another holder exists; the guard closes the handle on drop) and `focus_existing(title: &str) -> bool` (`FindWindowW`, `ShowWindow(SW_RESTORE)`, `SetForegroundWindow`).
  - `mod autostart`: `run_value(exe: &Path) -> String` (`"\"<path>\""`), `needs_rewrite(current: Option<&str>, wanted: &str) -> bool` (case-insensitive compare), `struct RunKey { subkey: String }` with `RunKey::user()` (`Software\Microsoft\Windows\CurrentVersion\Run`), `get(&self, name) -> Option<String>`, `set(&self, name, value) -> io::Result<()>`, `remove(&self, name) -> io::Result<()>` (removing a missing value is Ok), and `sync(key: &RunKey, name: &str, on: bool, exe: &Path) -> io::Result<()>`.
  - `work_areas() -> Vec<Rect>`, `primary_work_area() -> Option<Rect>`, `work_area_of(hwnd: isize) -> Option<Rect>` (all physical px, `rcWork`).
  - `window_rect(hwnd) -> Option<Rect>`, `set_window_bounds(hwnd, pos: (i32,i32), size: (i32,i32))` (`SetWindowPos` with `SWP_NOZORDER | SWP_NOACTIVATE`), `is_minimized(hwnd) -> bool`.
  - `open_url(url: &str) -> io::Result<()>` (`ShellExecuteW` "open").
  - `reduced_motion() -> bool` (`SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)` returns false).

- [ ] **Step 1: Write the tests** (`#[cfg(all(test, windows))]`):

```rust
#[test] fn run_value_quotes_path_with_spaces() { assert_eq!(run_value(Path::new(r"C:\Users\A B\limitrings.exe")), r#""C:\Users\A B\limitrings.exe""#); }
#[test] fn rewrites_when_exe_moved() {
    assert!(needs_rewrite(Some(r#""C:\old\limitrings.exe""#), r#""C:\new\limitrings.exe""#));
    assert!(!needs_rewrite(Some(r#""C:\NEW\limitrings.exe""#), r#""C:\new\limitrings.exe""#));
    assert!(needs_rewrite(None, "x"));
}
#[test] fn sync_sets_and_removes_in_scratch_key() {
    let key = RunKey { subkey: format!(r"Software\limitrings-tests\{}", std::process::id()) };
    sync(&key, "limitrings", true, Path::new(r"C:\a b\x.exe")).unwrap();
    assert_eq!(key.get("limitrings").as_deref(), Some(r#""C:\a b\x.exe""#));
    sync(&key, "limitrings", false, Path::new("unused")).unwrap();
    assert_eq!(key.get("limitrings"), None);
    // cleanup: delete the scratch subkey
}
#[test] fn second_instance_is_refused() {
    let name = format!(r"Local\limitrings-test-{}", std::process::id());
    let a = single_instance(&name); assert!(a.is_some()); assert!(single_instance(&name).is_none());
    drop(a); assert!(single_instance(&name).is_some());
}
#[test] fn primary_work_area_exists() { assert!(primary_work_area().is_some_and(|r| r.w > 0 && r.h > 0)); }
```

- [ ] **Step 2: Run** `cargo test -p limitrings platform::` — FAIL.
- [ ] **Step 3: Implement** with `windows-sys` (`winreg` for the Run key).
- [ ] **Step 4: Run** `cargo test -p limitrings` — pass.
- [ ] **Step 5: Commit** `feat(app): Windows platform module (single instance, Run key, monitors, window bounds)`. Push.

---

### Task 15: Host, engine thread and the real window

**Files:**
- Create: `app/src/engine_thread.rs`, `app/src/host.rs`, `app/build.rs`, `app/assets/icon.ico`, `app/assets/icon-64.png`
- Modify: `app/src/lib.rs`, `app/Cargo.toml` (build-dep `winresource`)

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `engine_thread::start(deps: EngineDeps, wake: impl Fn() + Send + 'static) -> (mpsc::Sender<Command>, watch::Receiver<AppState>, JoinHandle<()>)`: a thread running a tokio multi-thread runtime with `worker_threads(2)`; spawns the engine and a forwarder that logs `conn` changes (as `src-tauri/src/lib.rs:402-419`) and calls `wake()` on every state change.
  - `host::fit_bounds(ppp: f32, size: Vec2, extra: f32) -> ((i32,i32) /*base px*/, i32 /*extra px*/)` (each side `ceil(v * ppp)`), then `PopupRoom::apply(current, base, extra_px, work_area)`; the returned `offset_y` px / ppp becomes `FrameInput.offset_y`.
  - `host::MoveSaver`: `observe(&mut self, pos: (i32,i32), now: Instant, minimized: bool, expanded: bool)` and `due(&mut self, now: Instant) -> Option<(i32,i32)>` (500 ms after the last observed change; ignores minimized and expanded observations; saves `persisted_position`).
  - `lib::run()`: logging init → `DevEnv::current()` → `Prefs` from `prefs_from_settings(&settings, sys_locale::get_locale().as_deref())` → `single_instance("Local\\limitrings-single-instance")` else `focus_existing("Claude Limit Checker")` and exit → settings (first run: save defaults; with `manages_autostart` and the setting on, `autostart::sync` on every start so a moved exe is fixed) → `engine_thread::start` (+ `Command::Connect` when `autoconnect`) → host.

Host behavior: window created hidden, borderless, transparent, not resizable, no OS shadow, in the taskbar, title "Claude Limit Checker", icon `icon-64.png`, always-on-top per settings; placed with `initial_position(saved, size_px, work_areas(), primary)` and shown after the first frame is painted. Each frame: build `FrameInput` (`now = Utc::now()`, `tz = Local::now().offset().fix()`, `t` = egui input time, `reduced_motion` read at start, `focused` from viewport info), call `Widget::show`, apply `fit_bounds` + `PopupRoom` when size/extra changed (via `set_window_bounds` on the HWND), apply actions (`Send` → `try_send`; `SetLang`/`SetAlwaysOnTop`/`SetStartWithWindows` → `SettingsStore::update` + effect, autostart only when `manages_autostart`; `OpenUrl` → `platform::open_url`; `Minimize`; `StartDrag` → first back to rest bounds when expanded, then the native drag; `Quit` → close + 3 s hard-exit thread as `src-tauri/src/lib.rs:318-330`), feed `MoveSaver` with the window rect, and request repaint per `FrameOutput.repaint` (`Idle` → nothing; `After(d)` → `request_repaint_after(d)`; `Now` → `request_repaint()`), plus `request_repaint_after(500 ms)` while a save is pending. The close button path (Alt+F4) also quits. `build.rs` embeds `assets/icon.ico` with ProductName "Claude Limit Checker", FileDescription "Claude Limit Checker", OriginalFilename "ClaudeLimitChecker.exe".

- [ ] **Step 1: Write the tests** in `host.rs`:

```rust
#[test] fn fit_rounds_up_at_fractional_scale() { assert_eq!(fit_bounds(1.25, vec2(244.0, 145.6), 0.0), ((305, 182), 0)); assert_eq!(fit_bounds(1.5, vec2(244.0, 146.0), 92.3), ((366, 219), 139)); }
#[test] fn move_saver_debounces_500ms() { /* observe A at 0, B at 300 ms → due(700) None, due(800) Some(B) once */ }
#[test] fn move_saver_ignores_minimized_and_expanded() { /* (-32000,-32000) minimized → never saved; expanded → saves rest */ }
```

- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** Copy the icons from `src-tauri/icons/` (`icon.ico`, `64x64.png`).
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` — pass.
- [ ] **Step 5: Real-window check.** `node tools/fake-claude/server.mjs` (background), then a debug run with `LIMITRINGS_BASE_URL=http://127.0.0.1:8787 LIMITRINGS_FAKE_SESSION=1`. Click Connect; after ≈ 5 s the rings appear. Screenshot with `tools/screenshot-window.ps1 -Title "Claude Limit Checker"` and compare by eye (Read both PNGs) with `phase1-reference`: connected, hover controls, tooltip (window grows, then shrinks), menu (two orgs: `POST /_ctl/orgs-2`), near limit (`go-92`), error (`fail-503`), expired (`next-401`), planned reset (`jump`, wait 45 s), surprise (`surprise-session`). Also: drag the window, restart the exe, it reopens where dropped; launch a second instance, the first comes to the front and no second widget appears; minimize, restore from the taskbar. Fix every difference worth fixing; list what remains in the Ledger.
- [ ] **Step 6: Commit** `feat(app): host window, engine thread, fit, position save and quit`. Push.

---

### Task 16: Remove Phase 1, CI, release, README

**Files:**
- Delete: `src-tauri/`, `ui/`, `app/examples/spike_*.rs`
- Modify: `Cargo.toml` (members `crates/limits-core`, `app`; `[profile.release]`), `Cargo.lock`, `.gitignore`
- Create: `.cargo/config.toml`
- Modify: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `README.fr.md`, `tools/fake-claude/README.md` (mention the egui exe instead of the Tauri shell)

- [ ] **Step 1: Delete and wire the workspace.** `git rm -r src-tauri ui app/examples`. Workspace `Cargo.toml`:

```toml
[profile.release]
lto = true
codegen-units = 1
opt-level = "s"        # "z" if Task 2's Ledger says it measured smaller with no visible cost
panic = "abort"
strip = true
```

`.cargo/config.toml`: `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]`. Remove the `ui/` lines from `.gitignore`.

- [ ] **Step 2: Verify the exe has no runtime DLL dependency.** `cargo build --release -p limitrings`, then `dumpbin /dependents target\release\limitrings.exe` (from a VS developer prompt; or `llvm-readobj --coff-imports`). Expected: only system DLLs (`KERNEL32`, `USER32`, `GDI32`, `OPENGL32` / `d3d12`/`dxgi` depending on the renderer, `ADVAPI32`, `SHELL32`, `ole32`, `bcrypt`, `ws2_32`, `ntdll`, `crypt32`, `secur32`, `dwmapi`, `uxtheme`, `imm32`), no `VCRUNTIME140.dll`, no `MSVCP140.dll`, no `api-ms-win-crt-*`. Record the exe size in the Ledger; zip it and record the zip size (< 6 MB).

- [ ] **Step 3: CI.** `ci.yml` on push and pull_request, `windows-latest`: checkout, `dtolnay/rust-toolchain` (stable, components rustfmt + clippy), `Swatinem/rust-cache`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Every action pinned to a full commit SHA with the tag in a trailing comment (look the SHAs up with `gh api repos/<owner>/<repo>/git/ref/tags/<tag>`, dereferencing annotated tags). If kittest's wgpu cannot get an adapter on the runner, set `WGPU_BACKEND=dx12` (WARP) and, as a last resort, let snapshot tests skip with a logged reason only when `CI` is set and no adapter exists.

- [ ] **Step 4: Release.** `release.yml` on `v*` tags: fail unless `${GITHUB_REF_NAME#v}` equals the `limitrings` version from `cargo metadata --no-deps --format-version 1`; `cargo build --release -p limitrings`; copy to `ClaudeLimitChecker.exe`; `Compress-Archive` to `ClaudeLimitChecker-windows-x64.zip`; publish both with `softprops/action-gh-release` (SHA-pinned). Keep the "Releases are unsigned" comment.

- [ ] **Step 5: README (EN and FR).** Download = the bare exe or the zip; no installer; SmartScreen section kept; "Start with Windows is off by default; move the exe somewhere permanent first (e.g. `%LOCALAPPDATA%\Programs\`) because it registers the exe where it sits"; uninstall = turn off Start with Windows, delete the exe, optionally delete `%APPDATA%\limitrings` and the `limitrings` Credential Manager entry; build-from-source section uses `cargo build --release -p limitrings` (no Node). Keep the "Unofficial, not affiliated with Anthropic" opening and the existing structure.

- [ ] **Step 6: Verify** `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — pass; `git grep -n -i "tauri\|webview\|npm" -- ':!docs'` returns nothing that still applies.
- [ ] **Step 7: Commit** `chore: remove the Tauri shell and React UI; single-exe CI and release`. Push.

---

### Task 17: Measurements, final check and the PR

**Files:**
- Modify: this plan's Ledger; `docs/screenshot.png` (replace with a real-window capture of the egui widget, fake data)

- [ ] **Step 1: Measure.** Release builds ignore `LIMITRINGS_*`, so add to the workspace `Cargo.toml` a `[profile.measure]` with `inherits = "release"` and `debug-assertions = true` (this turns the dev switches on with release codegen). Build `cargo build --profile measure -p limitrings`, run it against `tools/fake-claude` with `LIMITRINGS_BASE_URL` + `LIMITRINGS_FAKE_SESSION=1` + `LIMITRINGS_AUTOCONNECT=1`, wait 5 min connected, and record private bytes and average CPU over 60 s (same commands as Task 2). Also record the true release exe disconnected and idle after 5 min, exe size, zip size, and cold start (launch to window visible: `Measure-Command` around `Start-Process` plus a `FindWindowW` poll loop). The owner adds the connected-to-claude.ai number in the PR checklist.
- [ ] **Step 2: Run superpowers:verification-before-completion** — `cargo test --workspace`, clippy, fmt, the release build — and paste the outputs in the Ledger.
- [ ] **Step 3: Replace `docs/screenshot.png`** with a real-window capture (fake data) and commit `docs: Phase 2 measurements and screenshot`. Push.
- [ ] **Step 4: Open the PR** `rewrite/phase-2-egui` → `master` with `gh pr create`. Body: summary, the Ledger numbers (Phase 1 ≈ 110 MB → Phase 2 X MB), the renderer choice, the deviations list below, and the owner checklist (first item: "Uninstall the Phase 1 app first"; then: run the exe on a clean machine or account, SmartScreen, Connect with Firefox, rings match claude.ai, 150 % scaling looks right, Quit with a real mouse, turn on Start with Windows + reboot, Wi-Fi cut and recovery, idle RAM after 5 min connected). End with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. The owner merges.

---

## Deviations from Phase 1 to confirm with the owner

- Hover buttons (minimize, close, refresh, flags) lose their native `title` hover text.
- The planned-reset ✓ keeps its full 3.2 s fade instead of vanishing when the ring unlocks at 2.15 s.
- The surprise number "blur" is approximated (egui has no text blur).
- Digits in the reset line are proportional if egui ignores Inter's `tnum` feature.

## Ledger

*(Filled during execution: visual differences left (Task 15), exe/zip sizes (Task 16), final measurements and verification output (Task 17).)*

### Task 2 — renderer spike

Release builds, Windows 11, 1920×1080 at 100 %, Rust 1.100 (egui/eframe 0.37 need ≥ 1.95). Each spike ran 60 s (ring animation done after 1.2 s), then private bytes (`PrivateMemorySize64`) and CPU over 10 s (`\Processus(spike_x)\% temps processeur` on this French Windows, cross-checked with `TotalProcessorTime`). Screenshots over a full-screen multicolor checkerboard, corners zoomed ×12.

| Renderer | Private MB at 60 s | Avg CPU % | Transparent corners clean? | Notes |
|---|---|---|---|---|
| `eframe` + `glow` | 127.9 | 0.0 | **No**: black corners, glass opaque (checkerboard not visible through the 80 % fill) | OpenGL driver cost alone is ~5× the budget. |
| `eframe` + `wgpu` (DX12 only) | 377.8 | 0.0 | **No**: same black corners, opaque glass | DXGI flip-model swap chain on a plain HWND has no per-pixel alpha. |
| `winit` + `egui` + CPU raster + `UpdateLayeredWindow` | **3.5** | 0.0 | **Yes**: anti-aliased rounded corners, checkerboard visible through the glass, no fringe | Working set 16 MB (mostly shared system DLL pages). |

`softbuffer` was not tried: it presents with `BitBlt`/`StretchDIBits` (no per-pixel alpha on Win32), which is why the layered path uses `UpdateLayeredWindow` from a premultiplied DIB instead.

Gotcha found and fixed: winit rewrites `GWL_EXSTYLE` from its own flags on every style change (show, minimize, always-on-top...), which drops `WS_EX_LAYERED`; `UpdateLayeredWindow` then fails with error 87 and the window falls back to an opaque redirection bitmap (black corners). `platform::Surface` (Windows) subclasses the window and keeps `WS_EX_LAYERED` in every `WM_STYLECHANGING`.

Opt-level (layered spike, LTO + `codegen-units = 1` + `panic = "abort"` + strip): `opt-level = 3` exe 3.10 MB; `"s"` 2.74 MB exe, 3.7 MB private; `"z"` 3.40 MB exe, 3.8 MB private, both 0 % idle CPU and the same rendering. **Keep `"s"`** (`"z"` measured larger here).

**Choice: the layered CPU host** (`app/src/host.rs` + `render.rs` + `platform/windows.rs`): the only variant with a clean transparent rounded window, and 3.5 MB private against a 25 MB budget. No eframe in the app; Task 15 extends `host.rs` (the spike's loop) instead of writing an `eframe::App`. Snapshot tests (Task 8+) still use `egui_kittest` with wgpu as a dev-dependency only.


### Task 15 — real window

Debug build against `tools/fake-claude`, Windows 11, 1920×1080 at 100 %. Every Step 5 check passed (see `.superpowers/sdd/2026-11-13-egui-native-ui/task-15-report.md`). Host gotchas found and fixed on the way:

- **Window moves and resizes go through `UpdateLayeredWindow` only** (`Surface::present(pos)`), together with the pixels. `SetWindowPos` first, then present, left an instant where the cursor sat over transparent (click-through) pixels: Windows reported the pointer gone and a tooltip that grew the window upward closed and reopened in a loop. Between passes the host also shifts egui's pointer by the window move, and a `CursorLeft` within 300 ms of our own move is ignored when `WindowFromPoint` still finds the widget (tracking re-armed).
- **Theming is off for the widget window** (`SetWindowTheme(hwnd, "", "")`). On a frame change (toggling always-on-top) the theme gave the captioned layered window a region of its current size. Once it had one, its input shape stayed that size when a menu grew the window, so the menu's lower items got no clicks, even after the region was removed. `Surface::present` still drops any region it finds (safety net, logged).
- winit 0.30.13 `drag_window` posts `WM_NCLBUTTONDOWN` with a pointer as `lParam`, but the move loop tracks the cursor itself: drags land exactly where dropped (checked). Kept.

Visual differences left (by eye against `phase1-reference`, all states):

- Tooltip and menu stay inside the 244 px window width. Phase 1's webview let them overhang the widget on the left (`connected_en_tooltip-session`, `twoOrgs_en_menu`). Same content, narrower box (Task 12 ruling).
- Body text wraps a word earlier in places (Inter vs Segoe metrics, e.g. `expired`: "Reconnect / to keep tracking" vs "Reconnect to / keep tracking").
- With the error foot line, the hover refresh button overlaps the end of the "retry in N s" text. Phase 1 positions it the same way (absolute bottom-right), so this is inherited, not new.
- The fake org names come from `tools/fake-claude` ("Max plan · Personal", "Team · Acme"), not the reference fixtures; the layout matches.
- Debug builds rasterize slowly (~90 ms per pass at 244×291), so animations stutter in debug. Release timing is Task 17's measurement.


### Task 16 — Phase 1 removed, CI, release

`src-tauri/`, `ui/`, `app/examples/` and the spike-only `eframe`/`wgpu` optional dependencies and features are gone; workspace members are `crates/limits-core` and `app`. `[profile.release]` is LTO, `codegen-units = 1`, `opt-level = "s"` (kept per Task 2), `panic = "abort"`, `strip`; `.cargo/config.toml` sets `+crt-static` for `x86_64-pc-windows-msvc`.

`cargo build --release -p limitrings` (212 crates, 2 min 10 s clean-ish): exe **6 713 344 bytes (6.4 MB)**; `ClaudeLimitChecker-windows-x64.zip` (Compress-Archive, exe only) **3 434 497 bytes (3.3 MB)**, under the 6 MB download budget. The exe is larger than the Task 2 spike (2.74 MB) because it now carries the whole app (tokio, HTTP/TLS, SQLite/cookie reader, embedded Inter fonts, egui). Not an issue for the budget.

Import table (parsed from the PE, `dumpbin` is not installed; static imports, no delay-load table):

```
kernel32.dll  bcryptprimitives.dll  api-ms-win-core-synch-l1-2-0.dll (API set, ships with Windows)
user32.dll  ws2_32.dll  comctl32.dll  uxtheme.dll  gdi32.dll  shell32.dll  ntdll.dll
dwmapi.dll  imm32.dll  bcrypt.dll  advapi32.dll  ole32.dll
```

System DLLs only: no `VCRUNTIME140`, no `MSVCP140`, no `api-ms-win-crt-*`, no `OPENGL32`/`d3d12`/`dxgi`. CI: `windows-latest`, fmt, clippy `-D warnings`, `cargo test --workspace` with `WGPU_BACKEND=dx12` for the kittest snapshots (WARP on the GPU-less runner); not exercised here since Actions cannot run locally. Action SHAs (looked up with `gh api`): checkout v7.0.1, rust-cache v2.9.3, action-gh-release v3.0.4, dtolnay/rust-toolchain `stable` branch tip.

Local verification: `cargo fmt --all --check` clean, clippy `-D warnings` clean, `cargo test --workspace` 256 (app) + 112 (limits-core, 3 ignored) passed.

Task 16 fix round 1 (size): `opt-level = "z"` on the full app measured **larger** (7 157 760 B vs 6 713 344 B), so `"s"` stays. `tempfile` in `limits-core` is used by non-test code (`session/firefox.rs`), so it stays a normal dependency. `cargo bloat --crates` top contributors (.text 4.9 MiB): std 682 KiB, fearless_simd 454 KiB, vello_cpu 262 KiB (egui's epaint rasterizer), harfrust 257 KiB, skrifa 241 KiB, rustls 235 KiB, limitrings_app 153 KiB, read_fonts 148 KiB, epaint 131 KiB, egui 130 KiB, regex_syntax 77 KiB, hashbrown 85 KiB, ring 70 KiB. Trim taken: `tracing-subscriber` `EnvFilter` replaced by `filter::Targets` (same levels), features reduced to `fmt, registry, std`, which drops the regex stack. Result: exe **6 359 040 B (6.36 MB decimal, 6.07 MiB)**, zip about 3.3 MB. Still just above 6 MB for the bare exe; what is left (egui text and raster stack, rustls, bundled SQLite, ICU data via `idna`) would need risky surgery, so it stops here. The zip, which is the actual download-size number, is well under budget.

### Task 17 — measurements and final verification

Windows 11 (French), 1920x1080 at 100 %, Rust 1.100. Private bytes = `PrivateMemorySize64` (cross-checked with `Win32_PerfFormattedData_PerfProc_Process` PrivateBytes 5.33 MB, WorkingSetPrivate 4.4 MB). CPU = `\Processus(limitrings)\% temps processeur` over 60 s starting at the 5-minute mark, cross-checked with `TotalProcessorTime` deltas.

Added `[profile.measure]` (`inherits = "release"`, `debug-assertions = true`) to the workspace `Cargo.toml`; `cargo build --profile measure -p limitrings` gives an 8 034 304 B exe with the dev switches on.

| Build / state | Private MB at 5 min | Working set MB | Avg CPU (60 s) | CPU s per 60 s (TotalProcessorTime) |
|---|---|---|---|---|
| `measure` build, fake-claude, `LIMITRINGS_BASE_URL` + `FAKE_SESSION=1` + `AUTOCONNECT=1`, connected (62 % / 34 % rings shown) | **5.09** | 24.7 | 2.67 % of one core | 1.625 |
| true release exe, no dev switches, disconnected, idle | **5.06** | 50.1 | 2.62 % of one core | 1.594 |

Phase 1 was about 110 MB private; Phase 2 is about 5 MB (budget < 25 MB). Idle CPU is NOT ~0 %: about 2.6 % of one core in both states, i.e. something repaints or ticks while idle (Task 2's spike measured 0.0 % but did not have the app's tick). Flagged to the controller as a concern; the owner adds the connected-to-claude.ai number.

Sizes: release exe **6 359 040 B (6.36 MB)**; zip of that exe (`Compress-Archive`, same as release.yml) **3 267 417 B (3.27 MB)**.

Cold start (release exe, `Start-Process` to `FindWindowW("Claude Limit Checker")` found, 5 runs, ms): 82, 38, 32, 45, 30. The 82 ms is the first run after a fresh build (cold file cache); later runs are warm. Includes Start-Process overhead.

Screenshot: `docs/screenshot.png` (244x146 px) replaced with a real-window capture of the `measure` build against fake-claude, connected state, over a neutral dark (#282C34) borderless form; no email or account data. Capture steps: fake server on 8787, measure exe with the three switches, a topmost background form behind the widget, `tools/screenshot-window.ps1`.

Verification (all run on 4e0f192 + the Cargo.toml profile):

```
$ cargo fmt --all --check            -> no output, exit 0
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.90s    (exit 0)
$ cargo test --workspace
test result: ok. 256 passed; 0 failed; 0 ignored   (limitrings_app lib)
test result: ok. 0 passed                          (limitrings bin)
test result: ok. 112 passed; 0 failed; 3 ignored   (limits-core)
test result: ok. 0 passed (x2, doctests)
$ cargo build --release -p limitrings
    Finished `release` profile [optimized] target(s)   -> target/release/limitrings.exe 6 359 040 B
```

All processes started here (limitrings, node fake server, background form) were stopped. The real Run key was not touched; the true release build was never connected.
