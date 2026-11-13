//! Claude Limit Checker: native egui widget.

pub mod anim;
pub mod engine_thread;
pub mod host;
pub mod launch;
pub mod logging;
pub mod platform;
pub mod render;
pub mod shell;
pub mod ui;

use std::path::Path;

use limits_core::{Command, SettingsStore};

/// Window title (display name, see Global Constraints).
pub const TITLE: &str = "Claude Limit Checker";
/// Logical size the window opens with, before the widget reports its own.
pub const WINDOW_SIZE: egui::Vec2 = egui::Vec2::new(244.0, 146.0);
/// Single-instance mutex (Global Constraints).
const INSTANCE_MUTEX: &str = r"Local\limitrings-single-instance";
/// Run value name for "Start with Windows".
pub(crate) const AUTOSTART_NAME: &str = "limitrings";

/// Runs the widget: one instance, the engine on its own thread, the window on
/// this one. Returns when the window closes.
pub fn run() {
    let log_dir = logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), ?log_dir, "starting");
    let dev = shell::DevEnv::current();
    if dev != shell::DevEnv::default() {
        tracing::info!(?dev, "dev switches active");
    }

    let Some(_instance) = platform::single_instance(INSTANCE_MUTEX) else {
        tracing::info!("already running: focusing the widget");
        if !platform::focus_existing(TITLE) {
            tracing::warn!("another instance holds the lock but has no window");
        }
        return;
    };

    let Some(appdata) = std::env::var_os("APPDATA") else {
        tracing::error!("APPDATA is not set");
        return;
    };
    let path = shell::settings_path(Path::new(&appdata), dev.fake_session);
    let first_run = !path.exists();
    let store = SettingsStore::new(path);
    let settings = store.load();
    if first_run {
        tracing::info!("first run: saving the defaults");
        if let Err(e) = store.save(&settings) {
            tracing::warn!("cannot save settings: {e}");
        }
    }
    let manage_autostart = shell::manages_autostart(cfg!(debug_assertions), &dev);
    if manage_autostart && settings.start_with_windows {
        // On every start, so a moved exe is registered where it now is.
        let synced = std::env::current_exe().and_then(|exe| {
            platform::autostart::sync(
                &platform::autostart::RunKey::user(),
                AUTOSTART_NAME,
                true,
                &exe,
            )
        });
        if let Err(e) = synced {
            tracing::warn!("cannot update autostart: {e}");
        }
    }
    let prefs = shell::prefs_from_settings(&settings, sys_locale::get_locale().as_deref());

    let event_loop = host::event_loop();
    let proxy = event_loop.create_proxy();
    let deps = shell::build_deps(&dev, store.clone());
    let (cmd, state, _engine) = engine_thread::start(deps, move || {
        let _ = proxy.send_event(host::Wake);
    });
    if dev.autoconnect {
        tracing::info!("dev autoconnect: sending Connect");
        let _ = cmd.try_send(Command::Connect);
    }
    host::run(
        event_loop,
        host::Shell {
            cmd,
            state,
            settings: store,
            prefs,
            manage_autostart,
            saved_position: settings.position,
        },
    );
    tracing::info!("window closed: exiting");
}
