# Claude Limit Checker, Phase 2: native egui UI

Date: 2026-11-05 · Status: draft, pending owner review
Parent spec: [`2026-11-03-usage-widget-design.md`](2026-11-03-usage-widget-design.md). Everything it says about data, pace, resets, states, wording, settings, logging and packaging still holds unless this document changes it. This document covers only what Phase 2 changes.

---

## 1. Why

Phase 1 (Tauri 2 + React, merged in PR #1) works, but the process tree uses **105–117 MB of RAM**, almost all of it WebView2. The owner runs it all day and wants it to be **light** and to have **zero runtime dependencies**: one `.exe` anyone can download and run, with nothing else to install (no WebView2, no Node, no .NET, no VC++ redistributable).

The owner chose **egui** (via `eframe`) as the toolkit: "egui, let's go".

## 2. Goals, non-goals, anti-goals

**Goals**
- Same widget, same behavior as Phase 1. Phase 1's running app and the prototype (`2026-11-03-usage-widget-prototype.html`) are the visual and behavioral reference: rings, pace tick and colors, tooltip, right-click menu, hover controls, flags, refresh spin, Connect / Waiting / No Firefox / Expired / Error screens, planned and surprise reset animations, pill, near-limit glow, entrance spring.
- **The Ultra plan state** (added in PR #2): on a plan without a 5-hour limit (`Plan::has_session_limit() == false`, e.g. Ultra), the left ring is greyed with "∞", the label "No 5 h limit" / "Pas de limite 5 h", no tick, no reset animation, and the tooltip says "No 5-hour limit on the Ultra plan" / "Pas de limite de 5 h sur le forfait Ultra". `RingView.no_limit` already carries this from the core.
- **One file to run:** a single statically linked `.exe` (CRT linked statically) that starts on a clean Windows 10/11 machine.
- **Idle RAM < 25 MB** (private bytes, the whole process), idle CPU ≈ 0 %.
- **Download < 6 MB.**
- **macOS later must stay possible:** everything Windows-only sits behind `cfg(windows)` in its own module with a small trait or function surface, so a Mac port adds modules instead of rewriting the UI.

**Non-goals (Phase 2)**
- Any new feature. No tray icon, notifications, history, Chrome support, Mac build.
- Changing `limits-core` behavior. It is reused as is; small additive changes (a new helper, a new field) are fine if the UI needs them, with tests.

**Anti-goals**
- A visibly worse widget than Phase 1: jagged rings, flicker, a frame of garbage on start, the window background showing as a black or white rectangle around the rounded glass.
- A UI that redraws continuously when nothing moves (idle CPU must stay ≈ 0).
- Every Phase 1 anti-goal still applies (no telemetry, no cookie logging, no Anthropic branding, no blinking).

## 3. What changes, what stays

**Stays:** `crates/limits-core` (engine, API, parsing, pace, reset detection, Firefox session source, Credential Manager storage, settings store, `window_pos` helpers including `PopupRoom`), the settings file and its location, the Credential Manager target, the `limitrings` internal ID everywhere, the log location and rotation, `tools/fake-claude` (dev only), README structure.

**Goes:** `src-tauri/`, `ui/` (React, Vite, Node, Tailwind, vitest, Playwright mocks), `tauri.conf.json`, the Tauri plugins, the NSIS bundle config. They are deleted in the same branch once the egui app reaches parity, not before.

**New:** a binary crate `app/` (package and binary name `limitrings`, product name "Claude Limit Checker") that owns the window, drawing, animation, i18n and Windows integration, and talks to the core only through `spawn_engine` (commands in, `watch<AppState>` out), exactly as the Tauri shell does today.

## 4. Architecture

- **Threads:** the egui UI on the main thread; a tokio multi-thread runtime on a background thread running the engine. A small forwarder task waits on the `watch` receiver and calls `egui::Context::request_repaint()` on each change, so the UI wakes only when state changes.
- **Repaint policy (this is what keeps idle CPU at zero):** the UI repaints on input, on a state change, and while an animation is running. Anything time-driven requests the next repaint at the moment it is needed: the error countdown once per second, "updated X ago" in an open tooltip once per second, the pace tick and colors at the next 30 s poll (which already triggers a state change). No repaint timer runs otherwise.
- **Shell logic ported from `src-tauri/src/lib.rs`:** `resolve_language`, `initial_position`, `DevEnv` and the `LIMITRINGS_*` debug switches, `manages_autostart` (rule R14: only release builds touch autostart, unless `LIMITRINGS_DEV_AUTOSTART=1`), `settings_path`, position save with a 500 ms debounce that ignores minimized positions and popup growth, the Quit path with a 3 s hard-exit fallback. Their existing unit tests move with them.
- **UI logic ported from `ui/src/lib` and `ui/src/anim`:** ring geometry, formatting, i18n table, menu model, room math, easing and spring, planned / surprise / pill / lock timelines. These are pure functions today and have vitest tests; each gets an equivalent Rust unit test with the same cases, so the port is checked against Phase 1's expectations, not re-derived.

## 5. Window and drawing

- **Window:** borderless, transparent, not resizable, no shadow from the OS, always on top per the setting, shown in the taskbar with the app icon. The window fits the widget's content (rule R11 from Phase 1: no Acrylic or Mica; the glass is drawn, not a system backdrop). The glass is a filled rounded rect **`rgba(22,22,28,.80)`** with the 1 px `rgba(255,255,255,.09)` border, matching Phase 1's shipped build (`ui/src/index.css`, `.tauri .wd`). The `.66` in the parent spec only applies with a real blur behind it, which neither build has.
- **Effects stay inside the widget box** (rule R13): glows, sparks and the pill are clipped to the widget, not drawn outside it.
- **Tooltip and menu:** drawn inside the same window. When one opens, the window grows to make room and shrinks back on close, using `PopupRoom` from the core exactly as Phase 1's `fit_window` does (grows upward when there is no room below, never persists the grown position).
- **Rings:** anti-aliased arcs with round caps (tessellated paths with egui's feathering), track, pace tick, number with smaller "%". They must look as smooth as Phase 1 at 100 %, 125 % and 150 % display scaling.
- **Fonts:** Inter (OFL), embedded, subset to Latin plus the symbols used ("∞", "✓", "★", "⚠", "·"). Icons (Lucide: refresh-cw, minus, x) are drawn as vector paths, not a font. FR / UK flags are drawn as shapes, as in Phase 1.
- **Drag:** from anywhere except controls, via the native drag command.

## 6. Renderer choice: measured, not assumed

egui can render through OpenGL (`glow`) or `wgpu`, and some GPU drivers make either one cost far more RAM than the < 25 MB target. **The first task of the plan is a spike** that builds the bare transparent window with one animated ring and measures private bytes and idle CPU after 60 s, for:

1. `eframe` + `glow`
2. `eframe` + `wgpu` (DX12 backend only)
3. a CPU raster path (egui tessellation rasterized with `tiny-skia` into a `softbuffer` surface on a `winit` window)

Pick the first option in that order that meets < 25 MB and draws a clean transparent rounded window. If none meets the target, pick the lightest that looks right, record the numbers in the spec, and continue: RAM below Phase 1's is still the win the owner asked for. The spike's numbers and the choice go in the plan ledger and the PR.

## 7. Windows integration (no Tauri plugins)

Each item lives in `app/src/platform/windows.rs` behind a small function surface, using `windows-sys` or `winreg` (already a dependency of the launcher):

- **Single instance:** a named mutex `Local\limitrings-single-instance`. A second launch finds the existing window and restores and focuses it, then exits.
- **Start with Windows:** the `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `limitrings` pointing at the current exe path, quoted. On each release-build start with the setting on, rewrite the value if the exe has moved. **Default is now off** (owner's decision, because a bare exe is often first run from Downloads): `Settings::default().start_with_windows` becomes `false` in `limits-core`, with its tests updated. An existing settings file keeps whatever value it has. The user turns it on from the right-click menu.
- **Open Firefox:** the existing `launch.rs` logic (registry lookup, then default path) moves over unchanged.
- **Minimize / restore / always on top / taskbar click:** via the windowing library's commands.
- **Locale:** the existing `sys-locale` call.
- **App icon:** embedded as a Windows resource (taskbar, Alt-Tab, exe icon), generated from `src-tauri/icons/icon-source.png` before that folder is deleted.

## 8. Packaging and release

- The release artifact is the **single `.exe`** (`ClaudeLimitChecker.exe`) plus a zip containing it, attached to the GitHub Release. No installer (owner's decision): there is nothing to install. "Start with Windows" is off by default and, when turned on, registers the exe where it sits; the README tells people to put it somewhere permanent before turning it on (e.g. `%LOCALAPPDATA%\Programs\`) and how to uninstall (turn off Start with Windows, delete the exe, optionally delete `%APPDATA%\limitrings` and the Credential Manager entry).
- Built with `cargo build --release` using a release profile tuned for size (LTO, `codegen-units = 1`, `opt-level = "s"` or `"z"` if the spike's measurements allow, `panic = "abort"`, strip) and the static CRT.
- Still unsigned; the README's SmartScreen section stays.
- CI: `ci.yml` drops the Node steps and runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`. `release.yml` builds the exe on a tag and attaches the exe and zip. While touching it, pin actions to full commit SHAs and fail the release if the tag doesn't match the crate version (open follow-ups from PR #1).

## 9. Testing and visual checks

- **Unit tests:** everything in §4's port lists, plus the existing core suite. `cargo test --workspace` stays green throughout.
- **Snapshot tests** with `egui_kittest` (dev-dependency only): render the widget headless and compare PNG snapshots for each state, in EN and FR: connected (on track, ahead, room, near limit), idle session, Ultra "no limit", error with countdown, expired, disconnected, waiting, no Firefox, tooltip open, menu open (with and without the Organization submenu), and fixed frames inside a planned reset, a surprise reset, and the pill. Snapshots are committed; they must not contain any real account data or email.
- **Real-window checks** by the implementer: run the debug build against `tools/fake-claude` with `LIMITRINGS_FAKE_SESSION=1`, screenshot the real window (including at 150 % scaling), and compare by eye with Phase 1 screenshots of the same states. Playwright no longer applies; these screenshots replace it.
- **Measurements in the final PR:** idle private bytes and CPU after 5 minutes connected, exe size, zip size, cold start time.
- **Phase 1 coexistence:** only the owner has Phase 1 installed (no release was ever published), so no migration code. The PR checklist starts with "uninstall the Phase 1 app first" (its uninstaller could otherwise remove the `limitrings` Run value the new exe wrote, and the two single-instance locks differ).
- **Owner checklist** (manual, in the PR): run the exe on a machine or account with nothing installed, SmartScreen, Connect with Firefox, rings match claude.ai, Quit with a real mouse, reboot autostart, Wi-Fi cut and recovery.

## 10. Decisions

**The owner's:**
- Rewrite the UI in pure Rust with egui; no Tauri, no WebView.
- Goal: much lighter (Phase 1 is ~110 MB) and zero dependencies so anyone can use it.
- Same widget as Phase 1, including the Ultra "No 5 h limit" ring.
- Phase 1 merged first, then the rewrite (done: PR #1 and PR #2 are on master).
- No installer: the release is the bare `.exe` plus a zip (this replaces the parent spec's "installer plus portable zip"). Start with Windows is **off by default** and points at wherever the exe is when turned on.

**Calls made by Claude (check these):**
- Glass opacity `.80` with no backdrop, as Phase 1 shipped.
- No migration code for Phase 1 installs; a checklist note instead.
- Renderer chosen by a measured spike (§6) rather than up front, with a CPU-raster fallback.
- No real blur behind the glass (same as Phase 1's shipped build).
- Popups grow the single window, reusing `PopupRoom`, instead of separate native windows.
- Inter embedded as the font; Lucide icons as vector paths.
- Snapshot tests via `egui_kittest` replace Playwright and vitest.
- `src-tauri/` and `ui/` are deleted in this branch once parity is reached.
- New crate at `app/`, keeping the `limitrings` internal ID.

## 11. Left to the builder

- Module layout inside `app/`.
- Exact spring constants and easing curves, as long as the motion matches Phase 1 side by side.
- How the tiny-skia path is wired, if the spike picks it.
- Whether the error countdown and "updated X ago" share one 1 s repaint request.
