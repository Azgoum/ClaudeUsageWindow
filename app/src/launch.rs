use limits_core::BrowserLauncher;
use std::path::{Path, PathBuf};

const FALLBACK_EXE: &str = r"C:\Program Files\Mozilla Firefox\firefox.exe";
const LOGIN_URL: &str = "https://claude.ai/login";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

/// Firefox exe lookup order: HKLM, then HKCU, then the default install path.
/// `registry` returns the `PathToExe` recorded under a hive; only existing files are accepted.
pub fn resolve_exe(
    registry: impl Fn(Hive) -> Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    [Hive::LocalMachine, Hive::CurrentUser]
        .into_iter()
        .filter_map(registry)
        .chain(std::iter::once(PathBuf::from(FALLBACK_EXE)))
        .find(|p| exists(p))
}

#[cfg(windows)]
fn registry_exe(hive: Hive) -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let root = RegKey::predef(match hive {
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        Hive::CurrentUser => HKEY_CURRENT_USER,
    });
    let ff = root.open_subkey(r"SOFTWARE\Mozilla\Mozilla Firefox").ok()?;
    let ver: String = ff.get_value("CurrentVersion").ok()?;
    let main = ff.open_subkey(format!(r"{ver}\Main")).ok()?;
    let exe: String = main.get_value("PathToExe").ok()?;
    Some(PathBuf::from(exe))
}

#[cfg(not(windows))]
fn registry_exe(_hive: Hive) -> Option<PathBuf> {
    None
}

pub fn firefox_exe() -> Option<PathBuf> {
    resolve_exe(registry_exe, |p| p.is_file())
}

pub struct FirefoxLauncher {
    /// Debug-only (`LIMITRINGS_FAKE_NO_FIREFOX`): report Firefox as missing.
    pub pretend_missing: bool,
}

impl BrowserLauncher for FirefoxLauncher {
    fn firefox_installed(&self) -> bool {
        !self.pretend_missing && firefox_exe().is_some()
    }

    fn open_login(&self) -> anyhow::Result<()> {
        let exe = firefox_exe().ok_or_else(|| anyhow::anyhow!("Firefox not found"))?;
        std::process::Command::new(exe).arg(LOGIN_URL).spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn prefers_hklm_then_hkcu() {
        let both = |h: Hive| match h {
            Hive::LocalMachine => Some(p(r"D:\ff\firefox.exe")),
            Hive::CurrentUser => Some(p(r"E:\ff\firefox.exe")),
        };
        assert_eq!(resolve_exe(both, |_| true), Some(p(r"D:\ff\firefox.exe")));

        let hkcu_only = |h: Hive| (h == Hive::CurrentUser).then(|| p(r"E:\ff\firefox.exe"));
        assert_eq!(
            resolve_exe(hkcu_only, |_| true),
            Some(p(r"E:\ff\firefox.exe"))
        );
    }

    #[test]
    fn skips_registry_entries_whose_file_is_missing() {
        let stale = |_| Some(p(r"D:\gone\firefox.exe"));
        let exists = |q: &Path| q != Path::new(r"D:\gone\firefox.exe");
        assert_eq!(resolve_exe(stale, exists), Some(p(FALLBACK_EXE)));
    }

    #[test]
    fn falls_back_to_program_files_then_none() {
        assert_eq!(resolve_exe(|_| None, |_| true), Some(p(FALLBACK_EXE)));
        assert_eq!(resolve_exe(|_| None, |_| false), None);
    }
}
