//! Installed or standalone copy, and the move from one to the other.
//!
//! The installer registers ADBHelper per user. Later versions arrive through the
//! built-in updater, which replaces only the EXE, so the app writes its own version
//! into that entry on start. Portable copies (no entry, or another folder) are left alone.
//!
//! A standalone copy can start the installer from Settings. Before that it notes
//! its own path, so the installed app can point to the old file once.

use std::path::{Path, PathBuf};

/// Uninstall key that Inno Setup creates from the installer's AppId.
#[cfg(windows)]
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{3E7F0FBB-556D-4708-9254-3F8B13E7F30F}_is1";
/// Settings file with the path of the standalone copy that started the installer.
const OLD_COPY_FILE: &str = "standalone-copy";

/// Whether `exe` lives directly in the registered install folder.
fn runs_from(install_location: &str, exe: &Path) -> bool {
    let folder = Path::new(install_location.trim().trim_end_matches(['\\', '/']));
    let canonical = |path: &Path| std::fs::canonicalize(path).ok();
    match (exe.parent().and_then(canonical), canonical(folder)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(windows)]
mod registry {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The app's uninstall key, closed again when dropped.
    pub struct Key(HKEY);

    impl Key {
        pub fn open(path: &str, write: bool) -> Option<Self> {
            let path = wide(path);
            let access = if write {
                KEY_QUERY_VALUE | KEY_SET_VALUE
            } else {
                KEY_QUERY_VALUE
            };
            let mut key: HKEY = std::ptr::null_mut();
            // SAFETY: `path` is NUL-terminated; the key is closed in Drop.
            (unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) } == 0)
                .then_some(Self(key))
        }

        pub fn read(&self, name: &str) -> Option<String> {
            let name = wide(name);
            let mut buffer = vec![0u16; 1024];
            let mut bytes = (buffer.len() * 2) as u32;
            let mut kind = 0;
            // SAFETY: `buffer` holds `bytes` bytes and outlives the call.
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    buffer.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            (status == 0 && kind == REG_SZ).then(|| {
                let length = (bytes as usize / 2).min(buffer.len());
                String::from_utf16_lossy(&buffer[..length])
                    .trim_end_matches('\0')
                    .to_owned()
            })
        }

        pub fn write(&self, name: &str, value: &str) {
            let name = wide(name);
            let value = wide(value);
            // SAFETY: `value` is a NUL-terminated UTF-16 string of the given byte length.
            unsafe {
                RegSetValueExW(
                    self.0,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                );
            }
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the key was opened in `open`.
            unsafe { RegCloseKey(self.0) };
        }
    }
}

/// True when this EXE is the installed copy.
#[cfg(windows)]
pub fn runs_installed() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    registry::Key::open(UNINSTALL_KEY, false)
        .and_then(|key| key.read("InstallLocation"))
        .is_some_and(|location| runs_from(&location, &exe))
}

#[cfg(not(windows))]
pub fn runs_installed() -> bool {
    false
}

#[cfg(windows)]
pub fn record_version(version: &str) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(key) = registry::Key::open(UNINSTALL_KEY, true) else {
        return;
    };
    let installed_here = key
        .read("InstallLocation")
        .is_some_and(|location| runs_from(&location, &exe));
    if installed_here && key.read("DisplayVersion").as_deref() != Some(version) {
        key.write("DisplayVersion", version);
    }
}

#[cfg(not(windows))]
pub fn record_version(_: &str) {}

/// Remembers this standalone EXE before the installer starts.
pub fn remember_standalone_copy() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    crate::settings::write(OLD_COPY_FILE, exe.to_string_lossy().as_bytes())
}

/// The old standalone EXE, once, in the installed app; `None` when it is gone.
pub fn standalone_copy() -> Option<PathBuf> {
    if !runs_installed() {
        return None;
    }
    let path = crate::settings::file(OLD_COPY_FILE).ok()?;
    let recorded = std::fs::read_to_string(&path).ok()?;
    let current = std::env::current_exe().ok()?;
    let old = old_copy(&recorded, &current);
    if old.is_none() {
        let _ = std::fs::remove_file(path);
    }
    old
}

/// The recorded file if it still exists, is an EXE, and is not the running app.
fn old_copy(recorded: &str, current: &Path) -> Option<PathBuf> {
    let old = PathBuf::from(recorded.trim());
    let is_exe = old
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
    let same = std::fs::canonicalize(&old).ok() == std::fs::canonicalize(current).ok();
    (is_exe && old.is_file() && !same).then_some(old)
}

/// Forget the old copy (after the user deleted or kept it).
pub fn forget_standalone_copy() {
    if let Ok(path) = crate::settings::file(OLD_COPY_FILE) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("adbhelper-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn only_the_registered_folder_counts_as_installed() {
        let folder = temp_folder("install");
        let exe = folder.join("ADBHelper.exe");
        std::fs::write(&exe, b"").unwrap();
        let location = folder.display().to_string();
        assert!(runs_from(&location, &exe));
        assert!(runs_from(&format!("{location}\\"), &exe));
        assert!(!runs_from(
            &std::env::temp_dir().display().to_string(),
            &exe
        ));
        assert!(!runs_from("", &exe));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn only_an_existing_other_exe_counts_as_old_copy() {
        let folder = temp_folder("old-copy");
        let old = folder.join("ADBHelper-Windows-x64.exe");
        let current = folder.join("ADBHelper.exe");
        let notes = folder.join("notes.txt");
        for file in [&old, &current, &notes] {
            std::fs::write(file, b"").unwrap();
        }
        assert_eq!(
            old_copy(&format!(" {}\n", old.display()), &current),
            Some(old.clone())
        );
        assert_eq!(old_copy(&current.display().to_string(), &current), None);
        assert_eq!(old_copy(&notes.display().to_string(), &current), None);
        assert_eq!(
            old_copy(&folder.join("gone.exe").display().to_string(), &current),
            None
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
