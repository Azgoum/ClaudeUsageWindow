mod launch;
mod logging;

use limits_core::session::firefox::FirefoxSource;
use limits_core::{
    clamp_position, persisted_position, spawn_engine, AppState, Command, EngineDeps, HttpApi,
    KeyringCreds, PopupRoom, Rect, Settings, SettingsStore, SystemClock,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewWindow,
    WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::{mpsc, watch};
use url::Url;

const MAIN: &str = "main";
const AUTOSTART_NAME: &str = "limitrings";
const MOVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Command sent by the UI: `{type: "connect"}`, `{type: "selectOrg", uuid: "..."}`, ...
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CommandDto {
    Connect,
    CancelConnect,
    Refresh,
    SelectOrg { uuid: String },
    Disconnect,
}

impl From<CommandDto> for Command {
    fn from(dto: CommandDto) -> Self {
        match dto {
            CommandDto::Connect => Command::Connect,
            CommandDto::CancelConnect => Command::CancelConnect,
            CommandDto::Refresh => Command::Refresh,
            CommandDto::SelectOrg { uuid } => Command::SelectOrg(uuid),
            CommandDto::Disconnect => Command::Disconnect,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub language: String,
    pub always_on_top: bool,
    pub start_with_windows: bool,
}

/// Saved `fr`/`en`, else `fr` when the system locale starts with `fr`, else `en`.
pub fn resolve_language(saved: Option<&str>, locale: Option<&str>) -> &'static str {
    match saved {
        Some("fr") => "fr",
        Some("en") => "en",
        _ if locale.is_some_and(|l| l.to_ascii_lowercase().starts_with("fr")) => "fr",
        _ => "en",
    }
}

impl Prefs {
    pub fn from_settings(s: &Settings, locale: Option<&str>) -> Self {
        Self {
            language: resolve_language(s.language.as_deref(), locale).to_string(),
            always_on_top: s.always_on_top,
            start_with_windows: s.start_with_windows,
        }
    }
}

/// Saved position clamped to the monitors; with nothing saved, the primary's top-right
/// (clamp_position's fallback). All values in physical pixels.
pub fn initial_position(
    saved: Option<(i32, i32)>,
    size: (i32, i32),
    monitors: &[Rect],
    primary: Rect,
) -> (i32, i32) {
    match saved {
        Some(pos) => clamp_position(pos, size, monitors, primary),
        // No monitor to be visible on: clamp_position falls back to the primary's top-right.
        None => clamp_position((0, 0), size, &[], primary),
    }
}

/// Debug-only dev switches (release builds ignore every `LIMITRINGS_*` variable).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DevEnv {
    /// `LIMITRINGS_BASE_URL`: API base instead of https://claude.ai.
    pub base_url: Option<Url>,
    /// `LIMITRINGS_FAKE_SESSION=1`: FixedSource + isolated credentials and settings.
    pub fake_session: bool,
    /// `LIMITRINGS_AUTOCONNECT=1`: send Connect at startup.
    pub autoconnect: bool,
    /// `LIMITRINGS_FAKE_NO_FIREFOX=1`: the launcher reports Firefox as not installed.
    pub no_firefox: bool,
    /// `LIMITRINGS_DEV_AUTOSTART=1`: let a debug run register itself at logon (testing only).
    pub autostart: bool,
}

/// Only release builds touch the logon Run value: a debug exe loads the dev server URL, so
/// registering it would open a broken window at every logon (and replace an installed
/// release's entry). A debug run opts in with `LIMITRINGS_DEV_AUTOSTART=1`, never when isolated.
pub fn manages_autostart(debug: bool, dev: &DevEnv) -> bool {
    !debug || (dev.autostart && !dev.fake_session)
}

impl DevEnv {
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Self {
        let on = |k: &str| get(k).is_some_and(|v| v.trim() == "1");
        let base_url = get("LIMITRINGS_BASE_URL").and_then(|v| match Url::parse(v.trim()) {
            Ok(u) => Some(u),
            Err(e) => {
                tracing::warn!("ignoring invalid LIMITRINGS_BASE_URL: {e}");
                None
            }
        });
        Self {
            base_url,
            fake_session: on("LIMITRINGS_FAKE_SESSION"),
            autoconnect: on("LIMITRINGS_AUTOCONNECT"),
            no_firefox: on("LIMITRINGS_FAKE_NO_FIREFOX"),
            autostart: on("LIMITRINGS_DEV_AUTOSTART"),
        }
    }

    fn current() -> Self {
        if cfg!(debug_assertions) {
            Self::from_vars(|k| std::env::var(k).ok())
        } else {
            Self::default()
        }
    }
}

/// `<appdata>\limitrings\settings.json`, or `limitrings-dev` for isolated dev runs.
pub fn settings_path(appdata: &Path, dev: bool) -> PathBuf {
    appdata
        .join(if dev { "limitrings-dev" } else { "limitrings" })
        .join("settings.json")
}

#[cfg(debug_assertions)]
struct FakeLauncher(Arc<limits_core::session::FixedSource>, bool);

#[cfg(debug_assertions)]
impl limits_core::BrowserLauncher for FakeLauncher {
    fn firefox_installed(&self) -> bool {
        !self.1
    }
    fn open_login(&self) -> anyhow::Result<()> {
        tracing::info!("fake session: login opened, session available in 5 s");
        self.0.arm();
        Ok(())
    }
}

/// `settings` must be a clone of the shell's store: clones share the write lock.
fn build_deps(dev: &DevEnv, settings: SettingsStore) -> EngineDeps {
    let api = Arc::new(match &dev.base_url {
        Some(base) => HttpApi::new(base.clone()),
        None => HttpApi::claude(),
    });
    #[cfg(debug_assertions)]
    if dev.fake_session {
        use limits_core::session::{FixedSource, FAKE_ORG_UUID};
        let source = Arc::new(FixedSource::new(FAKE_ORG_UUID));
        return EngineDeps {
            api,
            source: source.clone(),
            creds: Arc::new(KeyringCreds::with_user("session-dev")),
            settings,
            clock: Arc::new(SystemClock),
            browser: Arc::new(FakeLauncher(source, dev.no_firefox)),
        };
    }
    EngineDeps {
        api,
        source: Arc::new(FirefoxSource::new(
            FirefoxSource::default_root().unwrap_or_default(),
        )),
        creds: Arc::new(KeyringCreds::new()),
        settings,
        clock: Arc::new(SystemClock),
        browser: Arc::new(launch::FirefoxLauncher {
            pretend_missing: cfg!(debug_assertions) && dev.no_firefox,
        }),
    }
}

struct Shell {
    cmd: mpsc::Sender<Command>,
    state: watch::Receiver<AppState>,
    /// Same lock as the engine's clone, so shell and engine writes never clobber each other.
    settings: SettingsStore,
    /// See `manages_autostart`: the debug exe must never be registered at logon.
    manage_autostart: bool,
    move_gen: AtomicU64,
    /// Resting position while the window is grown for a tooltip or menu (never persisted).
    room: Mutex<PopupRoom>,
}

impl Shell {
    fn load(&self) -> Settings {
        self.settings.load()
    }

    fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        self.settings.update(f).map(drop).map_err(|e| {
            tracing::warn!("cannot save settings: {e}");
            e.to_string()
        })
    }
}

