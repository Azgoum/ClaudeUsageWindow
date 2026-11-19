# Phase 1 reference captures

Captured from the Phase 1 React UI (`ui/`, browser mock, `?mock=<kind>`) before it is deleted.
Chromium via Playwright, `html.tauri` class added so the glass is the shipped `.80` look.
Mock org renamed to "Jane's Organization" (max / ultra) and "Acme Studio" (team); no email appears.
The widget is clipped to `.wd` over the page's dark gradient background (the corners show that
background, not a transparent window). Tooltip and menu captures include the widget plus the popup.

Naming: `<kind>_<lang>[_<extra>]@<scale>x.png`, scale 1 or 1.5 (device scale factor).

## Files

- Base states, `.wd` only: `connected`, `idle`, `ultra`, `near`, `error`, `expired`, `waiting`,
  `disconnected`, `timedout`, `needsFirefox`, `twoOrgs`. `en` and `fr` at 1x for all;
  at 1.5x `en` and `fr` for connected, idle, ultra, near, error, expired, and `en` only for the rest.
- `<kind>_<lang>_tooltip-session` / `_tooltip-weekly`: ring tooltip open (kind = connected, idle,
  ultra). 1x en+fr, 1.5x en only.
- `<kind>_<lang>_menu`: context menu open (kind = connected, twoOrgs, disconnected). 1x en+fr, 1.5x en only.
- `connected_<lang>_hover`, `disconnected_en/fr_hover` (1x only): pointer over the widget, so the
  flags, minimise/close and refresh controls are visible. `connected_en_hover@1.5x` also exists.
- `connected_en_planned_<t>ms@1x` and `connected_en_surprise_<t>ms@1x`: frames after
  `window.__mock.reset('planned' | 'surprise')` at about 300, 900, 1500, 2500 ms
  (actual capture times 361/955/1558/2553 ms and 363/952/1562/2555 ms; screenshot latency adds about 60 ms).
  Clip is the widget plus 10 px margin so glow/pill overflow is visible.

## Not captured

- `fr` for the planned/surprise frames, tooltips/menus at 1.5x `fr`, and `plannedWeekly`,
  `surpriseWeekly`, `both`, `bothPlanned` resets (count kept reasonable).
- Real Tauri window rendering (this is the browser mock, so no OS window frame or desktop behind the glass).
