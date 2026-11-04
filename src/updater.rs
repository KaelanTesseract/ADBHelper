use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "KaelanTesseract/ADBHelper";
pub const REPOSITORY_URL: &str = "https://github.com/KaelanTesseract/ADBHelper";
pub const ASSET_NAME: &str = "ADBHelper-Windows-x64.exe";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

pub enum CheckEvent {
    Started,
    Finished(Result<Option<Release>, String>),
}

pub fn watch_updates(requests: std::sync::mpsc::Receiver<()>, report: impl FnMut(CheckEvent)) {
    watch_with(CHECK_INTERVAL, requests, || check(VERSION), report);
}

fn watch_with(
    interval: Duration,
    requests: std::sync::mpsc::Receiver<()>,
    mut fetch: impl FnMut() -> Result<Option<Release>, String>,
    mut report: impl FnMut(CheckEvent),
) {
    loop {
        report(CheckEvent::Started);
        report(CheckEvent::Finished(fetch()));
        match requests.recv_timeout(interval) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
const MAX_SIZE: u64 = 250 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    download: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

fn parse_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let release: GithubRelease =
        serde_json::from_str(json).map_err(|e| format!("Ungültige Release-Antwort: {e}"))?;
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|e| e.to_string())?;
    if release.draft
        || release.prerelease
        || !version.pre.is_empty()
        || !version
            .cmp_precedence(&Version::parse(current).map_err(|e| e.to_string())?)
            .is_gt()
    {
        return Ok(None);
    }
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or("Das Release enthält keine Windows-x64-EXE.")?;
    let prefix = format!("{REPOSITORY_URL}/releases/download/");
    if !asset.browser_download_url.starts_with(&prefix) || !(4096..=MAX_SIZE).contains(&asset.size)
    {
        return Err("Ungültige Release-Datei.".into());
    }
    let sha256 = asset
        .digest
        .and_then(|d| d.strip_prefix("sha256:").map(str::to_owned))
        .ok_or("Das Release enthält keine SHA-256-Prüfsumme.")?;
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Ungültige Release-Prüfsumme.".into());
    }
    let notes = release
        .body
        .filter(|s| !s.trim().is_empty())
        .ok_or("Das Release enthält keine Patchnotes.")?;
    Ok(Some(Release {
        version: version.to_string(),
        notes,
        url: format!("{REPOSITORY_URL}/releases/tag/{}", release.tag_name),
        download: asset.browser_download_url,
        size: asset.size,
        sha256,
    }))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(timeout))
        .build()
        .into()
}