fn apply_autostart(app: &AppHandle, manage: bool, on: bool) -> Result<(), String> {
    if !manage {
        tracing::info!(on, "dev run: autostart left untouched");
        return Ok(());
    }
    let launcher = app.autolaunch();
    let res = if on {
        launcher.enable()
    } else {
        launcher.disable()
    };
    res.map_err(|e| {
        tracing::warn!(on, "cannot change autostart: {e}");
        e.to_string()
    })
}

#[tauri::command]
fn get_state(shell: State<'_, Shell>) -> AppState {
    shell.state.borrow().clone()
}

#[tauri::command]
async fn send_command(shell: State<'_, Shell>, cmd: CommandDto) -> Result<(), String> {
    shell.cmd.send(cmd.into()).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_prefs(shell: State<'_, Shell>) -> Prefs {
    Prefs::from_settings(&shell.load(), sys_locale::get_locale().as_deref())
}

#[tauri::command]
fn set_language(shell: State<'_, Shell>, lang: String) -> Result<(), String> {
    if lang != "fr" && lang != "en" {
        return Err(format!("unsupported language: {lang}"));
    }
    shell.update(|s| s.language = Some(lang))
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, shell: State<'_, Shell>, on: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(MAIN) {
        w.set_always_on_top(on).map_err(|e| e.to_string())?;
    }
    shell.update(|s| s.always_on_top = on)
}

