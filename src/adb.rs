use crate::localization::tr;
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
    match adb_source() {
        AdbSource::Setting(path) | AdbSource::Path(path) | AdbSource::MuMu(path) => path,
        AdbSource::NotFound => PathBuf::from("adb"),
    }
}

/// Which adb ADBHelper uses and where it came from, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbSource {
    /// The ADB path setting (or `ADB_PATH`).
    Setting(PathBuf),
    /// Found in `PATH`.
    Path(PathBuf),
    /// MuMu Player's own adb.
    MuMu(PathBuf),
    /// Nothing found; plain `adb` is tried.
    NotFound,
}

pub fn adb_source() -> AdbSource {
    for variable in ["ADB_PATH", "ADBUTILS_ADB_PATH"] {
        if let Some(path) = std::env::var_os(variable).filter(|path| !path.is_empty()) {
            let path = PathBuf::from(path);
            if path.is_file() {
                return AdbSource::Setting(path);
            }
        }
    }

    if let Some(path) = find_on_path() {
        return AdbSource::Path(path);
    }

    #[cfg(windows)]
    if let Some(path) = mumu_player_adb() {
        return AdbSource::MuMu(path);
    }

    AdbSource::NotFound
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

/// MuMu Player's adb, searched once per app start: the search asks the
/// registry and looks through every drive, too slow for every ADB command.
#[cfg(windows)]
fn mumu_player_adb() -> Option<PathBuf> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| mumu_adb_candidates(&mumu_roots()).into_iter().next())
        .clone()
}

/// Folders that may hold MuMu Player: the install folders Windows lists for
/// uninstalling, and the usual Netease folders on every drive, so a MuMu
/// version or drive we did not know of is still found. Searched on every
/// start, so an update that moves MuMu is followed without a setting.
#[cfg(windows)]
fn mumu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in [
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        for name in ["MuMu", "Nemu"] {
            let mut command = Command::new("reg");
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            if let Ok(output) = command.args(["query", key, "/s", "/f", name]).output() {
                roots.extend(mumu_install_dirs(&String::from_utf8_lossy(&output.stdout)));
            }
        }
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            roots.push(PathBuf::from(root).join("Netease"));
        }
    }
    for drive in b'C'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", drive as char));
        if !drive.is_dir() {
            continue;
        }
        for folder in ["Program Files", "Program Files (x86)", ""] {
            roots.push(drive.join(folder).join("Netease"));
            // Older MuMu Player installs straight into Program Files as "Nemu".
            roots.push(drive.join(folder));
        }
    }
    roots
}

/// Every `adb.exe` of MuMu Player under `roots`, best first. A root is either
/// a MuMu install folder itself or a folder holding them (such as Netease).
/// Known layouts: `nx_device/<version>/shell` and `nx_main` (MuMu Player),
/// `shell` (MuMu Player 12), `vmonitor/bin` in a `Nemu` folder and
/// `emulator/nemu/vmonitor/bin` (older MuMu Player, `adb_server.exe`).
#[cfg_attr(not(windows), allow(dead_code))]
fn mumu_adb_candidates(roots: &[PathBuf]) -> Vec<PathBuf> {
    let is_mumu = |path: &std::path::Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                name.contains("mumu") || name.contains("nemu")
            })
    };
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let installs = if is_mumu(root) {
            vec![root.clone()]
        } else {
            let mut installs: Vec<PathBuf> = std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() && is_mumu(path))
                .collect();
            installs.sort();
            installs
        };
        for install in installs {
            let mut devices: Vec<PathBuf> = std::fs::read_dir(install.join("nx_device"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("shell").join("adb.exe"))
                .collect();
            let version = |path: &PathBuf| {
                path.parent()
                    .and_then(|shell| shell.parent())
                    .and_then(|directory| directory.file_name())
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.parse::<f32>().ok())
                    .unwrap_or_default()
            };
            devices.sort_by(|left, right| version(right).total_cmp(&version(left)));
            devices.push(install.join("nx_main").join("adb.exe"));
            devices.push(install.join("shell").join("adb.exe"));
            devices.push(install.join("vmonitor").join("bin").join("adb_server.exe"));
            devices.push(
                install
                    .join("emulator")
                    .join("nemu")
                    .join("vmonitor")
                    .join("bin")
                    .join("adb_server.exe"),
            );
            for path in devices {
                if path.is_file() && !found.contains(&path) {
                    found.push(path);
                }
            }
        }
    }
    found
}