pub fn check(current: &str) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let mut response = match agent(Duration::from_secs(20))
        .get(&url)
        .header("User-Agent", &format!("ADBHelper/{VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(format!("Update-Prüfung fehlgeschlagen: {e}")),
    };
    let json = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse_release(&json, current)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_executable(path: &Path, size: u64, hash: &str) -> Result<(), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() != size {
        return Err("Der Download ist unvollständig.".into());
    }
    if sha256_file(path)? != hash.to_ascii_lowercase() {
        return Err("Die Prüfsumme des Downloads stimmt nicht überein.".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos).map_err(|e| e.to_string())?;
    let pe = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || pe > size.saturating_sub(94) {
        return Err("Keine gültige Windows-EXE.".into());
    }
    file.seek(SeekFrom::Start(pe)).map_err(|e| e.to_string())?;
    let mut header = [0_u8; 94];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..4] != b"PE\0\0"
        || u16::from_le_bytes([header[4], header[5]]) != 0x8664
        || u16::from_le_bytes([header[24], header[25]]) != 0x20b
        || u16::from_le_bytes([header[92], header[93]]) != 2
    {
        return Err("Das Update ist keine Windows-x64-GUI-Anwendung.".into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Job {
    target: PathBuf,
    staging: PathBuf,
    parent_pid: u32,
    size: u64,
    sha256: String,
    original_sha256: String,
    version: String,
}

struct StagingGuard {
    path: PathBuf,
    handed_off: bool,
}
impl Drop for StagingGuard {
    fn drop(&mut self) {
        if !self.handed_off {
            for name in ["new.exe", "updater.exe", "job.json", "ready", "status.json"] {
                let _ = fs::remove_file(self.path.join(name));
            }
            let _ = fs::remove_dir(&self.path);
        }
    }
}

#[derive(Serialize, Deserialize)]
struct InstallStatus {
    success: bool,
    message: String,
}

fn hidden_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Runs off the UI thread. The original app stays open until the helper is ready.
pub fn download_and_handoff(release: &Release, mut progress: impl FnMut(u8)) -> Result<(), String> {
    let target = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = target
        .parent()
        .ok_or("Kein Programmordner")?
        .join(format!(".adbhelper-update-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging).map_err(|e| format!("Der Programmordner ist nicht beschreibbar. Verschiebe ADBHelper in einen eigenen Ordner: {e}"))?;
    let mut guard = StagingGuard {
        path: staging.clone(),
        handed_off: false,
    };
    let download = staging.join("new.exe");
    let mut output = fs::File::create(&download).map_err(|e| e.to_string())?;
    let mut response = agent(Duration::from_secs(600))
        .get(&release.download)
        .header("User-Agent", &format!("ADBHelper/{VERSION}"))
        .call()
        .map_err(|e| format!("Download fehlgeschlagen: {e}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0_u8; 65536];
    let mut total = 0_u64;
    let mut last = 101;
    loop {
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > release.size || total > MAX_SIZE {
            return Err("Der Download ist größer als die Release-Datei.".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        let percent = (total * 100 / release.size) as u8;
        if percent != last {
            progress(percent);
            last = percent;
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    verify_executable(&download, release.size, &release.sha256)?;
    let job = Job {
        target: target.clone(),
        staging: staging.clone(),
        parent_pid: std::process::id(),
        size: release.size,
        sha256: release.sha256.clone(),
        original_sha256: sha256_file(&target)?,
        version: release.version.clone(),
    };
    let job_path = staging.join("job.json");
    fs::write(
        &job_path,
        serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let helper = staging.join("updater.exe");
    fs::copy(&target, &helper).map_err(|e| e.to_string())?;
    let mut child = hidden_command(&helper)
        .arg("--apply-update")
        .arg(&job_path)
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if staging.join("ready").is_file() {
            guard.handed_off = true;
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Update-Helfer konnte nicht starten ({status})."));
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    Err("Update-Helfer antwortet nicht; die bisherige Version bleibt geöffnet.".into())
}

fn read_job(path: &Path) -> Result<Job, String> {
    let job: Job = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let dir = fs::canonicalize(path.parent().ok_or("Ungültiger Update-Auftrag")?)
        .map_err(|e| e.to_string())?;
    if path.file_name().and_then(|s| s.to_str()) != Some("job.json")
        || fs::canonicalize(&job.staging).map_err(|e| e.to_string())? != dir
        || !dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .starts_with(".adbhelper-update-")
        || job.target.parent().and_then(|p| fs::canonicalize(p).ok())
            != dir.parent().map(Path::to_path_buf)
        || job.target.extension().and_then(|s| s.to_str()) != Some("exe")
    {
        return Err("Ungültiger Update-Auftrag.".into());
    }
    Ok(job)
}

#[cfg(windows)]
fn wait_for_parent(pid: u32, ready: &Path) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    if pid == std::process::id() {
        return Err("Ungültiger Elternprozess.".into());
    }
    // The handle prevents PID reuse while waiting for the original process to exit.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            if GetLastError() == ERROR_INVALID_PARAMETER {
                fs::write(ready, b"ready").map_err(|e| e.to_string())?;
                return Ok(());
            }
            return Err("Der bisherige Prozess kann nicht überwacht werden.".into());
        }
        let signal = fs::write(ready, b"ready");
        if signal.is_err() {
            CloseHandle(handle);
            return Err("Update-Bereitschaft konnte nicht gespeichert werden.".into());
        }
        let result = WaitForSingleObject(handle, 120_000);
        CloseHandle(handle);
        if result != WAIT_OBJECT_0 {
            return Err("Die bisherige App wurde nicht rechtzeitig beendet.".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn wait_for_parent(_: u32, _: &Path) -> Result<(), String> {
    Err("Automatische Installation ist nur für Windows verfügbar.".into())
}

fn rename_with_retry(source: &Path, destination: &Path) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match fs::rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                ) && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error),
        }
    }
}

fn replace_and_launch(
    target: &Path,
    source: &Path,
    backup: &Path,
    launch: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), String> {
    rename_with_retry(target, backup)
        .map_err(|e| format!("Alte EXE konnte nicht freigegeben werden: {e}"))?;
    if let Err(error) = rename_with_retry(source, target) {
        rename_with_retry(backup, target).map_err(|restore| format!("Installation fehlgeschlagen ({error}); Wiederherstellung fehlgeschlagen ({restore}). Sicherung: {}", backup.display()))?;
        return Err(format!(
            "Installation fehlgeschlagen; alte Version wiederhergestellt: {error}"
        ));
    }
    if let Err(error) = launch(target) {
        rename_with_retry(target, source).and_then(|_| rename_with_retry(backup, target)).map_err(|restore| format!("Start fehlgeschlagen ({error}); Wiederherstellung fehlgeschlagen ({restore}). Sicherung: {}", backup.display()))?;
        return Err(format!(
            "Start fehlgeschlagen; alte Version wiederhergestellt: {error}"
        ));
    }
    Ok(())
}

pub fn apply_update(path: &Path) -> Result<(), String> {
    let job = read_job(path)?;
    if std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?
        != job.staging.join("updater.exe")
    {
        return Err("Update muss vom separaten Helfer ausgeführt werden.".into());
    }
    let source = job.staging.join("new.exe");
    verify_executable(&source, job.size, &job.sha256)?;
    wait_for_parent(job.parent_pid, &job.staging.join("ready"))?;
    let result_path = job.staging.join("status.json");
    let status = InstallStatus {
        success: true,
        message: format!("ADBHelper wurde auf Version {} aktualisiert.", job.version),
    };
    let result = (|| {
        if sha256_file(&job.target)? != job.original_sha256 {
            return Err(
                "Die installierte EXE wurde zwischenzeitlich verändert; Update abgebrochen.".into(),
            );
        }
        fs::write(
            &result_path,
            serde_json::to_vec(&status).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        replace_and_launch(
            &job.target,
            &source,
            &job.staging.join("previous.exe"),
            |target| {
                hidden_command(target)
                    .arg("--update-result")
                    .arg(path)
                    .current_dir(target.parent().unwrap())
                    .spawn()
                    .map(|_| ())
            },
        )
    })();
    if let Err(error) = &result {
        let status = InstallStatus {
            success: false,
            message: error.clone(),
        };
        let _ = fs::write(
            &result_path,
            serde_json::to_vec(&status).unwrap_or_default(),
        );
        if job.target.is_file() {
            let _ = hidden_command(&job.target)
                .arg("--update-result")
                .arg(path)
                .spawn();
        }
    }
    result
}

pub fn startup_notice() -> Option<String> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.to_str()? != "--update-result" {
        return None;
    }
    let path = PathBuf::from(args.next()?);
    let job = read_job(&path).ok()?;
    if std::env::current_exe().and_then(fs::canonicalize).ok()? != job.target {
        return None;
    }
    let status: InstallStatus =
        serde_json::from_slice(&fs::read(job.staging.join("status.json")).ok()?).ok()?;
    // Keep the old executable until the new GUI has successfully initialized.
    if status.success && verify_executable(&job.target, job.size, &job.sha256).is_ok() {
        thread::spawn(move || {
            for _ in 0..30 {
                for name in ["previous.exe", "new.exe", "updater.exe"] {
                    let _ = fs::remove_file(job.staging.join(name));
                }
                if !job.staging.join("updater.exe").exists() {
                    for name in ["ready", "status.json", "job.json"] {
                        let _ = fs::remove_file(job.staging.join(name));
                    }
                    let _ = fs::remove_dir(&job.staging);
                    break;
                }
                thread::sleep(Duration::from_millis(200));
            }
        });
    }
    Some(status.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_continue_without_ui_events_and_stop_after_sender_is_dropped() {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            watch_with(
                Duration::from_millis(20),
                request_rx,
                || Ok(None),
                |event| {
                    if let CheckEvent::Finished(result) = event {
                        result_tx.send(result.is_ok()).unwrap();
                    }
                },
            )
        });
        assert!(result_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(result_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        drop(request_tx);
        worker.join().unwrap();
    }

    #[test]
    fn manual_check_wakes_the_same_worker_after_a_network_error() {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let mut count = 0;
            watch_with(
                Duration::from_secs(60),
                request_rx,
                || {
                    count += 1;
                    if count == 1 {
                        Err("offline".into())
                    } else {
                        Ok(None)
                    }
                },
                |event| {
                    if let CheckEvent::Finished(result) = event {
                        result_tx.send(result.is_ok()).unwrap();
                    }
                },
            );
        });
        assert!(!result_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        request_tx.send(()).unwrap();
        assert!(result_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        drop(request_tx);
        worker.join().unwrap();
    }

    #[test]
    fn each_check_reports_started_before_result() {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        drop(request_tx);
        let mut events = Vec::new();
        watch_with(
            Duration::from_secs(60),
            request_rx,
            || Ok(None),
            |event| events.push(matches!(event, CheckEvent::Started)),
        );
        assert_eq!(events, vec![true, false]);
    }
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let root = std::env::current_dir()
                .unwrap()
                .join("target")
                .join("update-tests");
            fs::create_dir_all(&root).unwrap();
            let path = root.join(format!(
                "adbhelper-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            for name in ["target.exe", "new.exe", "previous.exe"] {
                let _ = fs::remove_file(self.file(name));
            }
            let _ = fs::remove_dir(&self.0);
        }
    }
    fn release(version: &str) -> serde_json::Value {
        serde_json::json!({"tag_name":version,"body":"## Änderungen\n- Update-Funktion verbessert.","assets":[{"name":ASSET_NAME,"browser_download_url":format!("{REPOSITORY_URL}/releases/download/{version}/{ASSET_NAME}"),"size":8192,"digest":format!("sha256:{}", "a".repeat(64))}]})
    }
    #[test]
    fn version_order_uses_semver_not_text_or_build_metadata() {
        assert!(parse_release(&release("v0.10.0").to_string(), "0.9.0")
            .unwrap()
            .is_some());
        assert!(parse_release(&release("v0.2.0").to_string(), "0.2.0")
            .unwrap()
            .is_none());
        assert!(
            parse_release(&release("v0.2.0+build2").to_string(), "0.2.0+build1")
                .unwrap()
                .is_none()
        );
        assert!(parse_release(&release("v0.2.0-rc.1").to_string(), "0.1.0")
            .unwrap()
            .is_none());
    }
    #[test]
    fn release_requires_notes_hash_and_the_expected_repository_asset() {
        for changed in ["notes", "digest", "url", "name", "size"] {
            let mut json = release("v0.3.0");
            match changed {
                "notes" => json["body"] = serde_json::json!("  "),
                "digest" => json["assets"][0]["digest"] = serde_json::Value::Null,
                "url" => {
                    json["assets"][0]["browser_download_url"] =
                        serde_json::json!("https://example.com/evil.exe")
                }
                "name" => json["assets"][0]["name"] = serde_json::json!("other.exe"),
                _ => json["assets"][0]["size"] = serde_json::json!(MAX_SIZE + 1),
            }
            assert!(
                parse_release(&json.to_string(), "0.2.0").is_err(),
                "{changed}"
            );
        }
        let mut draft = release("v0.3.0");
        draft["draft"] = true.into();
        assert!(parse_release(&draft.to_string(), "0.2.0")
            .unwrap()
            .is_none());
    }
    #[test]
    fn verifies_download_integrity_and_windows_architecture_before_install() {
        let dir = TestDir::new();
        let mut bytes = vec![0_u8; 4096];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&0x8664_u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        bytes[220..222].copy_from_slice(&2_u16.to_le_bytes());
        let path = dir.file("new.exe");
        fs::write(&path, &bytes).unwrap();
        let hash = sha256_file(&path).unwrap();
        verify_executable(&path, 4096, &hash).unwrap();
        assert!(verify_executable(&path, 4095, &hash).is_err());
        assert!(verify_executable(&path, 4096, &"0".repeat(64)).is_err());
        bytes[132..134].copy_from_slice(&0x14c_u16.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(verify_executable(&path, 4096, &sha256_file(&path).unwrap()).is_err());
    }
    #[test]
    fn successful_install_replaces_the_original_path_and_retains_backup_until_gui_start() {
        let dir = TestDir::new();
        let target = dir.file("target.exe");
        let source = dir.file("new.exe");
        let backup = dir.file("previous.exe");
        fs::write(&target, b"old").unwrap();
        fs::write(&source, b"new").unwrap();
        replace_and_launch(&target, &source, &backup, |path| {
            assert_eq!(fs::read(path).unwrap(), b"new");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"old");
    }
    #[test]
    fn failed_start_restores_old_executable_and_keeps_download() {
        let dir = TestDir::new();
        let target = dir.file("target.exe");
        let source = dir.file("new.exe");
        let backup = dir.file("previous.exe");
        fs::write(&target, b"old").unwrap();
        fs::write(&source, b"new").unwrap();
        let error = replace_and_launch(&target, &source, &backup, |_| {
            Err(std::io::Error::other("cannot launch"))
        })
        .unwrap_err();
        assert!(error.contains("Start fehlgeschlagen"), "{error}");
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert_eq!(fs::read(&source).unwrap(), b"new");
        assert!(!backup.exists());
    }
    #[test]
    fn failed_move_restores_original_without_launching() {
        let dir = TestDir::new();
        let target = dir.file("target.exe");
        let source = dir.file("new.exe");
        let backup = dir.file("previous.exe");
        fs::write(&target, b"old").unwrap();
        let error = replace_and_launch(&target, &source, &backup, |_| panic!("must not launch"))
            .unwrap_err();
        assert!(error.contains("Installation fehlgeschlagen"), "{error}");
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!backup.exists());
    }
}