#[tauri::command]
fn set_start_with_windows(app: AppHandle, shell: State<'_, Shell>, on: bool) -> Result<(), String> {
    apply_autostart(&app, shell.manage_autostart, on)?;
    shell.update(|s| s.start_with_windows = on)
}

/// Sizes the window to the widget (`w`×`h`, CSS px) plus `extra` px of room for an open
/// tooltip or menu (0 = back to rest). Returns how far down (CSS px) the widget must sit
/// inside the window: non-zero when the window grew upward.
#[tauri::command]
fn fit_window(
    window: WebviewWindow,
    shell: State<'_, Shell>,
    w: f64,
    h: f64,
    extra: f64,
) -> Result<f64, String> {
    if !(w >= 1.0 && h >= 1.0 && extra >= 0.0) {
        return Err(format!("bad widget size {w}x{h} (+{extra})"));
    }
    let err = |e: tauri::Error| e.to_string();
    let scale = window.scale_factor().map_err(err)?;
    let px = |v: f64| (v.max(0.0) * scale).ceil() as i32;
    let pos = window.outer_position().map_err(err)?;
    let monitor = match window.current_monitor().map_err(err)? {
        Some(m) => Some(m),
        None => window.primary_monitor().map_err(err)?,
    };
    let Some(monitor) = monitor else {
        return Ok(0.0);
    };
    let b = shell.room.lock().unwrap_or_else(|p| p.into_inner()).apply(
        (pos.x, pos.y),
        (px(w), px(h)),
        px(extra),
        monitor_rect(&monitor),
    );
    if (b.pos.0, b.pos.1) != (pos.x, pos.y) {
        window
            .set_position(PhysicalPosition::new(b.pos.0, b.pos.1))
            .map_err(err)?;
    }
    window
        .set_size(PhysicalSize::new(b.size.0 as u32, b.size.1 as u32))
        .map_err(err)?;
    Ok(b.offset_y as f64 / scale)
}

/// If the normal exit hasn't ended the process after this long (a hung WebView2 teardown),
/// it is ended outright: Quit must never leave an invisible process behind.
const EXIT_FALLBACK: Duration = Duration::from_secs(3);

fn request_exit(app: &AppHandle, from: &'static str) {
    tracing::info!(from, "quit requested");
    app.exit(0);
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_FALLBACK);
        tracing::warn!("exit still pending after 3 s; ending the process");
        std::process::exit(0);
    });
}

#[tauri::command]
fn quit(app: AppHandle) {
    request_exit(&app, "menu");
}

/// Work area (monitor minus taskbar), physical pixels.
fn monitor_rect(m: &tauri::Monitor) -> Rect {
    let a = m.work_area();
    Rect {
        x: a.position.x,
        y: a.position.y,
        w: a.size.width as i32,
        h: a.size.height as i32,
    }
}

fn place_window(w: &WebviewWindow, saved: Option<(i32, i32)>) -> tauri::Result<()> {
    let monitors: Vec<Rect> = w.available_monitors()?.iter().map(monitor_rect).collect();
    let primary = match w.primary_monitor()? {
        Some(m) => monitor_rect(&m),
        None => match monitors.first() {
            Some(r) => *r,
            None => return Ok(()),
        },
    };
    let size = w.outer_size()?;
    let (x, y) = initial_position(
        saved,
        (size.width as i32, size.height as i32),
        &monitors,
        primary,
    );
    w.set_position(PhysicalPosition::new(x, y))
}

fn on_moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    // Moves made while grown for a popup are our own and temporary: they don't restart the
    // debounce, so a pending save from the user's drag still goes through.
    if app
        .state::<Shell>()
        .room
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .expanded()
    {
        return;
    }
    let gen = app.state::<Shell>().move_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MOVE_DEBOUNCE).await;
        let shell = app.state::<Shell>();
        if shell.move_gen.load(Ordering::SeqCst) != gen {
            return;
        }
        // Minimizing reports a (-32000, -32000) move; never persist that.
        let minimized = app
            .get_webview_window(MAIN)
            .and_then(|w| w.is_minimized().ok())
            .unwrap_or(false);
        if !minimized {
            // A popup may have expanded the window since: save its resting position then.
            let room = shell.room.lock().unwrap_or_else(|p| p.into_inner());
            let saved = persisted_position((pos.x, pos.y), &room);
            drop(room);
            let _ = shell.update(|s| s.position = Some(saved));
        }
    });
}

