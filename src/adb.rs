use crate::localization::{format_text, text};
use std::path::PathBuf;
use std::process::{Command, Output};

fn adb_command() -> Command {
    let mut command = Command::new(executable());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The GUI has no console; suppress a new console for each ADB child process too.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn executable() -> PathBuf {
    for variable in ["ADB_PATH", "ADBUTILS_ADB_PATH"] {
        if let Some(path) = std::env::var_os(variable).filter(|path| !path.is_empty()) {
            let path = PathBuf::from(path);
            if path.is_file() {
                return path;
            }
        }
    }

    if let Some(path) = find_on_path() {
        return path;
    }

    #[cfg(windows)]
    if let Some(path) = mumu_player_adb() {
        return path;
    }

    PathBuf::from("adb")
}

fn find_on_path() -> Option<PathBuf> {
    let names = if cfg!(windows) {
        ["adb.exe", "adb"]
    } else {
        ["adb", "adb"]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|directory| {
        names
            .iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(windows)]
fn mumu_player_adb() -> Option<PathBuf> {
    let mut roots = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            roots.push(PathBuf::from(root).join("Netease").join("MuMuPlayer"));
        }
    }
    roots.push(PathBuf::from(r"C:\Program Files\Netease\MuMuPlayer"));

    for root in roots {
        let mut candidates = Vec::new();
        if let Ok(entries) = std::fs::read_dir(root.join("nx_device")) {
            candidates.extend(
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path().join("shell").join("adb.exe")),
            );
            candidates.sort_by(|left, right| {
                let version = |path: &PathBuf| {
                    path.parent()
                        .and_then(|shell| shell.parent())
                        .and_then(|directory| directory.file_name())
                        .and_then(|name| name.to_str())
                        .and_then(|name| name.parse::<f32>().ok())
                        .unwrap_or_default()
                };
                version(right).total_cmp(&version(left))
            });
        }
        candidates.push(root.join("nx_main").join("adb.exe"));
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }
    None
}

pub fn suggested_adb_path() -> String {
    executable().display().to_string()
}

fn is_mumu_adb() -> bool {
    executable()
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains("mumuplayer")
}

fn connect_mumu() {
    #[cfg(windows)]
    let mumu_installed = mumu_player_adb().is_some();
    #[cfg(not(windows))]
    let mumu_installed = false;
    if !is_mumu_adb() && !mumu_installed {
        return;
    }
    let port = std::env::var("MUMU_ADB_PORT").unwrap_or_else(|_| "7555".to_owned());
    let address = format!("127.0.0.1:{port}");
    let _ = adb_command().args(["connect", address.as_str()]).output();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    pub details: String,
}

#[derive(Debug)]
pub enum AdbError {
    NotInstalled(std::io::Error),
    Failed(String),
    InvalidOutput(String),
}

impl std::fmt::Display for AdbError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled(error) => formatter.write_str(&format_text!(
                "Could not start ADB: {error}",
                "ADB konnte nicht gestartet werden: {error}"
            )),
            Self::Failed(message) => formatter.write_str(&format_text!(
                "ADB command failed: {message}",
                "ADB-Befehl fehlgeschlagen: {message}"
            )),
            Self::InvalidOutput(message) => formatter.write_str(&format_text!(
                "Could not read ADB output: {message}",
                "ADB-Ausgabe konnte nicht gelesen werden: {message}"
            )),
        }
    }
}

impl std::error::Error for AdbError {}

pub fn devices() -> Result<Vec<Device>, AdbError> {
    connect_mumu();
    let output = run(&["devices", "-l"])?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| AdbError::InvalidOutput(error.to_string()))?;

    Ok(stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            (state == "device").then(|| Device {
                serial: serial.to_owned(),
                details: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect())
}

pub fn shell(serial: &str, args: &[&str]) -> Result<Output, AdbError> {
    if args.first() == Some(&"input") {
        crate::afk_journey::recovery::check_process(serial).map_err(AdbError::Failed)?;
    }
    let mut command_args = vec!["-s", serial, "shell"];
    command_args.extend_from_slice(args);
    run(&command_args)
}

pub fn installed_packages(serial: &str) -> Result<Vec<String>, AdbError> {
    let output = shell(serial, &["pm", "list", "packages"])?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| AdbError::InvalidOutput(error.to_string()))?;
    Ok(stdout
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(str::to_owned)
        .collect())
}

pub fn tap(serial: &str, x: u32, y: u32) -> Result<(), AdbError> {
    shell(serial, &["input", "tap", &x.to_string(), &y.to_string()]).map(|_| ())
}

pub fn press_back(serial: &str) -> Result<(), AdbError> {
    shell(serial, &["input", "keyevent", "KEYCODE_BACK"]).map(|_| ())
}

pub fn launch_afk_journey(serial: &str) -> Result<String, AdbError> {
    crate::afk_journey::recovery::check_process(serial).map_err(AdbError::Failed)?;
    let packages = installed_packages(serial)?;
    let package = packages
        .iter()
        .find(|package| crate::afk_journey::is_supported_package(package))
        .ok_or_else(|| AdbError::Failed("AFK Journey is not installed on this device".into()))?;
    shell(
        serial,
        &[
            "monkey",
            "-p",
            package,
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ],
    )?;
    crate::afk_journey::recovery::expect_running(serial);
    Ok(package.clone())
}