/// MuMu install folders from `reg query … /s /f MuMu` output: per key whose
/// name or display name mentions MuMu, the install location or the folder of
/// its uninstaller.
#[cfg_attr(not(windows), allow(dead_code))]
fn mumu_install_dirs(output: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for block in output.split("HKEY_").skip(1) {
        let lower = block.to_ascii_lowercase();
        if !lower.contains("mumu") && !lower.contains("nemu") {
            continue;
        }
        let value = |name: &str| {
            block.lines().find_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix(name)?.trim_start();
                let rest = rest
                    .strip_prefix("REG_SZ")
                    .or_else(|| rest.strip_prefix("REG_EXPAND_SZ"))?;
                let text = rest.trim();
                // An uninstall command may carry arguments after the quoted path.
                let path = match text.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next()?,
                    None => text,
                };
                (!path.is_empty()).then(|| PathBuf::from(path))
            })
        };
        let dir = value("InstallLocation")
            .or_else(|| value("UninstallString").and_then(|path| path.parent().map(PathBuf::from)));
        if let Some(dir) = dir {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// Ports MuMu Player listens on: 7555 (older MuMu), 16384 and the next
/// instances 32 apart (MuMu Player 12), 5555; a port set in `MUMU_ADB_PORT`
/// comes first.
fn mumu_ports(configured: Option<&str>) -> Vec<u16> {
    let mut ports: Vec<u16> = configured
        .and_then(|port| port.trim().parse().ok())
        .into_iter()
        .collect();
    for port in [7555, 16384, 16416, 16448, 16480, 5555] {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    ports
}

pub fn suggested_adb_path() -> String {
    executable().display().to_string()
}

/// Connects to every usual MuMu Player port, whichever adb is used: a port
/// that is closed answers at once. Returns the ports tried and those that
/// answered.
fn connect_mumu() -> (Vec<u16>, Vec<u16>) {
    let configured = std::env::var("MUMU_ADB_PORT").ok();
    let ports = mumu_ports(configured.as_deref());
    let answered = ports
        .iter()
        .copied()
        .filter(|port| {
            let address = format!("127.0.0.1:{port}");
            adb_command()
                .args(["connect", address.as_str()])
                .output()
                .is_ok_and(|output| connected(&String::from_utf8_lossy(&output.stdout)))
        })
        .collect();
    (ports, answered)
}

/// `adb connect` printed that the address answered.
fn connected(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.starts_with("connected to") || line.starts_with("already connected to"))
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
            Self::NotInstalled(error) => {
                formatter.write_str(&tr!("adb-could-not-start-adb-error", error = error))
            }
            Self::Failed(message) => {
                formatter.write_str(&tr!("adb-adb-command-failed-message", message = message))
            }
            Self::InvalidOutput(message) => formatter.write_str(&tr!(
                "adb-could-not-read-adb-output-message",
                message = message
            )),
        }
    }
}

impl std::error::Error for AdbError {}

pub fn devices() -> Result<Vec<Device>, AdbError> {
    scan().map(|scan| scan.devices)
}

/// What a device search found, with what the log needs to explain it.
#[derive(Debug, Clone)]
pub struct Scan {
    pub adb: AdbSource,
    /// MuMu Player ports tried and those that answered.
    pub ports: (Vec<u16>, Vec<u16>),
    /// Devices ready to use.
    pub devices: Vec<Device>,
    /// Devices adb lists in another state, such as `offline` or
    /// `unauthorized`, as (serial, state).
    pub waiting: Vec<(String, String)>,
}

pub fn scan() -> Result<Scan, AdbError> {
    let adb = adb_source();
    let ports = connect_mumu();
    let output = run(&["devices", "-l"])?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| AdbError::InvalidOutput(error.to_string()))?;
    let (devices, waiting) = parse_devices(&stdout);
    Ok(Scan {
        adb,
        ports,
        devices,
        waiting,
    })
}