fn forward_state(app: AppHandle, mut rx: watch::Receiver<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut last_conn = None;
        loop {
            let s = rx.borrow_and_update().clone();
            if last_conn.as_ref() != Some(&s.conn) {
                tracing::info!(conn = ?s.conn, "state");
                last_conn = Some(s.conn.clone());
            }
            if let Err(e) = app.emit("state", &s) {
                tracing::warn!("cannot emit state: {e}");
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
    });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dev = DevEnv::current();
    if dev != DevEnv::default() {
        tracing::info!(?dev, "dev switches active");
    }
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    let path = settings_path(Path::new(&appdata), dev.fake_session);
    let first_run = !path.exists();
    let store = SettingsStore::new(path);
    let settings = store.load();
    let manage_autostart = manages_autostart(cfg!(debug_assertions), &dev);

    if first_run {
        tracing::info!("first run: applying defaults");
        let _ = apply_autostart(app.handle(), manage_autostart, settings.start_with_windows);
        if let Err(e) = store.save(&settings) {
            tracing::warn!("cannot save settings: {e}");
        }
    }

    let deps = build_deps(&dev, store.clone());
    let (cmd, state) = tauri::async_runtime::block_on(async move { spawn_engine(deps) });
    if dev.autoconnect {
        tracing::info!("dev autoconnect: sending Connect");
        let _ = cmd.try_send(Command::Connect);
    }
    forward_state(app.handle().clone(), state.clone());
    app.manage(Shell {
        cmd,
        state,
        settings: store,
        manage_autostart,
        move_gen: AtomicU64::new(0),
        room: Mutex::new(PopupRoom::default()),
    });

    let w = app.get_webview_window(MAIN).ok_or("main window missing")?;
    if let Err(e) = place_window(&w, settings.position) {
        tracing::warn!("cannot place window: {e}");
    }
    w.set_always_on_top(settings.always_on_top)?;
    w.show()?;
    Ok(())
}

