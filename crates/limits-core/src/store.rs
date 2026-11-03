use crate::session::SessionCookies;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub position: Option<(i32, i32)>,
    pub language: Option<String>,
    pub org_uuid: Option<String>,
    pub always_on_top: bool,
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: None,
            language: None,
            org_uuid: None,
            always_on_top: true,
            start_with_windows: true,
        }
    }
}

/// Clones share one lock, so every writer of the file must use the same store (or a clone).
#[derive(Clone)]
pub struct SettingsStore {
    pub path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Arc::default() }
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        // The lock protects no data, so a poisoned lock is still usable.
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// load -> `f` -> save under the shared lock, so concurrent writers never lose fields;
    /// returns the saved settings.
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> std::io::Result<Settings> {
        let _g = self.guard();
        let mut s = self.load_unlocked();
        f(&mut s);
        self.save_unlocked(&s)?;
        Ok(s)
    }

    /// Missing or corrupt file gives defaults (corrupt also warns).
    pub fn load(&self) -> Settings {
        let _g = self.guard();
        self.load_unlocked()
    }

    fn load_unlocked(&self) -> Settings {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!("cannot read settings file: {e}");
                }
                return Settings::default();
            }
        };
        serde_json::from_str(&raw).unwrap_or_else(|e| {
            tracing::warn!("corrupt settings file, using defaults: {e}");
            Settings::default()
        })
    }

    /// Writes to `<path>.tmp` then renames over the target; creates the parent dir.
    /// Prefer `update` for read-modify-write.
    pub fn save(&self, s: &Settings) -> std::io::Result<()> {
        let _g = self.guard();
        self.save_unlocked(s)
    }

    fn save_unlocked(&self, s: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

pub trait CredStore: Send + Sync {
    fn load(&self) -> Option<SessionCookies>;
    fn save(&self, c: &SessionCookies) -> anyhow::Result<()>;
    fn clear(&self) -> anyhow::Result<()>;
}

pub struct KeyringCreds {
    user: String,
}

const SERVICE: &str = "limitrings";

impl KeyringCreds {
    fn entry(&self) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, &self.user)
    }

    pub fn new() -> Self {
        Self::with_user("session")
    }
    pub fn with_user(user: &str) -> Self {
        Self { user: user.to_string() }
    }
}

impl Default for KeyringCreds {
    fn default() -> Self {
        Self::new()
    }
}

impl CredStore for KeyringCreds {
    fn load(&self) -> Option<SessionCookies> {
        let raw = match self.entry().and_then(|e| e.get_password()) {
            Ok(raw) => raw,
            Err(keyring::Error::NoEntry) => return None,
            Err(e) => {
                tracing::warn!("credential store read failed: {e}");
                return None;
            }
        };
        match serde_json::from_str(&raw) {
            Ok(c) => Some(c),
            Err(_) => {
                tracing::warn!("stored credentials are not valid JSON; ignoring");
                None
            }
        }
    }
    fn save(&self, c: &SessionCookies) -> anyhow::Result<()> {
        let json = serde_json::to_string(c)?;
        self.entry()?.set_password(&json)?;
        Ok(())
    }
    fn clear(&self) -> anyhow::Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Default)]
pub struct MemCreds(Mutex<Option<SessionCookies>>);

impl CredStore for MemCreds {
    fn load(&self) -> Option<SessionCookies> {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, c: &SessionCookies) -> anyhow::Result<()> {
        *self.0.lock().unwrap() = Some(c.clone());
        Ok(())
    }
    fn clear(&self) -> anyhow::Result<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookies() -> SessionCookies {
        SessionCookies { cookies: vec![("sessionKey".into(), "sk-secret".into())] }
    }

    #[test]
    fn settings_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let s = store.load();
        assert_eq!(s, Settings::default());
        assert!(s.always_on_top && s.start_with_windows);
        assert_eq!(s.position, None);
    }

    #[test]
    fn settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("nested").join("settings.json"));
        let s = Settings {
            position: Some((-10, 42)),
            language: Some("fr".into()),
            org_uuid: Some("abc".into()),
            always_on_top: false,
            start_with_windows: false,
        };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
        assert!(!dir.path().join("nested").join("settings.json.tmp").exists());
    }

    #[test]
    fn concurrent_updates_from_clones_all_survive() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let store = store.clone();
                std::thread::spawn(move || {
                    for i in 0..50 {
                        store
                            .update(|s| match t {
                                0 => s.position = Some((i, i)),
                                1 => s.language = Some(format!("l{i}")),
                                2 => s.org_uuid = Some(format!("o{i}")),
                                _ => s.always_on_top = i % 2 == 0,
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let s = store.load();
        assert_eq!(s.position, Some((49, 49)));
        assert_eq!(s.language.as_deref(), Some("l49"));
        assert_eq!(s.org_uuid.as_deref(), Some("o49"));
        assert!(!s.always_on_top);
    }

    #[test]
    fn settings_corrupt_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(SettingsStore::new(path).load(), Settings::default());
    }

    #[test]
    fn settings_partial_file_keeps_true_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"en"}"#).unwrap();
        let s = SettingsStore::new(path).load();
        assert_eq!(s.language.as_deref(), Some("en"));
        assert!(s.always_on_top);
        assert!(s.start_with_windows);
    }

    #[test]
    fn memcreds_roundtrip_and_clear() {
        let m = MemCreds::default();
        assert_eq!(m.load(), None);
        m.save(&cookies()).unwrap();
        assert_eq!(m.load(), Some(cookies()));
        m.clear().unwrap();
        assert_eq!(m.load(), None);
    }

    #[test]
    #[ignore = "touches the real Windows Credential Manager"]
    fn keyring_roundtrip() {
        let k = KeyringCreds::with_user("session-test");
        k.clear().unwrap();
        assert_eq!(k.load(), None);
        k.save(&cookies()).unwrap();
        assert_eq!(k.load(), Some(cookies()));
        k.clear().unwrap();
        assert_eq!(k.load(), None);
        k.clear().unwrap();
    }
}
