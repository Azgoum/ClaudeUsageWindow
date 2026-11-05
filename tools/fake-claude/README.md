# fake-claude

Dev-only scripted fake of claude.ai's usage endpoints. Node 22, no dependencies.

    node tools/fake-claude/server.mjs [--port 8787]

Debug builds of the Tauri shell honor:

- `LIMITRINGS_BASE_URL=http://127.0.0.1:8787` - `HttpApi` uses this base.
- `LIMITRINGS_FAKE_SESSION=1` - `FixedSource` returns a fake session 5 s after login is opened.
- `LIMITRINGS_DEV_AUTOSTART=1` - let a debug run manage Start with Windows (off by default: a
  debug exe registered at logon would open a broken window; release builds always manage it).

Endpoints: `GET /api/organizations`, `GET /api/organizations/:uuid/usage` (any cookie accepted),
`GET /_ctl` (control page), `GET /_ctl/state` (scripted state as JSON), `POST /_ctl/:action`.

Initial state: session 62 % resetting in 2 h 30, weekly 34 % resetting in 3 d 12 h.
A window past its `resets_at` (or reset by an action) is reported as `null` until "use Claude"
starts a fresh one (5 h / 7 d).

## Actions (`POST /_ctl/<action>`)

| action | effect |
| --- | --- |
| `use-claude[?n=6]` | toggle +N %/min on the session (weekly gets N/10), applied lazily per GET, clamped at 100 |
| `jump` | session `resets_at` = now + 45 s |
| `surprise-session` / `surprise-weekly` / `surprise-both` | window becomes `null` immediately |
| `go-92` | session utilization = 92 |
| `fail-503` | next 3 `/api` requests return 503 |
| `next-401` | next `/api` request returns 401 |
| `next-403` | next `/api` request returns a Cloudflare "Just a moment" 403 |
| `unknown` | next `/api` request returns 200 `{"hello":"world"}` |
| `orgs-1` / `orgs-2` / `toggle-orgs` | list 1 or 2 chat orgs (an API-only org is always listed) |

Org UUIDs are fixed constants; the first (`0b5a1f6e-3c2d-4e8a-9a41-7d6c5b4a3f21`) equals
`FAKE_ORG_UUID` in `crates/limits-core/src/session/mod.rs`.
