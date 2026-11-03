use super::{SessionCookies, SessionSource};
use std::path::{Path, PathBuf};

pub struct FirefoxSource {
    root: PathBuf,
}

impl FirefoxSource {
    /// `%APPDATA%\Mozilla\Firefox`.
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Mozilla").join("Firefox"))
    }

    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

/// Profile directories listed in `profiles.ini` (`[Profile*]` sections, honoring `IsRelative`).
pub fn profile_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_profile = false;
    let mut path: Option<String> = None;
    let mut relative = true;
    let flush = |path: &mut Option<String>, relative: &mut bool, out: &mut Vec<PathBuf>| {
        if let Some(p) = path.take() {
            out.push(if *relative { root.join(p) } else { PathBuf::from(p) });
        }
        *relative = true;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            flush(&mut path, &mut relative, &mut out);
            in_profile = line.starts_with("[Profile");
        } else if in_profile {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "Path" => path = Some(v.trim().to_string()),
                    "IsRelative" => relative = v.trim() != "0",
                    _ => {}
                }
            }
        }
    }
    flush(&mut path, &mut relative, &mut out);
    out
}

/// Claude cookies of one profile (deduped by name, greatest `lastAccessed` wins) plus the
/// `lastAccessed` of its `sessionKey` row.
fn read_profile(dir: &Path) -> anyhow::Result<Option<(i64, Vec<(String, String)>)>> {
    let db = dir.join("cookies.sqlite");
    if !db.exists() {
        return Ok(None);
    }
    // TempDir is dropped (and removed) on every return path, including errors.
    let tmp = tempfile::TempDir::new()?;
    std::fs::copy(&db, tmp.path().join("cookies.sqlite"))?;
    let wal = dir.join("cookies.sqlite-wal");
    if wal.exists() {
        std::fs::copy(&wal, tmp.path().join("cookies.sqlite-wal"))?;
    }
    // Read-write on the copy so SQLite can recover the WAL.
    let conn = rusqlite::Connection::open(tmp.path().join("cookies.sqlite"))?;
    let mut stmt = conn.prepare(
        "SELECT name, value, lastAccessed FROM moz_cookies WHERE host IN ('claude.ai','.claude.ai')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0)))
    })?;
    let mut best: Vec<(String, String, i64)> = Vec::new();
    for row in rows {
        let (n, v, l) = row?;
        match best.iter_mut().find(|(bn, _, _)| *bn == n) {
            Some(e) if e.2 < l => *e = (n, v, l),
            Some(_) => {}
            None => best.push((n, v, l)),
        }
    }
    let session = best.iter().find(|(n, v, _)| n == "sessionKey" && !v.is_empty()).map(|e| e.2);
    drop(stmt);
    Ok(session.map(|s| (s, best.into_iter().map(|(n, v, _)| (n, v)).collect())))
}

