//! Optional hero, equipment and shopping steps in the Daily routine.
mod hall;
mod shop;

use super::{afk_stages as common, ocr, phase_rewards};
use crate::{
    activity::{ActivityEvent, GameMode},
    adb,
    localization::{format_text, text},
    routine::EmporiumSettings,
};
use image::{GrayImage, RgbImage};
use std::{collections::BTreeMap, ops::Range, sync::atomic::AtomicBool};

type Point = (u32, u32);

#[derive(Default)]
pub struct RunState {
    completed: BTreeMap<GameMode, u32>,
    // A crash after an irreversible input can leave its outcome unknown.
    pending: bool,
    purchased_essences: Vec<String>,
    affinity_attempted: Vec<hall::HeroNameplate>,
    shop: shop::State,
}

struct Screen {
    gray: GrayImage,
    color: RgbImage,
    lines: Vec<ocr::TextLine>,
    size: Point,
}

fn key(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

impl Screen {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let (gray, size) = common::normalize_screenshot(bytes)?;
        let color = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .resize_exact(
                gray.width(),
                gray.height(),
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();
        let lines = ocr::recognize(&gray)?;
        Ok(Self {
            gray,
            color,
            lines,
            size,
        })
    }

    fn line(
        &self,
        x: Range<u32>,
        y: Range<u32>,
        matches: impl Fn(&str) -> bool,
    ) -> Option<&ocr::TextLine> {
        self.lines.iter().find(|line| {
            let (px, py) = line.center();
            x.contains(&px) && y.contains(&py) && matches(&key(&line.text))
        })
    }

    fn point(&self, x: Range<u32>, y: Range<u32>, name: &str) -> Option<Point> {
        self.line(x, y, |value| value == name)
            .map(ocr::TextLine::center)
    }

    fn template(
        &self,
        path: &str,
        x: Range<u32>,
        y: Range<u32>,
        confidence: f32,
    ) -> Result<Option<Point>, String> {
        common::find_template(
            &self.gray,
            path,
            common::Rect {
                x: x.start,
                y: y.start,
                width: x.end - x.start,
                height: y.end - y.start,
            },
            confidence,
        )
        .map(|found| found.map(|m| m.center()))
    }

    fn receipt_close(&self) -> Option<Point> {
        phase_rewards::loot_close(&self.lines).or_else(|| {
            self.line(200..880, 200..1050, |label| {
                matches!(
                    label,
                    "treasureobtained"
                        | "treasurebtained"
                        | "successfullypurchased"
                        | "purchasesuccessful"
                        | "swapsuccessful"
                        | "successfullyswapped"
                )
            })?;
            self.point(300..780, 1630..1880, "taptoclose")
        })
    }
}

struct Driver<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
}

impl Driver<'_> {
    fn read(&self) -> Result<Screen, String> {
        common::ensure_not_cancelled(self.cancelled)?;
        let bytes = adb::screenshot(self.serial).map_err(|error| error.to_string())?;
        let screen = Screen::decode(&bytes)?;
        common::ensure_not_cancelled(self.cancelled)?;
        Ok(screen)
    }

    fn wait_for(&self, mut ready: impl FnMut(&Screen) -> bool) -> Result<Option<Screen>, String> {
        for _ in 0..8 {
            let screen = self.read()?;
            if ready(&screen) {
                return Ok(Some(screen));
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Ok(None)
    }

    fn tap_at(&self, size: Point, (x, y): Point) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        adb::tap_game_point(self.serial, x, y, size).map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)
    }

    fn tap(&self, screen: &Screen, point: Point) -> Result<(), String> {
        self.tap_at(screen.size, point)
    }

    fn back(&self) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        adb::press_back(self.serial).map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)
    }

    fn overview_tab(&self, x: u32, log: &mut impl FnMut(String)) -> Result<(), String> {
        let size = common::navigate_to_current_overview(self.serial, self.cancelled, log)?;
        self.tap_at(size, (x, 1830))
    }
}

struct Progress<'a> {
    mode: GameMode,
    state: &'a mut RunState,
    report: &'a mut dyn FnMut(ActivityEvent),
}

impl Progress<'_> {
    fn entered(&mut self) {
        (self.report)(ActivityEvent::ModeEntered(self.mode));
    }

    fn count(&self) -> u32 {
        self.state.completed.get(&self.mode).copied().unwrap_or(0)
    }

    fn completed(&mut self) {
        self.state.pending = false;
        *self.state.completed.entry(self.mode).or_default() += 1;
        (self.report)(ActivityEvent::Collected(self.mode, 1));
    }
}

pub fn run(
    serial: &str,
    mode: GameMode,
    settings: EmporiumSettings,
    state: &mut RunState,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    if state.pending {
        return Err(text(
            "The previous operation could not be confirmed. Check its result in the game.",
            "Die letzte Änderung konnte nicht bestätigt werden. Bitte prüfe das Ergebnis im Spiel.",
        )
        .into());
    }
    report(ActivityEvent::ModeOpening(mode));
    let driver = Driver { serial, cancelled };
    let mut progress = Progress {
        mode,
        state,
        report,
    };
    match mode {
        GameMode::CollectAffinity => hall::affinity(&driver, &mut progress, log)?,
        GameMode::LevelUpHeroes => hall::level_up(&driver, &mut progress, log)?,
        GameMode::EquipNewEquipment => hall::equipment(&driver, &mut progress, log)?,
        GameMode::EssenceSwap => hall::swap(&driver, &mut progress, log)?,
        GameMode::Emporium => shop::purchase(&driver, &mut progress, settings, log)?,
        GameMode::SinglePull => shop::recruit(&driver, &mut progress, log)?,
        _ => {
            return Err(format_text!(
                "Invalid progression step {}",
                "Ungültiger Fortschrittsschritt {}",
                mode.label()
            ))
        }
    }
    Ok(progress.count())
}

#[cfg(test)]
fn captured(name: &str) -> Screen {
    let root =
        std::path::PathBuf::from(std::env::var_os("ADBHELPER_TEST_PROGRESSION_CAPTURES").unwrap());
    let bytes = std::fs::read(root.join(format!("{name}.png"))).unwrap();
    Screen::decode(&bytes).unwrap()
}

#[test]
#[ignore = "opens gameplay views on a connected AFK Journey device"]
fn connected_emulator_progression_previews() {
    let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
    let cancelled = AtomicBool::new(false);
    let driver = Driver {
        serial: &serial,
        cancelled: &cancelled,
    };
    let log = &mut |message| println!("{message}");
    hall::preview(&driver, log).unwrap();
    shop::preview(&driver, log).unwrap();
}
