# Claude Limit Checker

English | [Français](README.fr.md)

> **Unofficial, not affiliated with Anthropic.** This is an independent hobby project.

![Claude Limit Checker showing the 5-hour session and weekly rings](docs/screenshot.png)

A small always-on-top Windows widget that shows your claude.ai usage limits at a glance.

## What it shows

Two rings, refreshed every 30 seconds:

- **5-hour session**: how much of the current 5-hour window you have used, and when it resets.
- **Weekly**: how much of your 7-day limit you have used, and when it resets.

The white **tick** on each ring marks where you would be if you used your allowance at an even pace
until the reset. The ring color compares your actual usage to that pace:

| Color | Meaning |
| --- | --- |
| Red | Ahead of pace: you are using it faster than an even pace (more than 5 points ahead) |
| Green | On track (within 5 points of the tick) |
| Blue | Room left: you are behind pace (more than 5 points under the tick) |
| Gray | Idle: no session window is currently running |

## Requirements

- **Windows 10 or 11**
- **Firefox**, signed in to claude.ai. Firefox is the only supported browser for now.

## Install

Download the installer (`.exe`) from the [Releases page](https://github.com/Azgoum/ClaudeUsageWindow/releases)
and run it.

Releases are **not code-signed**: a code-signing certificate costs money every year, and this is a free
open-source side project. Windows SmartScreen will therefore warn you ("Windows protected your PC").
Click **More info**, then **Run anyway**. If you prefer, [build it from source](#build-from-source) and
check the code yourself.

## Connect

On first launch, click **Connect**. The app opens Firefox on claude.ai if needed and waits for you to be
signed in.

Exactly what is read: the **claude.ai cookies from your Firefox profile** (the `cookies.sqlite` file, copied
to a temporary file because Firefox locks it). Nothing else in your profile is touched. The cookies are
then stored in **Windows Credential Manager**, never in a plain file, and are **sent only to claude.ai**.
"Disconnect" in the menu deletes them.

## Heads up: undocumented endpoint, terms of service

Claude Limit Checker uses an **undocumented claude.ai endpoint**, the same one the usage page in
your settings uses. It can change or break at any time without notice. Using your browser session
from a third-party app also sits in a **gray area of Anthropic's terms of service**. Use it at your
own risk.

## Privacy

No telemetry, no analytics, no accounts. The only network traffic goes to `https://claude.ai`.
Settings live in `%APPDATA%\limitrings\`, logs in `%LOCALAPPDATA%\limitrings\logs\` (cookie values are
never logged).

## Build from source

Requirements: Rust (MSVC toolchain), Node 22, and `cargo install tauri-cli --version "^2" --locked`.

```sh
cd ui && npm ci && cd ..
cargo tauri build      # installer in target/release/bundle/nsis/
```

Tests: `cargo test --workspace`, and `npm test -- --run` plus `npx tsc --noEmit` in `ui/`.

For development without a real claude.ai account, run the scripted fake server
(see [tools/fake-claude/README.md](tools/fake-claude/README.md)):

```sh
node tools/fake-claude/server.mjs
```

## License

[MIT](LICENSE) © 2026 Aziz Goumiri
