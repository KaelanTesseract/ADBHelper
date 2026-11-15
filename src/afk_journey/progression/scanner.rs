//! Read-only game views and local AFKJ Tracker / Guild Manager exports.
mod guild;
mod hero;

use super::{common, text, Driver, Screen};
use crate::activity::{ActivityEvent, GameMode};
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
}

impl Default for GuildSettings {
    fn default() -> Self {
        Self {
            dream_days: 1,
            supreme_arena: false,
            activeness: true,
            afk_stages: false,
            guild_members_url: String::new(),
        }
    }
}

pub fn run(
    serial: &str,
    mode: GameMode,
    guild_settings: &GuildSettings,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
    exported: &mut impl FnMut(PathBuf),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
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
        GameMode::GuildScan => guild::scan(&driver, guild_settings, log, report, &mut save),
        _ => Err("Invalid scanner action".into()),
    }
}

fn write_export(folder: &Path, name: &str, value: &impl Serialize) -> Result<PathBuf, String> {
    let data = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let path = folder.join(name);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, data)
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn client() -> ureq::Agent {
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
) -> Result<Vec<super::ocr::TextLine>, String> {
    let mut lines = crate::afk_journey::daily::crop_lines(
        &screen.gray,
        common::Rect {
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
        let mode = if case == "hero" {
            GameMode::HeroScan
        } else {
            GameMode::GuildScan
        };
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
            &settings,
            &AtomicBool::new(false),
            &mut |line| println!("{line}"),
            &mut |event| {
                if !matches!(event, ActivityEvent::Collected(..)) {
                    println!("{event:?}");
                }
            },
            &mut |path| println!("Export: {}", path.display()),
        );
        println!("Scan result: {result:?}");
        assert!(result.is_ok_and(|rows| rows > 0));
    }
}