pub fn run() {
    let log_dir = logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), ?log_dir, "starting");
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::info!("second instance launched: focusing the widget");
            if let Some(w) = app.get_webview_window(MAIN) {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            // The MacosLauncher setting only exists on macOS builds; Windows uses the Run key.
            tauri_plugin_autostart::Builder::new()
                .app_name(AUTOSTART_NAME)
                .build(),
        )
        .setup(setup)
        .on_window_event(|w, e| match e {
            WindowEvent::CloseRequested { .. } => request_exit(w.app_handle(), "close button"),
            WindowEvent::Moved(pos) if w.label() == MAIN => on_moved(w.app_handle(), *pos),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            send_command,
            get_prefs,
            set_language,
            set_always_on_top,
            set_start_with_windows,
            fit_window,
            quit
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|_, e| {
            if let RunEvent::Exit = e {
                tracing::info!("exiting");
            }
        }),
        Err(e) => tracing::error!("tauri runtime failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto(json: &str) -> Command {
        serde_json::from_str::<CommandDto>(json).unwrap().into()
    }

    #[test]
    fn command_dto_deserializes_every_variant() {
        assert_eq!(dto(r#"{"type":"connect"}"#), Command::Connect);
        assert_eq!(dto(r#"{"type":"cancelConnect"}"#), Command::CancelConnect);
        assert_eq!(dto(r#"{"type":"refresh"}"#), Command::Refresh);
        assert_eq!(
            dto(r#"{"type":"selectOrg","uuid":"o-1"}"#),
            Command::SelectOrg("o-1".into())
        );
        assert_eq!(dto(r#"{"type":"disconnect"}"#), Command::Disconnect);
    }

    #[test]
    fn command_dto_rejects_unknown_or_incomplete() {
        assert!(serde_json::from_str::<CommandDto>(r#"{"type":"explode"}"#).is_err());
        assert!(serde_json::from_str::<CommandDto>(r#"{"type":"selectOrg"}"#).is_err());
    }

    #[test]
    fn language_saved_then_locale_then_en() {
        assert_eq!(resolve_language(Some("en"), Some("fr-FR")), "en");
        assert_eq!(resolve_language(Some("fr"), Some("en-GB")), "fr");
        assert_eq!(resolve_language(None, Some("fr-FR")), "fr");
        assert_eq!(resolve_language(None, Some("fr")), "fr");
        assert_eq!(resolve_language(None, Some("en-US")), "en");
        assert_eq!(resolve_language(None, Some("de-DE")), "en");
        assert_eq!(resolve_language(None, None), "en");
        // Junk in the settings file falls through to the locale.
        assert_eq!(resolve_language(Some("xx"), Some("fr-CA")), "fr");
    }

    #[test]
    fn prefs_serialize_camel_case() {
        let s = Settings {
            always_on_top: false,
            ..Settings::default()
        };
        let p = Prefs::from_settings(&s, Some("fr-FR"));
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({"language": "fr", "alwaysOnTop": false, "startWithWindows": false})
        );
    }

    const PRIMARY: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    const SIZE: (i32, i32) = (244, 146);

    #[test]
    fn default_position_is_primary_top_right() {
        let second = Rect {
            x: -1280,
            y: 0,
            w: 1280,
            h: 1024,
        };
        assert_eq!(
            initial_position(None, SIZE, &[second, PRIMARY], PRIMARY),
            (1920 - 244 - 16, 16)
        );
    }

    #[test]
    fn saved_position_kept_or_snapped() {
        assert_eq!(
            initial_position(Some((100, 200)), SIZE, &[PRIMARY], PRIMARY),
            (100, 200)
        );
        assert_eq!(
            initial_position(Some((5000, 5000)), SIZE, &[PRIMARY], PRIMARY),
            (1660, 16)
        );
    }

    #[test]
    fn dev_env_parses_flags() {
        let vars = |k: &str| match k {
            "LIMITRINGS_BASE_URL" => Some("http://127.0.0.1:8787".to_string()),
            "LIMITRINGS_FAKE_SESSION" => Some("1".to_string()),
            "LIMITRINGS_AUTOCONNECT" => Some("0".to_string()),
            "LIMITRINGS_FAKE_NO_FIREFOX" => Some("1".to_string()),
            "LIMITRINGS_DEV_AUTOSTART" => Some("1".to_string()),
            _ => None,
        };
        assert_eq!(
            DevEnv::from_vars(vars),
            DevEnv {
                base_url: Some(Url::parse("http://127.0.0.1:8787").unwrap()),
                fake_session: true,
                autoconnect: false,
                no_firefox: true,
                autostart: true,
            }
        );
        assert_eq!(DevEnv::from_vars(|_| None), DevEnv::default());
        let bad = |k: &str| (k == "LIMITRINGS_BASE_URL").then(|| "not a url".to_string());
        assert_eq!(DevEnv::from_vars(bad).base_url, None);
    }

    #[test]
    fn autostart_managed_in_release_or_on_dev_opt_in() {
        let plain = DevEnv::default();
        let opt_in = DevEnv {
            autostart: true,
            ..DevEnv::default()
        };
        assert!(manages_autostart(false, &plain), "release");
        assert!(
            manages_autostart(false, &opt_in),
            "release ignores the switch"
        );
        assert!(!manages_autostart(true, &plain), "plain debug run");
        assert!(manages_autostart(true, &opt_in), "debug run that opted in");
        let isolated = DevEnv {
            fake_session: true,
            ..opt_in
        };
        assert!(
            !manages_autostart(true, &isolated),
            "isolated dev runs never"
        );
    }

    #[test]
    fn settings_path_isolates_dev_runs() {
        let appdata = Path::new(r"C:\Users\u\AppData\Roaming");
        assert_eq!(
            settings_path(appdata, false),
            PathBuf::from(r"C:\Users\u\AppData\Roaming\limitrings\settings.json")
        );
        assert_eq!(
            settings_path(appdata, true),
            PathBuf::from(r"C:\Users\u\AppData\Roaming\limitrings-dev\settings.json")
        );
    }
}
