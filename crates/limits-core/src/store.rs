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
            start_with_windows: false,
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
        Self {
            path,
            lock: Arc::default(),
        }
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

/// Named secret blobs (one Windows Credential Manager entry each). A missing name is
/// `Ok(None)` on `get` and a no-op on `delete`.
pub trait Blobs: Send + Sync {
    fn get(&self, name: &str) -> anyhow::Result<Option<String>>;
    fn set(&self, name: &str, value: &str) -> anyhow::Result<()>;
    fn delete(&self, name: &str) -> anyhow::Result<()>;
}

/// Windows Credential Manager entries under the service `limitrings`.
pub struct KeyringBlobs;

const SERVICE: &str = "limitrings";

impl Blobs for KeyringBlobs {
    fn get(&self, name: &str) -> anyhow::Result<Option<String>> {
        match keyring::Entry::new(SERVICE, name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn set(&self, name: &str, value: &str) -> anyhow::Result<()> {
        Ok(keyring::Entry::new(SERVICE, name)?.set_password(value)?)
    }
    fn delete(&self, name: &str) -> anyhow::Result<()> {
        match keyring::Entry::new(SERVICE, name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Credentials as JSON in a head blob `<user>`. One Credential Manager blob holds at most 2560
/// UTF-16 bytes, less than a real cookie set, so longer JSON goes to chunk blobs
/// `<user>.<gen>.<i>` and the head holds `chunked:<gen>:<n>`. A save writes a whole new
/// generation (1 or 2), then flips the head, then deletes every other chunk, so a crash or a
/// failed write at any point leaves the previous credentials loadable. Older heads still load:
/// plain JSON, and `chunked:<n>` with chunks `<user>.<i>`.
pub struct ChunkedCreds<B: Blobs> {
    blobs: B,
    user: String,
}

pub type KeyringCreds = ChunkedCreds<KeyringBlobs>;

const CHUNK_CHARS: usize = 600;
/// Upper bound on chunks per generation (38 400 chars), also the bound of cleanup probing.
const MAX_CHUNKS: usize = 64;
const CHUNKED: &str = "chunked:";
const GENS: [u8; 2] = [1, 2];

/// What the head blob says.
#[derive(Debug, PartialEq)]
enum Head {
    Plain(String),
    /// `chunked:<n>`: chunks `<user>.<i>` (first chunked format).
    Legacy(usize),
    /// `chunked:<gen>:<n>`: chunks `<user>.<gen>.<i>`.
    Gen(u8, usize),
    Corrupt,
}

fn parse_head(head: String) -> Head {
    let Some(rest) = head.strip_prefix(CHUNKED) else {
        return Head::Plain(head);
    };
    let count = |n: &str| {
        n.parse::<usize>()
            .ok()
            .filter(|n| (1..=MAX_CHUNKS).contains(n))
    };
    let parsed = match rest.split_once(':') {
        None => count(rest).map(Head::Legacy),
        Some((g, n)) => match (g.parse::<u8>().ok().filter(|g| GENS.contains(g)), count(n)) {
            (Some(g), Some(n)) => Some(Head::Gen(g, n)),
            _ => None,
        },
    };
    parsed.unwrap_or(Head::Corrupt)
}

impl KeyringCreds {
    pub fn new() -> Self {
        Self::with_user("session")
    }
    pub fn with_user(user: &str) -> Self {
        ChunkedCreds::with_blobs(KeyringBlobs, user)
    }
}

impl Default for KeyringCreds {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Blobs> ChunkedCreds<B> {
    pub fn with_blobs(blobs: B, user: &str) -> Self {
        Self {
            blobs,
            user: user.to_string(),
        }
    }

    fn chunk_name(&self, gen: Option<u8>, i: usize) -> String {
        match gen {
            Some(g) => format!("{}.{g}.{i}", self.user),
            None => format!("{}.{i}", self.user),
        }
    }

    fn head(&self) -> anyhow::Result<Option<Head>> {
        Ok(self.blobs.get(&self.user)?.map(parse_head))
    }

    /// Concatenated chunks; `Ok(None)` when one is missing.
    fn read_chunks(&self, gen: Option<u8>, n: usize) -> anyhow::Result<Option<String>> {
        let mut out = String::new();
        for i in 0..n {
            match self.blobs.get(&self.chunk_name(gen, i))? {
                Some(part) => out.push_str(&part),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    fn read_raw(&self) -> anyhow::Result<Option<String>> {
        let raw = match self.head()? {
            None => return Ok(None),
            Some(Head::Plain(json)) => return Ok(Some(json)),
            Some(Head::Legacy(n)) => self.read_chunks(None, n)?,
            Some(Head::Gen(g, n)) => self.read_chunks(Some(g), n)?,
            Some(Head::Corrupt) => {
                tracing::warn!("stored credential header is corrupt; ignoring");
                return Ok(None);
            }
        };
        if raw.is_none() {
            tracing::warn!("a stored credential chunk is missing; ignoring the stored session");
        }
        Ok(raw)
    }

    /// Deletes every chunk name (both generations and the legacy layout), all indices up to the
    /// cap, except generation `keep`. Bounded: never trusts a count read from the store.
    fn purge_chunks(&self, keep: Option<u8>) -> anyhow::Result<()> {
        let mut first_err = None;
        let gens = [None, Some(GENS[0]), Some(GENS[1])];
        for gen in gens.into_iter().filter(|g| g.is_none() || *g != keep) {
            for i in 0..MAX_CHUNKS {
                if let Err(e) = self.blobs.delete(&self.chunk_name(gen, i)) {
                    first_err.get_or_insert(e);
                }
            }
        }
        first_err.map_or(Ok(()), Err)
    }
}

impl<B: Blobs> CredStore for ChunkedCreds<B> {
    fn load(&self) -> Option<SessionCookies> {
        let raw = match self.read_raw() {
            Ok(Some(raw)) => raw,
            Ok(None) => return None,
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
        let chars: Vec<char> = json.chars().collect();
        if chars.len() <= CHUNK_CHARS {
            self.blobs.set(&self.user, &json)?;
            if let Err(e) = self.purge_chunks(None) {
                tracing::warn!("cannot delete old credential chunks: {e}");
            }
            return Ok(());
        }
        let chunks: Vec<String> = chars
            .chunks(CHUNK_CHARS)
            .map(|c| c.iter().collect())
            .collect();
        anyhow::ensure!(chunks.len() <= MAX_CHUNKS, "credentials too large to store");
        // The generation not in use, so the current one stays intact until the head flips.
        let current = match self.head() {
            Ok(Some(Head::Gen(g, _))) => Some(g),
            _ => None,
        };
        let gen = if current == Some(GENS[0]) {
            GENS[1]
        } else {
            GENS[0]
        };
        let write = || -> anyhow::Result<()> {
            for (i, chunk) in chunks.iter().enumerate() {
                self.blobs.set(&self.chunk_name(Some(gen), i), chunk)?;
            }
            self.blobs
                .set(&self.user, &format!("{CHUNKED}{gen}:{}", chunks.len()))
        };
        if let Err(e) = write() {
            // Drop the partial new generation; the head still points at the previous one.
            for i in 0..MAX_CHUNKS {
                let _ = self.blobs.delete(&self.chunk_name(Some(gen), i));
            }
            return Err(e);
        }
        if let Err(e) = self.purge_chunks(Some(gen)) {
            tracing::warn!("cannot delete old credential chunks: {e}");
        }
        Ok(())
    }

    fn clear(&self) -> anyhow::Result<()> {
        // Head first: from here on nothing loads, even if the chunk cleanup is interrupted.
        self.blobs.delete(&self.user)?;
        self.purge_chunks(None)
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
        SessionCookies {
            cookies: vec![("sessionKey".into(), "sk-secret".into())],
        }
    }

    #[test]
    fn settings_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let s = store.load();
        assert_eq!(s, Settings::default());
        assert!(s.always_on_top && !s.start_with_windows);
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
    fn settings_partial_file_keeps_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"en"}"#).unwrap();
        let s = SettingsStore::new(path).load();
        assert_eq!(s.language.as_deref(), Some("en"));
        assert!(s.always_on_top);
        assert!(!s.start_with_windows);
    }

    #[test]
    fn existing_file_keeps_start_with_windows_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"start_with_windows":true}"#).unwrap();
        assert!(SettingsStore::new(path).load().start_with_windows);
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

    // ---------- chunked credentials, on an in-memory blob map ----------

    #[derive(Default)]
    struct MemBlobs {
        map: Mutex<std::collections::BTreeMap<String, String>>,
        /// Fail the k-th `set` counted from `fail_on_set`.
        fail_set_at: Mutex<Option<usize>>,
        sets: Mutex<usize>,
    }

    impl MemBlobs {
        fn names(&self) -> Vec<String> {
            self.map.lock().unwrap().keys().cloned().collect()
        }
        fn put(&self, name: &str, v: &str) {
            self.map.lock().unwrap().insert(name.into(), v.into());
        }
        fn fail_on_set(&self, k: usize) {
            *self.sets.lock().unwrap() = 0;
            *self.fail_set_at.lock().unwrap() = Some(k);
        }
    }

    impl Blobs for &MemBlobs {
        fn get(&self, name: &str) -> anyhow::Result<Option<String>> {
            Ok(self.map.lock().unwrap().get(name).cloned())
        }
        fn set(&self, name: &str, value: &str) -> anyhow::Result<()> {
            let mut n = self.sets.lock().unwrap();
            *n += 1;
            if *self.fail_set_at.lock().unwrap() == Some(*n) {
                anyhow::bail!("simulated write failure");
            }
            assert!(
                value.encode_utf16().count() <= 1280,
                "blob over the Credential Manager limit"
            );
            self.map.lock().unwrap().insert(name.into(), value.into());
            Ok(())
        }
        fn delete(&self, name: &str) -> anyhow::Result<()> {
            self.map.lock().unwrap().remove(name);
            Ok(())
        }
    }

    fn big(tag: char) -> SessionCookies {
        SessionCookies {
            cookies: vec![
                ("sessionKey".into(), tag.to_string().repeat(1800)),
                ("other".into(), "é".repeat(1500)),
            ],
        }
    }

    #[test]
    fn chunked_small_roundtrip_uses_one_blob() {
        let m = MemBlobs::default();
        let k = ChunkedCreds::with_blobs(&m, "session");
        assert_eq!(k.load(), None);
        k.save(&cookies()).unwrap();
        assert_eq!(k.load(), Some(cookies()));
        assert_eq!(m.names(), vec!["session"]);
    }

    #[test]
    fn chunked_legacy_plain_head_reads_back() {
        let m = MemBlobs::default();
        m.put("session", &serde_json::to_string(&cookies()).unwrap());
        assert_eq!(
            ChunkedCreds::with_blobs(&m, "session").load(),
            Some(cookies())
        );
    }

    #[test]
    fn chunked_legacy_unversioned_chunks_read_back_and_are_replaced() {
        let m = MemBlobs::default();
        let j: Vec<char> = serde_json::to_string(&big('a')).unwrap().chars().collect();
        let parts: Vec<String> = j.chunks(600).map(|c| c.iter().collect()).collect();
        for (i, p) in parts.iter().enumerate() {
            m.put(&format!("session.{i}"), p);
        }
        m.put("session", &format!("chunked:{}", parts.len()));
        let k = ChunkedCreds::with_blobs(&m, "session");
        assert_eq!(k.load(), Some(big('a')));
        k.save(&big('b')).unwrap();
        assert_eq!(k.load(), Some(big('b')));
        let names = m.names();
        assert!(
            names
                .iter()
                .all(|n| n == "session" || n.starts_with("session.1.")),
            "{names:?}"
        );
    }

    #[test]
    fn chunked_grow_then_shrink_leaves_no_extra_names() {
        let m = MemBlobs::default();
        let k = ChunkedCreds::with_blobs(&m, "session");
        k.save(&big('a')).unwrap();
        assert_eq!(k.load(), Some(big('a')));
        let first = m.names();
        assert!(first.len() > 2, "a large set is chunked: {first:?}");
        k.save(&big('b')).unwrap();
        assert_eq!(k.load(), Some(big('b')));
        assert_eq!(
            m.names().len(),
            first.len(),
            "previous generation deleted: {:?}",
            m.names()
        );
        k.save(&cookies()).unwrap();
        assert_eq!(k.load(), Some(cookies()));
        assert_eq!(m.names(), vec!["session"]);
    }

    #[test]
    fn chunked_failure_mid_write_keeps_previous_credentials() {
        let m = MemBlobs::default();
        let k = ChunkedCreds::with_blobs(&m, "session");
        k.save(&big('a')).unwrap();
        let before = m.names();
        m.fail_on_set(3);
        assert!(k.save(&big('b')).is_err());
        assert_eq!(k.load(), Some(big('a')));
        assert_eq!(m.names(), before, "partial new generation cleaned up");
        // Same when the previous set fit in one blob.
        k.save(&cookies()).unwrap();
        m.fail_on_set(3);
        assert!(k.save(&big('c')).is_err());
        assert_eq!(k.load(), Some(cookies()));
        assert_eq!(m.names(), vec!["session"]);
    }

    #[test]
    fn chunked_clear_removes_orphans() {
        let m = MemBlobs::default();
        let k = ChunkedCreds::with_blobs(&m, "session");
        k.save(&big('a')).unwrap();
        for i in 7..10 {
            m.put(&format!("session.{i}"), "orphan");
            m.put(&format!("session.1.{i}"), "orphan");
            m.put(&format!("session.2.{i}"), "orphan");
        }
        k.clear().unwrap();
        assert!(m.names().is_empty(), "{:?}", m.names());
        assert_eq!(k.load(), None);
        k.clear().unwrap();
    }

    #[test]
    fn chunked_missing_chunk_is_none() {
        let m = MemBlobs::default();
        let k = ChunkedCreds::with_blobs(&m, "session");
        k.save(&big('a')).unwrap();
        let chunk = m.names().into_iter().find(|n| n.ends_with(".1")).unwrap();
        m.map.lock().unwrap().remove(&chunk);
        assert_eq!(k.load(), None);
    }

    #[test]
    fn chunked_corrupt_head_is_none_and_clear_is_bounded() {
        let heads = [
            "chunked:999999",
            "chunked:4000000000",
            "chunked:1:999999",
            "chunked:9:2",
            "chunked:x",
            "{not json",
        ];
        for head in heads {
            let m = MemBlobs::default();
            m.put("session", head);
            let k = ChunkedCreds::with_blobs(&m, "session");
            assert_eq!(k.load(), None, "{head}");
            k.clear().unwrap();
            assert!(m.names().is_empty(), "{head}");
        }
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

    #[test]
    #[ignore = "touches the real Windows Credential Manager"]
    fn keyring_roundtrip_large_cookie_set_beyond_one_entry_limit() {
        // A real claude.ai cookie set is longer than the 2560-byte credential blob limit.
        let big = SessionCookies {
            cookies: vec![
                ("sessionKey".into(), "a".repeat(1800)),
                ("other".into(), "b".repeat(1500)),
            ],
        };
        let k = KeyringCreds::with_user("session-test-large");
        k.clear().unwrap();
        k.save(&big).unwrap();
        assert_eq!(k.load(), Some(big));
        // Shrinking leaves no stale chunks behind.
        k.save(&cookies()).unwrap();
        assert_eq!(k.load(), Some(cookies()));
        k.clear().unwrap();
        assert_eq!(k.load(), None);
    }
}