/// `adb devices -l` output as ready devices and the others with their state.
fn parse_devices(stdout: &str) -> (Vec<Device>, Vec<(String, String)>) {
    let mut ready = Vec::new();
    let mut waiting = Vec::new();
    for line in stdout.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let (Some(serial), Some(state)) = (fields.next(), fields.next()) else {
            continue;
        };
        if state == "device" {
            ready.push(Device {
                serial: serial.to_owned(),
                details: fields.collect::<Vec<_>>().join(" "),
            });
        } else {
            waiting.push((serial.to_owned(), state.to_owned()));
        }
    }
    (ready, waiting)
}

pub fn shell(serial: &str, args: &[&str]) -> Result<Output, AdbError> {
    if args.first() == Some(&"input") {
        crate::afk_journey::recovery::check_process(serial)
            .map_err(|error| AdbError::Failed(error.to_string()))?;
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
    crate::afk_journey::recovery::check_process(serial)
        .map_err(|error| AdbError::Failed(error.to_string()))?;
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
    crate::afk_journey::recovery::check_process(serial)
        .map_err(|error| AdbError::Failed(error.to_string()))?;
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
    crate::afk_journey::recovery::check_process(serial)
        .map_err(|error| AdbError::Failed(error.to_string()))?;
    let started = std::time::Instant::now();
    let output = run(&["-s", serial, "exec-out", "screencap", "-p"])?;
    crate::timing::observe_screenshot(started.elapsed());
    Ok(output.stdout)
}

pub fn raw_screenshot(serial: &str) -> Result<Vec<u8>, AdbError> {
    crate::afk_journey::recovery::check_process(serial)
        .map_err(|error| AdbError::Failed(error.to_string()))?;
    Ok(run(&["-s", serial, "exec-out", "screencap"])?.stdout)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Running,
    Missing,
    Launcher,
}

/// Only transport failures are retried. Permission/configuration failures and
/// action-specific recognition errors are not treated as game crashes.
pub fn is_transport_error(error: &str) -> bool {
    let message = error.to_ascii_lowercase();
    [
        "error: closed",
        "device offline",
        "device not found",
        "no devices/emulators found",
        "connection reset",
        "connection refused",
        "connection aborted",
        "transport error",
        "protocol fault",
        "cannot connect to daemon",
        "failed to start daemon",
    ]
    .iter()
    .any(|part| message.contains(part))
        || (message.contains("device '") && message.contains("' not found"))
}

fn component_package(value: &str) -> Option<String> {
    value.split_whitespace().find_map(|word| {
        let (package, _) = word.split_once('/')?;
        (package.contains('.')
            && package
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'))
        .then(|| package.to_owned())
    })
}

fn foreground_package(output: &str) -> Option<String> {
    // Prefer the modern top-resumed record over older/multi-window records.
    for marker in [
        "topResumedActivity=",
        "mResumedActivity:",
        "mResumedActivity=",
        "mFocusedActivity:",
    ] {
        if let Some(package) = output
            .lines()
            .filter(|line| line.contains(marker))
            .find_map(component_package)
        {
            return Some(package);
        }
    }
    None
}

pub fn game_state(serial: &str) -> Result<GameState, AdbError> {
    if !afk_journey_running(serial)? {
        return Ok(GameState::Missing);
    }
    let focus = match run(&["-s", serial, "shell", "dumpsys", "activity", "activities"]) {
        Ok(output) => foreground_package(&String::from_utf8_lossy(&output.stdout)),
        Err(error) if is_transport_error(&error.to_string()) => return Err(error),
        // Some Android builds restrict dumpsys. PID detection still works.
        Err(_) => None,
    };
    if let Some(focus) = focus.filter(|package| !crate::afk_journey::is_supported_package(package))
    {
        let home = match run(&[
            "-s",
            serial,
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.HOME",
        ]) {
            Ok(output) => component_package(&String::from_utf8_lossy(&output.stdout)),
            Err(error) if is_transport_error(&error.to_string()) => return Err(error),
            Err(_) => None,
        };
        if home.as_deref() == Some(focus.as_str()) {
            return Ok(GameState::Launcher);
        }
    }
    Ok(GameState::Running)
}

pub fn reconnect(serial: &str) -> Result<(), AdbError> {
    // Reconnect only this transport; never kill the shared ADB server or
    // repeat a tap, purchase, claim, or other game input after an error.
    if serial.parse::<std::net::SocketAddr>().is_ok() {
        run(&["connect", serial]).map(|_| ())
    } else {
        run(&["-s", serial, "reconnect"]).map(|_| ())
    }
}

#[cfg(test)]
pub(crate) fn disconnect_for_recovery_test(serial: &str) -> Result<(), AdbError> {
    if serial.parse::<std::net::SocketAddr>().is_err() {
        return Err(AdbError::Failed(
            "The recovery integration test requires a TCP emulator".into(),
        ));
    }
    run(&["disconnect", serial]).map(|_| ())
}

fn parse_game_process(stdout: &str) -> Result<bool, AdbError> {
    let Some((pids, status)) = stdout.rsplit_once("ADBHELPER_PID_EXIT:") else {
        return Err(AdbError::InvalidOutput(
            tr!("adb-the-process-check-returned-no-status").into(),
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
            tr!("adb-the-process-check-could-not-confirm").into(),
        )),
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

pub fn hold_game_point(
    serial: &str,
    point: (u32, u32),
    actual_size: (u32, u32),
    duration_ms: u32,
) -> Result<(), AdbError> {
    let (width, height) = crate::afk_journey::BASE_RESOLUTION;
    let x = point.0.saturating_mul(actual_size.0) / width;
    let y = point.1.saturating_mul(actual_size.1) / height;
    let seconds = duration_ms / 1000;
    let millis = duration_ms % 1000;
    let command = format!(
        "input motionevent DOWN {x} {y} && sleep {seconds}.{millis:03}; input motionevent UP {x} {y}"
    );
    shell(serial, &[&command]).map(|_| ())
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

#[cfg(test)]
mod process_tests {
    use super::*;
    #[test]
    fn transport_failures_and_foreground_records_are_unambiguous() {
        for value in [
            "ADB command failed: ADB command failed: error: closed",
            "error: device offline",
            "error: device '127.0.0.1:7555' not found",
            "ADB command failed: adb.exe: device '127.0.0.1:7555' not found",
            "error: connection reset by peer",
        ] {
            assert!(is_transport_error(value), "{value}");
        }
        for value in [
            "The battle closed without a result",
            "AFK Journey is not installed",
            "device unauthorized",
            "permission denied",
        ] {
            assert!(!is_transport_error(value), "{value}");
        }
        let focus = "mResumedActivity: ActivityRecord{12 u0 app.lawnchair/.LawnchairLauncher t2}\ntopResumedActivity=ActivityRecord{34 u0 com.farlightgames.igame.gp/com.game.Main t7}";
        assert_eq!(
            foreground_package(focus).as_deref(),
            Some("com.farlightgames.igame.gp")
        );
        assert_eq!(
            foreground_package(
                "topResumedActivity=ActivityRecord{12 u0 app.lawnchair/.LawnchairLauncher t2}"
            )
            .as_deref(),
            Some("app.lawnchair")
        );
        assert_eq!(
            foreground_package("background record com.farlightgames.igame.gp/com.game.Main"),
            None
        );
        assert_eq!(component_package("No activity found"), None);
    }
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

    fn fake_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("adbhelper-emulator-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn touch(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn mumu_adb_is_found_in_every_known_layout() {
        let netease = fake_dir("layouts");
        // MuMu Player 12 and the newer MuMu Player keep adb in different places.
        touch(
            &netease
                .join("MuMuPlayer-12.0")
                .join("shell")
                .join("adb.exe"),
        );
        touch(&netease.join("MuMuPlayer").join("nx_main").join("adb.exe"));
        touch(
            &netease
                .join("MuMuPlayer")
                .join("nx_device")
                .join("12.0")
                .join("shell")
                .join("adb.exe"),
        );
        touch(&netease.join("Other").join("shell").join("adb.exe"));
        let found = mumu_adb_candidates(std::slice::from_ref(&netease));
        assert_eq!(
            found,
            [
                netease
                    .join("MuMuPlayer")
                    .join("nx_device")
                    .join("12.0")
                    .join("shell")
                    .join("adb.exe"),
                netease.join("MuMuPlayer").join("nx_main").join("adb.exe"),
                netease
                    .join("MuMuPlayer-12.0")
                    .join("shell")
                    .join("adb.exe"),
            ]
        );
        std::fs::remove_dir_all(&netease).unwrap();
    }

    #[test]
    fn an_install_folder_counts_as_well_as_its_parent() {
        let install = fake_dir("install").join("MuMu Player 12");
        touch(&install.join("shell").join("adb.exe"));
        assert_eq!(
            mumu_adb_candidates(std::slice::from_ref(&install)),
            [install.join("shell").join("adb.exe")]
        );
        std::fs::remove_dir_all(install.parent().unwrap()).unwrap();
    }

    #[test]
    fn mumu_install_folders_are_read_from_the_uninstall_entries() {
        let output = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\MuMuPlayer-12.0
    DisplayName    REG_SZ    MuMu Player 12
    UninstallString    REG_SZ    "D:\Games\Netease\MuMuPlayer-12.0\uninstall.exe"
    DisplayIcon    REG_SZ    D:\Games\Netease\MuMuPlayer-12.0\shell\MuMuPlayer.exe
HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam
    UninstallString    REG_SZ    "C:\Program Files (x86)\Steam\uninstall.exe"
"#;
        assert_eq!(
            mumu_install_dirs(output),
            [std::path::PathBuf::from(
                r"D:\Games\Netease\MuMuPlayer-12.0"
            )]
        );
        // The newer MuMu Player names its install folder directly.
        let output = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\MuMuPlayerGlobal
    DisplayName    REG_SZ    MuMuPlayer
    InstallLocation    REG_SZ    C:\Program Files\Netease\MuMuPlayer
    UninstallString    REG_SZ    "C:\Program Files\Netease\MuMuPlayer\uninstall.exe"

End of search: 4 match(es) found.
"#;
        assert_eq!(
            mumu_install_dirs(output),
            [std::path::PathBuf::from(
                r"C:\Program Files\Netease\MuMuPlayer"
            )]
        );
    }

    #[test]
    fn every_usual_mumu_port_is_tried_and_a_set_port_first() {
        assert_eq!(mumu_ports(None), [7555, 16384, 16416, 16448, 16480, 5555]);
        assert_eq!(
            mumu_ports(Some("16512")),
            [16512, 7555, 16384, 16416, 16448, 16480, 5555]
        );
        assert_eq!(
            mumu_ports(Some("7555")),
            [7555, 16384, 16416, 16448, 16480, 5555]
        );
        assert_eq!(
            mumu_ports(Some("kein Port")),
            [7555, 16384, 16416, 16448, 16480, 5555]
        );
    }

    #[test]
    fn the_old_mumu_player_in_a_nemu_folder_is_found() {
        // MuMuPlayer 6 installs as "Nemu" with adb_server.exe (MuMu's developer manual).
        let program_files = fake_dir("old-layout");
        let adb = program_files
            .join("Nemu")
            .join("vmonitor")
            .join("bin")
            .join("adb_server.exe");
        touch(&adb);
        touch(&program_files.join("Steam").join("adb.exe"));
        assert_eq!(
            mumu_adb_candidates(std::slice::from_ref(&program_files)),
            [adb]
        );
        std::fs::remove_dir_all(&program_files).unwrap();
    }

    #[test]
    fn every_device_state_is_read_not_only_ready_ones() {
        let stdout = "List of devices attached\n\
            127.0.0.1:16384        device product:V1923A model:V1923A transport_id:3\n\
            127.0.0.1:7555         offline transport_id:1\n\
            emulator-5554          unauthorized transport_id:2\n\n";
        let (ready, waiting) = parse_devices(stdout);
        assert_eq!(
            ready,
            [Device {
                serial: "127.0.0.1:16384".into(),
                details: "product:V1923A model:V1923A transport_id:3".into(),
            }]
        );
        assert_eq!(
            waiting,
            [
                ("127.0.0.1:7555".to_owned(), "offline".to_owned()),
                ("emulator-5554".to_owned(), "unauthorized".to_owned()),
            ]
        );
    }

    #[test]
    fn a_connect_answer_tells_whether_the_port_answered() {
        assert!(connected("connected to 127.0.0.1:16384\n"));
        assert!(connected("already connected to 127.0.0.1:7555\n"));
        assert!(!connected(
            "cannot connect to 127.0.0.1:5555: No connection could be made (10061)\n"
        ));
        assert!(!connected(
            "failed to connect to '127.0.0.1:16416': Connection refused\n"
        ));
    }
}
