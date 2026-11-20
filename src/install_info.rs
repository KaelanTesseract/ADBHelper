//! Keeps the Windows "Installed apps" entry in step with in-app updates.
//!
//! The installer registers ADBHelper per user. Later versions arrive through the
//! built-in updater, which replaces only the EXE, so the app writes its own version
//! into that entry on start. Portable copies (no entry, or another folder) are left alone.

/// Uninstall key that Inno Setup creates from the installer's AppId.
#[cfg(windows)]
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{3E7F0FBB-556D-4708-9254-3F8B13E7F30F}_is1";

/// Whether `exe` lives directly in the registered install folder.
#[cfg_attr(not(windows), allow(dead_code))]
fn runs_from(install_location: &str, exe: &std::path::Path) -> bool {
    let folder = std::path::Path::new(install_location.trim().trim_end_matches(['\\', '/']));
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    match (exe.parent().and_then(canonical), canonical(folder)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(windows)]
pub fn record_version(version: &str) {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read(key: HKEY, name: &str) -> Option<String> {
        let name = wide(name);
        let mut buffer = vec![0u16; 1024];
        let mut bytes = (buffer.len() * 2) as u32;
        let mut kind = 0;
        // SAFETY: `buffer` holds `bytes` bytes and outlives the call.
        let status = unsafe {
            RegQueryValueExW(
                key,
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

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let path = wide(UNINSTALL_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated; `key` is closed below.
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    } != 0
    {
        return;
    }
    let installed_here =
        read(key, "InstallLocation").is_some_and(|location| runs_from(&location, &exe));
    if installed_here && read(key, "DisplayVersion").as_deref() != Some(version) {
        let name = wide("DisplayVersion");
        let value = wide(version);
        // SAFETY: `value` is a NUL-terminated UTF-16 string of the given byte length.
        unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            );
        }
    }
    // SAFETY: `key` was opened above.
    unsafe { RegCloseKey(key) };
}

#[cfg(not(windows))]
pub fn record_version(_: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_registered_folder_counts_as_installed() {
        let folder = std::env::temp_dir().join(format!("adbhelper-install-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
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
}
