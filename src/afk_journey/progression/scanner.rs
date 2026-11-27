//! Read-only game views and local AFKJ Tracker / Guild Manager exports.
use crate::afk_journey::error::ActionError;
mod guild;
mod hero;
mod hero_db_scan;

use super::{tr, Driver, Screen};
use crate::activity::{ActivityEvent, GameMode};
use crate::afk_journey::vision;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct GuildSettings {
    pub dream_days: u8,
    pub supreme_arena: bool,
    pub activeness: bool,
    pub afk_stages: bool,
    pub guild_members_url: String,
    /// Supabase `apikey`; kept in the Windows Credential Manager, never in files.
    pub guild_api_key: String,
}

const GUILD_CREDENTIAL: &str = "ADBHelper/GuildManager";

impl GuildSettings {
    /// Restores the Guild Manager URL and key saved by an earlier scan.
    pub fn load_saved_access(&mut self) {
        if let Some(saved) = crate::credentials::load(GUILD_CREDENTIAL) {
            self.guild_members_url = saved.user;
            self.guild_api_key = saved.secret;
        }
    }

    /// Moves an `apikey` pasted as part of the URL into the separate key field.
    pub fn take_pasted_key(&mut self) -> bool {
        let (url, key) = guild::split_access(&self.guild_members_url);
        match key {
            Some(key) => {
                self.guild_members_url = url;
                self.guild_api_key = key;
                true
            }
            None => false,
        }
    }

    /// Saves URL and key in the credential store, or removes them when both are empty.
    pub fn save_access(&self) -> Result<(), ActionError> {
        let url = self.guild_members_url.trim();
        let key = self.guild_api_key.trim();
        if url.is_empty() && key.is_empty() {
            return crate::credentials::delete(GUILD_CREDENTIAL).map_err(ActionError::from);
        }
        crate::credentials::save(
            GUILD_CREDENTIAL,
            &crate::credentials::Credential {
                user: url.to_owned(),
                secret: key.to_owned(),
            },
        )
        .map_err(ActionError::from)
    }

    pub fn forget_access(&mut self) -> Result<(), ActionError> {
        self.guild_members_url.clear();
        self.guild_api_key.clear();
        crate::credentials::delete(GUILD_CREDENTIAL).map_err(ActionError::from)
    }
}

impl Default for GuildSettings {
    fn default() -> Self {
        Self {
            dream_days: 1,
            supreme_arena: false,
            activeness: true,
            afk_stages: false,
            guild_members_url: String::new(),
            guild_api_key: String::new(),
        }
    }
}

/// What the scanners need besides the device.
pub struct ScanOptions<'a> {
    pub guild: &'a GuildSettings,
    /// Hero database: skip heroes whose stats are newer than this.
    pub hero_db_max_age_days: Option<u32>,
}

pub fn run(
    serial: &str,
    mode: GameMode,
    options: &ScanOptions<'_>,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
    exported: &mut impl FnMut(PathBuf),
) -> Result<u32, ActionError> {
    vision::ensure_not_cancelled(cancelled)?;
    report(ActivityEvent::ModeOpening(mode));
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let folder = crate::settings::file("scans")
        .map_err(|error| error.to_string())?
        .join(format!("{millis}-{}", std::process::id()));
    let driver = Driver { serial, cancelled };
    crate::adb::launch_afk_journey(serial).map_err(|error| error.to_string())?;
    report(ActivityEvent::ModeEntered(mode));
    let mut save = |name: &str, data: &Value| {
        let path = write_export(&folder, name, data)?;
        exported(path);
        Ok(())
    };
    match mode {
        GameMode::HeroScan => hero::scan(&driver, log, report, &mut save),
        GameMode::GuildScan => guild::scan(&driver, options.guild, log, report, &mut save),
        GameMode::HeroDatabase => {
            hero_db_scan::scan(&driver, options.hero_db_max_age_days, log, report)
        }
        _ => Err("Invalid scanner action".into()),
    }
}

fn write_export(folder: &Path, name: &str, value: &impl Serialize) -> Result<PathBuf, ActionError> {
    let data = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let path = folder.join(name);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, data)
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

pub(in crate::afk_journey::progression) fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into()
}

fn crop(
    screen: &Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<super::ocr::TextLine>, ActionError> {
    let mut lines = crate::afk_journey::daily::crop_lines(
        &screen.gray,
        vision::Rect {
            x,
            y,
            width,
            height,
        },
    )?;
    for line in &mut lines {
        line.x = line.x / 3.0 + x as f32 - 16.0;
        line.y = line.y / 3.0 + y as f32 - 16.0;
        line.width /= 3.0;
        line.height /= 3.0;
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "runs a scanner in a connected AFK Journey emulator"]
    fn connected_emulator_scanner() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("device serial");
        let case = std::env::var("ADBHELPER_TEST_SCANNER").expect("scanner kind");
        let mode = match case.as_str() {
            "hero" => GameMode::HeroScan,
            "herodb" => GameMode::HeroDatabase,
            _ => GameMode::GuildScan,
        };
        // ADBHELPER_HERO_DB_LIMIT stops the hero database scan after that many heroes.
        let limit: Option<u32> = std::env::var("ADBHELPER_HERO_DB_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok());
        let cancelled = AtomicBool::new(false);
        let mut collected = 0;
        let settings = GuildSettings {
            dream_days: u8::from(case == "dream" || case == "guild"),
            supreme_arena: case == "supreme",
            activeness: case == "activity" || case == "guild",
            afk_stages: case == "afk",
            ..GuildSettings::default()
        };
        let result = run(
            &serial,
            mode,
            &ScanOptions {
                guild: &settings,
                hero_db_max_age_days: None,
            },
            &cancelled,
            &mut |line| println!("{line}"),
            &mut |event| {
                if let ActivityEvent::Collected(..) = event {
                    collected += 1;
                    if limit.is_some_and(|limit| collected >= limit) {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                } else {
                    println!("{event:?}");
                }
            },
            &mut |path| println!("Export: {}", path.display()),
        );
        println!("Scan result: {result:?}");
        if limit.is_some() {
            assert!(collected > 0);
        } else {
            assert!(result.is_ok_and(|rows| rows > 0));
        }
    }
}
