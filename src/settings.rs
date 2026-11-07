use std::{io, path::PathBuf};

pub fn file(name: &str) -> io::Result<PathBuf> {
    let directory = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    directory
        .filter(|path| path.is_absolute())
        .map(|path| path.join("ADBHelper").join(name))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "User configuration folder unavailable",
            )
        })
}

pub fn write(name: &str, bytes: &[u8]) -> io::Result<()> {
    let path = file(name)?;
    std::fs::create_dir_all(path.parent().expect("settings file has a directory"))?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    let result = std::fs::rename(&temporary, path);
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