pub fn stop_afk_journey(serial: &str) -> Result<String, AdbError> {
    crate::afk_journey::recovery::check_process(serial).map_err(AdbError::Failed)?;
    let packages = installed_packages(serial)?;
    let package = packages
        .iter()
        .find(|package| crate::afk_journey::is_supported_package(package))
        .ok_or_else(|| AdbError::Failed("AFK Journey is not installed on this device".into()))?;
    crate::afk_journey::recovery::intentional_stop(serial);
    shell(serial, &["am", "force-stop", package])?;
    Ok(package.clone())
}

pub fn screenshot(serial: &str) -> Result<Vec<u8>, AdbError> {
    crate::afk_journey::recovery::check_process(serial).map_err(AdbError::Failed)?;
    let output = run(&["-s", serial, "exec-out", "screencap", "-p"])?;
    Ok(output.stdout)
}

pub fn afk_journey_running(serial: &str) -> Result<bool, AdbError> {
    // Older emulator ADB clients report a remote nonzero exit as "error: closed".
    // A fixed Android shell command returns the pidof status as explicit data,
    // leaving transport failures distinguishable from a missing game process.
    let command = format!(
        "pidof {}; echo ADBHELPER_PID_EXIT:$?",
        crate::afk_journey::PACKAGE_NAMES.join(" ")
    );
    let output = run(&["-s", serial, "shell", &command])?;
    parse_game_process(&String::from_utf8_lossy(&output.stdout))
}

fn parse_game_process(stdout: &str) -> Result<bool, AdbError> {
    let Some((pids, status)) = stdout.rsplit_once("ADBHELPER_PID_EXIT:") else {
        return Err(AdbError::InvalidOutput(
            text(
                "The process check returned no status marker",
                "Prozessprüfung ohne Statusmarker",
            )
            .into(),
        ));
    };
    let pids = pids.trim();
    match status.trim() {
        "0" if !pids.is_empty()
            && pids
                .split_whitespace()
                .all(|s| s.parse::<u32>().is_ok_and(|pid| pid > 0)) =>
        {
            Ok(true)
        }
        "1" if pids.is_empty() => Ok(false),
        _ => Err(AdbError::InvalidOutput(
            text(
                "The process check could not confirm a game crash",
                "Prozessprüfung nicht eindeutig; kein Spielabsturz bestätigt",
            )
            .into(),
        )),
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    #[test]
    fn missing_process_is_distinct_from_unsupported_or_failed_adb_commands() {
        assert!(parse_game_process("30257\r\r\nADBHELPER_PID_EXIT:0\r\n").unwrap());
        assert!(!parse_game_process("ADBHELPER_PID_EXIT:1\r\n").unwrap());
        for response in [
            "error: closed",
            "device offline",
            "ADBHELPER_PID_EXIT:127",
            "ADBHELPER_PID_EXIT:0",
            "0\nADBHELPER_PID_EXIT:0",
        ] {
            assert!(parse_game_process(response).is_err(), "{response}");
        }
    }
}

pub fn display_size(serial: &str) -> Result<(u32, u32), AdbError> {
    let output = shell(serial, &["wm", "size"])?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| AdbError::InvalidOutput(error.to_string()))?;
    stdout
        .lines()
        .filter_map(|line| line.rsplit_once(' '))
        .find_map(|(_, size)| {
            let (width, height) = size.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .ok_or_else(|| AdbError::InvalidOutput(stdout.trim().to_owned()))
}

pub fn tap_game_point(
    serial: &str,
    x: u32,
    y: u32,
    actual_size: (u32, u32),
) -> Result<(), AdbError> {
    let scaled_x = x.saturating_mul(actual_size.0) / crate::afk_journey::BASE_RESOLUTION.0;
    let scaled_y = y.saturating_mul(actual_size.1) / crate::afk_journey::BASE_RESOLUTION.1;
    tap(serial, scaled_x, scaled_y)
}

pub fn swipe_game_points(
    serial: &str,
    start: (u32, u32),
    end: (u32, u32),
    actual_size: (u32, u32),
    duration_ms: u32,
) -> Result<(), AdbError> {
    let scale = |p: (u32, u32)| {
        (
            p.0.saturating_mul(actual_size.0) / crate::afk_journey::BASE_RESOLUTION.0,
            p.1.saturating_mul(actual_size.1) / crate::afk_journey::BASE_RESOLUTION.1,
        )
    };
    let start = scale(start);
    let end = scale(end);
    shell(
        serial,
        &[
            "input",
            "swipe",
            &start.0.to_string(),
            &start.1.to_string(),
            &end.0.to_string(),
            &end.1.to_string(),
            &duration_ms.to_string(),
        ],
    )
    .map(|_| ())
}

fn run(args: &[&str]) -> Result<Output, AdbError> {
    let output = adb_command()
        .args(args)
        .output()
        .map_err(AdbError::NotInstalled)?;

    if output.status.success() {
        Ok(output)
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(AdbError::Failed(if message.is_empty() {
            format!("exit status {}", output.status)
        } else {
            message
        }))
    }
}
