# Claude Limit Checker: design

Date: 2026-11-03 · Status: approved prototype, pending plan
Interactive reference: [`2026-11-03-usage-widget-prototype.html`](2026-11-03-usage-widget-prototype.html). Open it in a browser. It is the visual and behavioral source of truth for everything in this document: sizes, colors, animations, wording, timings. Where this text and the prototype disagree, ask; otherwise follow the prototype.

---

## 1. What this is, and why

A tiny always-on-top Windows desktop widget that shows, live, how much of your **Claude.ai plan limits** you have used, the **5-hour session** limit and the **weekly** limit, and compares each one to the **rate you should be at** if you spread your usage evenly over the window.

It replaces the current Python/tkinter app in this repo (`token_monitor.py`). That app has worked for six months for one person: one account, Firefox only, French UI, hardcoded org. It just broke because claude.ai started returning `null` for an idle 5-hour window. The owner wants to:

1. **Rewrite it from scratch**, cleanly, as something **other Claude users can install and connect to their own account**, and publish it as **open source**.
2. Make it **very light**: it runs all day, so idle CPU should be ~0 and memory should be small.
3. Make it **feel good**: a polished look in the style of the [beUI](https://beui.dev/) React library (glass surfaces, spring-based motion), with small celebratory moments when a limit resets.

The question the widget answers at a glance: **"How much do I have left, and am I going too fast?"**

## 2. Goals, non-goals, anti-goals

**Goals**
- Any Claude.ai user on Windows **with Firefox** can install it, click **Connect**, log in through Firefox, and see their own limits. No copying or pasting.
- Shows both limits as rings, with a pace tick and a pace color, refreshed every 30 s.
- Tells a **planned** reset (on schedule) apart from a **surprise** reset (Anthropic reset it early), with a different animation for each.
- UI in **French and English (UK)**, switchable from the widget.
- Open source, MIT, with an honest README.

**Non-goals (v1)**
- macOS / Linux (macOS is planned later; don't make choices that block it).
- **Chrome (and other Chromium browsers) in v1.** Chrome on Windows locks its cookies so other apps can't read them, so Chrome support needs a companion extension published on the Chrome Web Store. That is planned for **v2** (§5, "Later: Chrome"). Keep the session-source code behind an interface so a second source can plug in.
- Per-model scoped limits (e.g. the weekly limit for a specific model), extra-usage credits, spend, usage history or charts.
- Notifications or sounds, tray icon, multiple accounts shown at once.
- Any API-key / Anthropic Console usage.

**Anti-goals: things that would make this a failure even if it "works"**
- **Pretending to be an official Anthropic product.** No Anthropic or Claude logo; neutral ring icon; "unofficial, not affiliated with Anthropic" in the README and in the About text.
- **Using a fake OAuth or reusing Claude Code's credentials** to log in. Anthropic does not offer third-party OAuth for claude.ai accounts, and its terms reportedly forbid third-party apps offering claude.ai login or using subscription credentials. See §5.
- **Malware-like techniques**, e.g. decrypting Chrome's app-bound-encrypted cookie store, DLL injection, or elevated privileges. Antivirus software would flag them, and they'd betray users' trust.
- **Copy-paste setup** (pasting a `sessionKey` from DevTools). The owner explicitly rejected it.
- **Being heavy or chatty:** visible CPU when idle, or polling faster than 30 s.
- **Blinking and nagging.** No pulsing elements. The only motion is the entrance, resets and button feedback.
- **Sending data anywhere except claude.ai.** No telemetry, no analytics, no update pings to a third party in v1.

## 3. Platform and technology

**Windows 10/11 only** for v1.

Build in **two phases**, which the owner chose:

- **Phase 1: Tauri 2 + React.** The Rust backend (auth, HTTP, polling, reset detection, settings, Windows integration) plus a React 19 front end using **beUI** components and its stack (Tailwind CSS 4, Motion). This gives the exact beUI look quickly and makes a complete, shippable app.
- **Phase 2: native Rust UI.** Replace the WebView front end with a native Rust UI to cut memory, keeping the Phase 1 Rust backend as is. The goal is visual and behavioral parity with Phase 1. Some glass and blur effects will be approximated (Windows Acrylic/Mica can supply the backdrop). The **toolkit choice is left to the builder**; the criteria are idle RAM, animation quality (spring easing, per-element transforms) and Windows backdrop support.

So the **backend must not depend on the UI layer**. Keep it a separate Rust crate with a small interface: a state stream out, commands in. That way Phase 2 swaps only the UI.

Performance targets:
| | Phase 1 (WebView2) | Phase 2 (native) |
|---|---|---|
| Idle CPU | ~0% (no timers except the 30 s poll; no animation loop when nothing moves) | same |
| Idle RAM (total, incl. child processes) | best effort, expected 50–100 MB | **< 25 MB** |
| Installer | < 10 MB | < 6 MB |

## 4. Data source

Undocumented claude.ai endpoints, authenticated with the user's claude.ai session cookies (`sessionKey` and related cookies, as a browser would send them).

- `GET https://claude.ai/api/organizations` → list of orgs. Relevant fields: `uuid`, `name`, `capabilities` (array; `"chat"` = a claude.ai plan org; API-only orgs lack it), `rate_limit_tier`.
- `GET https://claude.ai/api/organizations/{uuid}/usage` → usage. Relevant fields:
  - `five_hour`: `{ utilization: number (0–100), resets_at: ISO-8601 }` **or `null`** when no 5-hour session is running.
  - `seven_day`: same shape, or `null`.
  - `limits`: newer structured array (`kind`, `group`, `percent`, `resets_at`, `scope`, `is_active`). Use it as a **fallback** if `five_hour`/`seven_day` disappear. Ignore scoped entries in v1.
  - Everything else is ignored.
- Treat both as **unstable**. Parse defensively: a missing or `null` field never crashes; an unknown shape becomes an error state with the raw body written to the log.

Captured on 2026-11-02, real response shape (trimmed): `"five_hour": null, "seven_day": {"utilization": 1.0, "resets_at": "2026-11-06T23:59:59.657790+00:00", ...}, "limits": [{"kind":"weekly_all","percent":1,"resets_at":"…"}, …]`.

**Polling:** every **30 s** while the app runs, minimized or not. Use a manual refresh on demand. On failure, back off: 1 min → 2 → 4 → 8 → 10 min max, then return to 30 s after the first success. Never poll when no session is stored.

**Errors:**
- Network failure, timeout, 5xx, or 403 with a Cloudflare challenge page → **error state**: keep the last values, grayed out, and back off.
- 401, or 403 with a JSON auth error → **session expired** state.

Make the HTTP request look like the browser it came from: same cookies, a normal User-Agent, `Referer: https://claude.ai/settings/usage`.

## 5. Connecting an account (the "Connect" flow)

Why not a "Log in with Claude" OAuth button: Anthropic offers no OAuth for third-party apps on claude.ai accounts, and as of 2026 its terms reportedly forbid third-party apps from offering claude.ai login or using subscription credentials. Faking it, for example with Claude Code's client ID, is an anti-goal. So the app **piggybacks on the user's real browser session** and says so plainly in the README, including that this sits in a gray area of the terms and can break at any time.

**Flow**, as in the prototype's "Start over" path:
1. First launch → widget shows "See your Claude limits live · Uses your claude.ai login in your browser. Nothing to copy or paste." and a **Connect to claude.ai** button.
2. Click → the app opens `https://claude.ai/login` **in Firefox**: the default browser if Firefox is the default, otherwise Firefox explicitly via its install path. Widget shows "Waiting for Firefox…" with "Open claude.ai again" and "Cancel".
   - If Firefox isn't installed, the widget shows instead: "Claude Limit Checker needs Firefox for now (Chrome support is coming)", with a link to download Firefox. No Connect wait.
3. The user logs in normally, if not already logged in.
4. The app picks up the session from **Firefox**: read cookies for `claude.ai` from the profile's `cookies.sqlite`. Find the default profile via `profiles.ini`; if there are several profiles, check all of them and use the one with the most recent claude.ai session. The DB is locked while Firefox runs, so copy it plus its `-wal` file to a temp file first, then read. While waiting, check every 2 s.
5. Session found → call `/api/organizations` and pick the org (see below) → **Connected**. The browser tab is left alone; the app can't close it, and that's fine.
6. Timeout after 10 min of waiting → back to step 1 with "Didn't see a login. Try again?"

**Storage:** store the session cookies in **Windows Credential Manager** (generic credential, per-user), never in plain files. Re-read cookies from the browser before treating a 401 as "expired". The user may simply have a newer session there, so silent recovery comes first. Show the Reconnect button only if that fails.

**Choosing the org:**
1. The `lastActiveOrg` cookie (the org the user last used on claude.ai), if it's in the list and has `chat` capability.
2. Otherwise the first org with `chat` capability.
3. Never show orgs without `chat` (API-only orgs have no plan limits).
4. If more than one org is eligible, the right-click menu lists them under "Organization" with a check mark; switching re-fetches immediately and doesn't animate a reset.

**Disconnect** (menu) deletes the stored credentials and settings tied to the account and returns to step 1.

**Later: Chrome (v2, not in this build).** A small companion extension: Manifest V3, permissions limited to `cookies` + host `https://claude.ai/*`, ID pinned with a manifest `key`. It sends the claude.ai cookies to the app through **Chrome native messaging**, with the host registered under `HKCU` by the installer, on install, on browser start, and when the session cookie changes. It will be published from the owner's Chrome Web Store account; if the store refuses it, the fallback is a documented manual install (developer mode). The prototype's Chrome path shows the intended UX. v1 only needs to keep the session source swappable.

## 6. Pace and color

For each limit with a `resets_at`:
- window length `W` = 5 h for the session, 7 days for the weekly limit
- window start = `resets_at − W`
- `expected% = clamp((now − start) / W × 100, 0, 100)`
- `diff = used% − expected%`
- color: **red `#fb7185`** if diff > +5 (ahead of pace) · **green `#34d399`** if within ±5 (on track) · **blue `#7dd3fc`** if diff < −5 (room left)
- the **white tick** on the ring sits at `expected%`

Recompute expected% and the colors every 30 s along with the poll, even if the data didn't change. The tick and color move with time.

**Idle session** (`five_hour` is `null`): show 0%, a gray ring (`#5f5f69`), a gray number, no tick, and the text "idle" / "inactive".

## 7. Reset detection

Compare each successful poll with the **previous successful poll in the same app run, for the same org**:

- A limit **reset** if previous `used ≥ 1` and new `used < previous − 0.5`. `null` counts as 0.
- **Planned** if `now ≥ previous.resets_at − 60 s`. Otherwise it's a **surprise** reset, with `early = previous.resets_at − now`.
- **No animation** on the first poll after launch, after an org switch, or after reconnecting, because there's no fair "previous". A reset that happened while the app was closed just shows the new values.
- After an error gap, still compare with the last good poll. A planned reset during an outage is still a planned reset.
- If both limits reset in the same poll, animate both rings and show **one** pill. Wording is a builder call, e.g. "★ Surprise reset · both limits".

Animations (exact motion is in the prototype; match it):
- **Planned (P2, "light sweep"):** a white glint runs along the filled arc (~750 ms). The ring empties (~450 ms) with a slight scale dip and spring back. The number rolls up and out, then 0% rolls in, green. A green ✓ pops after the label and fades after ~3 s. A green pill shows "✓ Session reset · fresh 5 hours" or "✓ New week · weekly limit reset" for ~2.2 s.
- **Surprise (golden burst + gold glow):** the arc and number turn gold with a brightness flash. About 30 gold, orange and white sparks fly out from along the filled arc. The arc collapses. The number blurs out and back in as 0% with a spring scale. The widget border glows gold for ~4 s. A gold pill shows "★ Surprise reset · 2h14 early" for ~2.8 s.
- While a ring animates, data updates to that ring wait until the animation ends.

## 8. The widget: look and behavior

**Size and shape:** about 242 × 150 px. Dark translucent glass: background `rgba(22,22,28,.66)`, backdrop blur, 1 px `rgba(255,255,255,.09)` border, 16 px radius, soft shadow. Use Windows Acrylic or Mica for the backdrop. Font: Inter or Segoe UI Variable.

**Content:** two rings side by side, the 5-hour session on the left and weekly on the right. Each ring is 64 px with a 6 px stroke, round caps, starting at 12 o'clock, on a track of `rgba(255,255,255,.08)`. Under each ring:
- **Number inside the ring:** "62" with a smaller "%", in the pace color.
- **Title:** "5-hour session" / "Weekly" (FR: "Session 5 h" / "Hebdomadaire"). Semi-bold, light gray, **centered exactly** on the ring.
- **Reset line, always white `#e7e7ea`:** "resets 14:00", and for the weekly limit when it's more than 24 h away, "resets Fri 23:59" (FR: "reset à 14:00", "reset ven. 23:59"). Same format at every usage level: no countdown, no red text.
- **Near the limit (≥ 90% on either ring):** a soft red glow around the widget. **No blinking.**
- The number and ring at 100% are just a full red ring. No special text.

**Visible only on hover**, all with Lucide icons and the same 22 × 18 px hit area:
- Top-left: **🇫🇷 / 🇬🇧 flags** (drawn as SVG; flag emojis don't render on Windows). The active flag is fully opaque with an outline. Clicking switches the language instantly.
- Top-right: **minimize "–"** and **close "✕"** (close turns red `#e81123` on hover, as on Windows).
- Bottom-right: **refresh** (Lucide `refresh-cw`). On click: a press (scale to 0.86 and back) and **one spring-eased full turn** of the icon (~900 ms, slight overshoot). No continuous spinning. Ignore clicks during the turn.

**Hover tooltip** on a ring:
- Title line: plan or org name · "updated 12 s ago".
- Then: "62% used · expected 50%", the pace phrase in its color ("12 pts ahead of pace" / "on track" / "16 pts of room"), and "Resets 14:00 (in 2h29)".
- Durations over 24 h read "3d 12h" / "3 j 12 h".
- Idle: "no session running · Starts with your next message".

**Right-click menu:** Refresh now · Organization (only if more than one, with check marks) · Always on top ✓ · Start with Windows ✓ · Disconnect · Quit. All items are translated.

**Window behavior:**
- No native title bar. **Drag from anywhere** except the controls.
- Remember the position. If the saved position is off-screen (for example, a monitor was unplugged), snap it to the nearest corner of the primary screen.
- **Minimize** → hides to the taskbar and keeps polling. Clicking the taskbar icon restores it, or minimizes it if it's showing.
- **Close / Quit** → the process exits and stops polling. Start it again from the Start menu or the desktop shortcut.
- Always on top: on by default. Start with Windows: on by default, via the `HKCU\…\Run` key. Both can be toggled in the menu.
- Single instance: launching it again focuses the existing widget.

**States:**
- **Connected:** the two rings.
- **Idle session:** see §6.
- **Near limit:** red glow.
- **Error:** the rings are grayed and dimmed, and one line underneath reads "⚠ claude.ai not responding · retry in 2 min", counting down.
- **Session expired:** "Session expired · Your claude.ai login ran out. Reconnect to keep tracking." with a [Reconnect] button that restarts the Connect flow.
- **Disconnected / first launch** and **Waiting:** see §5.

Entrance: the rings fill from 0 with a spring when data first arrives.

## 9. Language

- French and English (UK). The default comes from the Windows UI language: French if it's `fr-*`, otherwise English. The choice is saved.
- Use "tu" in French, matching the owner's existing app.
- **Every string** in the widget, tooltip, menu, pills and Connect screens is translated. The full table is in the prototype's `I18N` object; treat it as the starting copy.
- Times use 24 h in both languages. French uses a space before "%" and "h".

## 10. Settings and files

- `%APPDATA%\<AppName>\settings.json`: window position, language, selected org UUID, always on top, start with Windows.
- Credentials: Windows Credential Manager only.
- Logs: `%LOCALAPPDATA%\<AppName>\logs\`, rotating at about 1 MB × 3 files. Log errors with full detail, including the HTTP status and the first 2 KB of an unexpected body. **Never log cookies.** The old app cut errors to 30 characters and printed them to nowhere, which made the latest bug hard to diagnose.

## 11. Open-source packaging

- MIT license. README in **English**, with a French translation, covering:
  - what it is, with a screenshot
  - **"Unofficial, not affiliated with Anthropic"**
  - how the Connect flow works and exactly what data the app reads and where it is stored
  - that it uses an undocumented endpoint and can break
  - the terms-of-service gray area
  - that v1 requires Firefox
  - **the SmartScreen warning:** releases are unsigned, so Windows will show "Windows protected your PC". Say why and how to proceed ("More info" → "Run anyway"), and link to the source and the build instructions.
  - how to build it
- Distributed as a **public GitHub repo with GitHub Releases**: a Windows installer plus a portable zip. **Unsigned** (the owner's decision); keep the CI release step easy to extend with signing later.
- CI: build, tests, and release artifacts on tag.
- Name: **"Claude Limit Checker"**, the owner's choice. Display name only. Internal identifiers use a neutral ID (see §14), so a rename stays cheap if Anthropic ever asks. Must not use the Anthropic or Claude logo, and the README opens with "Unofficial, not affiliated with Anthropic".

## 12. Testing

- **Unit tests** (backend crate) using recorded API responses: normal, `five_hour: null`, `seven_day: null`, missing fields, unknown shape, `limits`-only. Cover pace math at the window edges and reset detection (planned, surprise, both at once, gap after an error, first poll, org switch).
- A **fake claude.ai server** (local, dev-only) that serves the responses on a script, so the UI can be driven through every state, like the prototype's control panel.
- Manual test list for the Connect flow: Firefox already logged in, Firefox logging in, Firefox not the default browser, Firefox not installed, several Firefox profiles, timeout, expired session that recovers silently.

## 13. Decisions made during design (the owner's)

- Rewrite from scratch, open source, multi-user. Windows first, macOS later.
- Connect through the user's real browser; no copy-paste. A Chrome extension is acceptable when Chrome support comes (v2).
- 30 s refresh.
- React with beUI first (Tauri), then move the UI to native Rust.
- Rings layout, with the % after the number. Titles "5-hour session" / "Weekly".
- Pace shown as a white tick.
- Planned reset = P2 light sweep. Surprise reset = golden burst + gold glow.
- All the other states and the Connect flow as shown in the prototype.
- Plan name and "updated X ago" go in the tooltip title, not on the widget.
- Reset line always white, the same format at every level. No blinking.
- FR / EN (UK) flags on the widget.
- Minimize and close on hover, top-right. Refresh at bottom-right, in a modern icon from the same set.
- Close really quits the app.
- **v1 is Firefox only.** Chrome comes in v2 through the companion extension.
- **Name: "Claude Limit Checker".** For now it's a public GitHub repo.
- **Releases are unsigned.** The README explains the SmartScreen warning.

## 14. Calls made by Claude during design (check these)

- Color meaning: blue for "room left" instead of the old orange.
- Error back-off schedule; 401 → expired, 403 + Cloudflare page → error.
- Native messaging for the future Chrome extension (not a localhost port).
- With several Firefox profiles, use the one holding the most recent claude.ai session.
- Org choice via the `lastActiveOrg` cookie, hiding orgs without `chat`.
- Reset threshold of a 0.5-point drop, a 60 s tolerance for "planned", and no animation on the first poll.
- No tray icon. Minimize goes to the taskbar.
- Both "Always on top" and "Start with Windows" on by default.
- Credential Manager for the session, `%APPDATA%` for settings.
- French wording ("Session 5 h", "Hebdomadaire", "reset à", "tu").
- The Phase 2 RAM target (< 25 MB).
- **Plan scope:** the first implementation plan covers **Phase 1 only**. The only Phase 2 work in it is keeping the backend a separate crate behind a state-out / commands-in interface. Phase 2 (toolkit spike, then the native UI) gets its own plan once Phase 1 ships.
- **Identifiers vs display name:** every persistent identifier uses a neutral internal ID with no "Claude" in it. That covers `%APPDATA%` and `%LOCALAPPDATA%` folders, Credential Manager target, `Run` value, native-messaging host name, single-instance mutex and installer product ID. The display name is a separate string, so it can change later without migration code.
- **Repository:** the rewrite lives in this repo, replacing `token_monitor.py`, and keeps its history. The repo is already public. I checked the full history: the real phone number, the org UUID and the owner's email never appear; only `+33XXXXXXXXX` placeholders do. No cleanup is needed.

## 15. Left to the builder

- The Phase 2 native UI toolkit, and how to get Acrylic/Mica.
- Internal architecture beyond the backend/UI split; crate and module layout.
- The installer technology (MSI, NSIS, etc.) and auto-update. Auto-update is optional, and if added it must use GitHub Releases only.
- Exact pill wording for "both limits reset".
- The app's icon design (neutral rings motif, as in the prototype's taskbar icon).
