//! Shell logic: language, position, dev switches, settings path, engine wiring.

use crate::ui::{Lang, Prefs};
use limits_core::session::firefox::FirefoxSource;
use limits_core::{
    clamp_position, EngineDeps, HttpApi, KeyringCreds, Rect, Settings, SettingsStore, SystemClock,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

/// Saved `fr`/`en`, else `fr` when the system locale starts with `fr`, else `en`.
pub fn resolve_language(saved: Option<&str>, locale: Option<&str>) -> Lang {
    match saved {
        Some("fr") => Lang::Fr,
        Some("en") => Lang::En,
        _ if locale.is_some_and(|l| l.to_ascii_lowercase().starts_with("fr")) => Lang::Fr,
        _ => Lang::En,
    }
}

pub fn prefs_from_settings(s: &Settings, locale: Option<&str>) -> Prefs {
    Prefs {
        lang: resolve_language(s.language.as_deref(), locale),
        always_on_top: s.always_on_top,
        start_with_windows: s.start_with_windows,
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

    pub fn current() -> Self {
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
pub fn build_deps(dev: &DevEnv, settings: SettingsStore) -> EngineDeps {
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
        browser: Arc::new(crate::launch::FirefoxLauncher {
            pretend_missing: cfg!(debug_assertions) && dev.no_firefox,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_saved_then_locale_then_en() {
        assert_eq!(resolve_language(Some("en"), Some("fr-FR")), Lang::En);
        assert_eq!(resolve_language(Some("fr"), Some("en-GB")), Lang::Fr);
        assert_eq!(resolve_language(None, Some("fr-FR")), Lang::Fr);
        assert_eq!(resolve_language(None, Some("fr")), Lang::Fr);
        assert_eq!(resolve_language(None, Some("en-US")), Lang::En);
        assert_eq!(resolve_language(None, Some("de-DE")), Lang::En);
        assert_eq!(resolve_language(None, None), Lang::En);
        // Junk in the settings file falls through to the locale.
        assert_eq!(resolve_language(Some("xx"), Some("fr-CA")), Lang::Fr);
    }

    #[test]
    fn prefs_from_settings_resolves_language() {
        let s = Settings {
            always_on_top: false,
            ..Settings::default()
        };
        assert_eq!(
            prefs_from_settings(&s, Some("fr-FR")),
            Prefs {
                lang: Lang::Fr,
                always_on_top: false,
                start_with_windows: false
            }
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
