use file_rotate::compression::Compression;
use file_rotate::suffix::AppendCount;
use file_rotate::{ContentLimit, FileRotate};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

const MAX_BYTES: usize = 1_000_000;
/// Archived files kept besides the live one: 3 files in total.
const ARCHIVES: usize = 2;

/// `<local_appdata>\limitrings\logs`.
pub fn log_dir(local_appdata: &Path) -> PathBuf {
    local_appdata.join("limitrings").join("logs")
}

/// Size-rotated writer for `<dir>\limitrings.log`.
pub fn rotating_writer(dir: &Path) -> FileRotate<AppendCount> {
    FileRotate::new(
        dir.join("limitrings.log"),
        AppendCount::new(ARCHIVES),
        ContentLimit::Bytes(MAX_BYTES),
        Compression::None,
    )
}

/// Installs the global subscriber: `debug` in debug builds, `info` in release.
/// Returns the log directory, or `None` when `%LOCALAPPDATA%` is unknown (no file logging).
pub fn init() -> Option<PathBuf> {
    let dir = log_dir(&PathBuf::from(std::env::var_os("LOCALAPPDATA")?));
    let level = if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    // Dependencies (hyper, tao, ...) are noisy at debug level; keep them at info.
    // `Targets` instead of `EnvFilter` keeps the regex engine out of the exe.
    let filter = Targets::new()
        .with_default(LevelFilter::INFO)
        .with_target("limitrings", level)
        .with_target("limitrings_app", level)
        .with_target("limits_core", level);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(rotating_writer(&dir)))
                .with_filter(filter),
        )
        .init();
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn log_dir_is_under_limitrings() {
        assert_eq!(
            log_dir(Path::new(r"C:\Users\u\AppData\Local")),
            PathBuf::from(r"C:\Users\u\AppData\Local\limitrings\logs")
        );
    }

    #[test]
    fn writer_creates_log_file_and_rotates_at_1mb_keeping_3_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("limitrings").join("logs");
        let mut w = rotating_writer(&dir);
        let line = [b'x'; 1000];
        for _ in 0..4_500 {
            w.write_all(&line).unwrap();
        }
        w.flush().unwrap();
        assert!(dir.join("limitrings.log").is_file());
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["limitrings.log", "limitrings.log.1", "limitrings.log.2"]
        );
        for n in &names {
            let len = std::fs::metadata(dir.join(n)).unwrap().len();
            assert!(len <= MAX_BYTES as u64, "{n} is {len} bytes");
        }
    }
}
