//! "Start with Windows" through the per-user Run key. The pure helpers are
//! platform-neutral; the registry access is Windows-only.

use std::path::Path;

/// The Run value for `exe`: the path in double quotes, so spaces are safe.
pub fn run_value(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Whether the stored Run value must be rewritten (absent, or pointing at a
/// moved exe). Windows paths are case-insensitive.
pub fn needs_rewrite(current: Option<&str>, wanted: &str) -> bool {
    current.is_none_or(|c| !c.eq_ignore_ascii_case(wanted))
}

#[cfg(windows)]
pub use reg::{sync, RunKey};

#[cfg(not(windows))]
pub use neutral::{sync, RunKey};

#[cfg(windows)]
mod reg {
    use std::{io, path::Path};

    use winreg::{enums::*, RegKey};

    use super::{needs_rewrite, run_value};

    /// A registry key under HKEY_CURRENT_USER holding autostart values.
    pub struct RunKey {
        pub subkey: String,
    }

    impl RunKey {
        pub fn user() -> Self {
            Self {
                subkey: r"Software\Microsoft\Windows\CurrentVersion\Run".to_owned(),
            }
        }

        pub fn get(&self, name: &str) -> Option<String> {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(&self.subkey)
                .ok()?
                .get_value::<String, _>(name)
                .ok()
        }

        pub fn set(&self, name: &str, value: &str) -> io::Result<()> {
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&self.subkey)?;
            key.set_value(name, &value)
        }

        /// Removing a value that does not exist is not an error.
        pub fn remove(&self, name: &str) -> io::Result<()> {
            let key = match RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(&self.subkey, KEY_SET_VALUE)
            {
                Ok(key) => key,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e),
            };
            match key.delete_value(name) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }

        /// Deletes the whole subkey (test scratch keys).
        #[cfg(test)]
        pub fn delete_subkey(&self) {
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.subkey);
        }
    }

    /// Makes the Run value match `on`, rewriting it only when it differs.
    pub fn sync(key: &RunKey, name: &str, on: bool, exe: &Path) -> io::Result<()> {
        if !on {
            return key.remove(name);
        }
        let wanted = run_value(exe);
        if needs_rewrite(key.get(name).as_deref(), &wanted) {
            key.set(name, &wanted)?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod neutral {
    use std::{io, path::Path};

    pub struct RunKey {
        pub subkey: String,
    }

    impl RunKey {
        pub fn user() -> Self {
            Self {
                subkey: String::new(),
            }
        }
        pub fn get(&self, _name: &str) -> Option<String> {
            None
        }
        pub fn set(&self, _name: &str, _value: &str) -> io::Result<()> {
            Err(io::ErrorKind::Unsupported.into())
        }
        pub fn remove(&self, _name: &str) -> io::Result<()> {
            Ok(())
        }
    }

    pub fn sync(_key: &RunKey, _name: &str, _on: bool, _exe: &Path) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_quotes_path_with_spaces() {
        assert_eq!(
            run_value(Path::new(r"C:\Users\A B\limitrings.exe")),
            r#""C:\Users\A B\limitrings.exe""#
        );
    }

    #[test]
    fn rewrites_when_exe_moved() {
        assert!(needs_rewrite(
            Some(r#""C:\old\limitrings.exe""#),
            r#""C:\new\limitrings.exe""#
        ));
        assert!(!needs_rewrite(
            Some(r#""C:\NEW\limitrings.exe""#),
            r#""C:\new\limitrings.exe""#
        ));
        assert!(needs_rewrite(None, "x"));
    }

    #[cfg(windows)]
    struct Scratch(RunKey);
    #[cfg(windows)]
    impl Drop for Scratch {
        fn drop(&mut self) {
            self.0.delete_subkey();
        }
    }

    #[cfg(windows)]
    #[test]
    fn sync_sets_and_removes_in_scratch_key() {
        let scratch = Scratch(RunKey {
            subkey: format!(r"Software\limitrings-tests\{}", std::process::id()),
        });
        let key = &scratch.0;
        sync(key, "limitrings", true, Path::new(r"C:\a b\x.exe")).unwrap();
        assert_eq!(key.get("limitrings").as_deref(), Some(r#""C:\a b\x.exe""#));
        sync(key, "limitrings", false, Path::new("unused")).unwrap();
        assert_eq!(key.get("limitrings"), None);
        // removing again is fine
        key.remove("limitrings").unwrap();
    }
}