impl SessionSource for FirefoxSource {
    fn read(&self) -> anyhow::Result<Option<SessionCookies>> {
        let mut winner: Option<(i64, Vec<(String, String)>)> = None;
        for dir in profile_dirs(&self.root) {
            match read_profile(&dir) {
                Ok(Some((last, cookies))) => {
                    if winner.as_ref().map_or(true, |(w, _)| last > *w) {
                        winner = Some((last, cookies));
                    }
                }
                Ok(None) => {}
                Err(_) => tracing::warn!(path = %dir.display(), "skipping unreadable Firefox profile cookies"),
            }
        }
        Ok(winner.map(|(_, cookies)| SessionCookies { cookies }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    const SCHEMA: &str = "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER, creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER)";

    fn insert(conn: &Connection, name: &str, value: &str, host: &str, last: i64) {
        conn.execute(
            "INSERT INTO moz_cookies (name, value, host, path, expiry, lastAccessed, creationTime, isSecure, isHttpOnly) VALUES (?1, ?2, ?3, '/', 0, ?4, ?4, 1, 1)",
            rusqlite::params![name, value, host, last],
        )
        .unwrap();
    }

    fn make_db(dir: &Path, rows: &[(&str, &str, &str, i64)]) {
        std::fs::create_dir_all(dir).unwrap();
        let conn = Connection::open(dir.join("cookies.sqlite")).unwrap();
        conn.execute(SCHEMA, []).unwrap();
        for (n, v, h, l) in rows {
            insert(&conn, n, v, h, *l);
        }
    }

    fn write_ini(root: &Path, profiles: &[(&str, bool)]) {
        let mut s = String::from("[General]\nStartWithLastProfile=1\n\n");
        for (i, (p, rel)) in profiles.iter().enumerate() {
            s.push_str(&format!(
                "[Profile{i}]\nName=p{i}\nIsRelative={}\nPath={p}\n\n",
                if *rel { 1 } else { 0 }
            ));
        }
        std::fs::write(root.join("profiles.ini"), s).unwrap();
    }

    fn get<'a>(c: &'a SessionCookies, name: &str) -> Option<&'a str> {
        c.cookies.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    #[test]
    fn parses_relative_and_absolute_profiles() {
        let root = TempDir::new().unwrap();
        let abs = TempDir::new().unwrap();
        let abs_str = abs.path().to_string_lossy().to_string();
        write_ini(root.path(), &[("Profiles/abc.default", true), (&abs_str, false)]);
        let dirs = profile_dirs(root.path());
        assert_eq!(dirs, vec![root.path().join("Profiles/abc.default"), abs.path().to_path_buf()]);
    }

    #[test]
    fn reads_claude_cookies_only() {
        let root = TempDir::new().unwrap();
        write_ini(root.path(), &[("Profiles/a", true)]);
        make_db(
            &root.path().join("Profiles/a"),
            &[
                ("sessionKey", "sk", ".claude.ai", 10),
                ("lastActiveOrg", "org1", "claude.ai", 10),
                ("tracker", "t", ".example.com", 10),
            ],
        );
        let c = FirefoxSource::new(root.path().to_path_buf()).read().unwrap().unwrap();
        assert!(c.has_session());
        assert_eq!(c.last_active_org(), Some("org1"));
        assert_eq!(c.cookies.len(), 2);
        assert!(get(&c, "tracker").is_none());
    }

    #[test]
    fn picks_profile_with_most_recent_session() {
        let root = TempDir::new().unwrap();
        write_ini(root.path(), &[("Profiles/old", true), ("Profiles/new", true)]);
        make_db(&root.path().join("Profiles/old"), &[("sessionKey", "OLD", ".claude.ai", 100)]);
        make_db(&root.path().join("Profiles/new"), &[("sessionKey", "NEW", ".claude.ai", 200)]);
        let c = FirefoxSource::new(root.path().to_path_buf()).read().unwrap().unwrap();
        assert_eq!(get(&c, "sessionKey"), Some("NEW"));
    }

    #[test]
    fn no_session_returns_none() {
        let root = TempDir::new().unwrap();
        write_ini(root.path(), &[("Profiles/a", true)]);
        make_db(&root.path().join("Profiles/a"), &[("lastActiveOrg", "o", ".claude.ai", 5)]);
        assert!(FirefoxSource::new(root.path().to_path_buf()).read().unwrap().is_none());
    }

    #[test]
    fn missing_root_returns_none() {
        let root = TempDir::new().unwrap();
        let missing = root.path().join("nope");
        assert!(FirefoxSource::new(missing).read().unwrap().is_none());
    }

    #[test]
    fn dedupes_by_name_keeping_latest() {
        let root = TempDir::new().unwrap();
        write_ini(root.path(), &[("Profiles/a", true)]);
        make_db(
            &root.path().join("Profiles/a"),
            &[
                ("sessionKey", "stale", ".claude.ai", 1),
                ("sessionKey", "fresh", "claude.ai", 9),
            ],
        );
        let c = FirefoxSource::new(root.path().to_path_buf()).read().unwrap().unwrap();
        assert_eq!(c.cookies.len(), 1);
        assert_eq!(get(&c, "sessionKey"), Some("fresh"));
    }

    #[test]
    fn reads_cookie_only_present_in_wal() {
        let root = TempDir::new().unwrap();
        write_ini(root.path(), &[("Profiles/a", true)]);
        let dir = root.path().join("Profiles/a");
        std::fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open(dir.join("cookies.sqlite")).unwrap();
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
        conn.query_row("PRAGMA wal_autocheckpoint=0", [], |_| Ok(())).unwrap();
        conn.execute(SCHEMA, []).unwrap();
        insert(&conn, "sessionKey", "walval", ".claude.ai", 7);
        // `conn` stays open: Firefox holding the DB.
        assert!(dir.join("cookies.sqlite-wal").exists());
        let c = FirefoxSource::new(root.path().to_path_buf()).read().unwrap().unwrap();
        assert_eq!(get(&c, "sessionKey"), Some("walval"));
        drop(conn);
    }

    #[test]
    #[ignore]
    fn real_firefox_smoke() {
        let root = FirefoxSource::default_root().expect("APPDATA not set");
        let r = FirefoxSource::new(root).read().expect("read failed");
        match r {
            Some(c) => {
                let names: Vec<&str> = c.cookies.iter().map(|(n, _)| n.as_str()).collect();
                println!("has_session: {}", c.has_session());
                println!("cookie names: {names:?}");
            }
            None => println!("has_session: false (no claude.ai session found)"),
        }
    }
}
