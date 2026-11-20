//! World-map navigation and the standard AFK Journey fishing minigame.
use super::{afk_stages as common, ocr, BASE_RESOLUTION};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::text_match::ascii_key;
use crate::afk_journey::vision;
use crate::{
    activity::{ActivityEvent, GameMode},
    adb,
    localization::tr,
};
use image::{DynamicImage, GrayImage, RgbaImage};
use std::{
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

type Point = (u32, u32);
type Rect = vision::Rect;

const HOOK: Rect = Rect {
    x: 390,
    y: 1250,
    width: 300,
    height: 345,
};
const ARRIVAL: Rect = Rect {
    x: 390,
    y: 1060,
    width: 300,
    height: 340,
};
const MAP_MARKERS: Rect = Rect {
    x: 40,
    y: 115,
    width: 930,
    height: 1310,
};
const RODS: &[&str] = &["fishing/fishing_rod.png", "fishing/fishing_rod_big.png"];
const BITES: &[&str] = &["fishing/hook_fish.png", "fishing/hook_fish_big.png"];
const PULL: &[&str] = &["fishing/hook.png", "fishing/hook_held.png"];

struct Frame {
    pixels: RgbaImage,
    gray: GrayImage,
    size: Point,
}

fn raw_image(bytes: &[u8]) -> Option<RgbaImage> {
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (width, height) = (word(0)?, word(4)?);
    if word(8)? != 1 || width == 0 || height == 0 {
        return None;
    }
    let size = usize::try_from(
        u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?,
    )
    .ok()?;
    let header = bytes.len().checked_sub(size)?;
    if !matches!(header, 12 | 16) {
        return None;
    }
    RgbaImage::from_raw(width, height, bytes[header..].to_vec())
}

impl Frame {
    fn find(
        &self,
        paths: &[&str],
        region: Rect,
        confidence: f32,
    ) -> Result<Option<Point>, ActionError> {
        Ok(vision::get_match(&self.gray, paths, region, confidence)?
            .map(|(_, found)| found.center()))
    }

    fn book(&self) -> Result<bool, ActionError> {
        Ok(self
            .find(
                &["fishing/book.png"],
                Rect {
                    x: 890,
                    y: 0,
                    width: 190,
                    height: 208,
                },
                0.72,
            )?
            .is_some())
    }

    fn cast_button(&self) -> Result<Option<Point>, ActionError> {
        let button = self.find(RODS, HOOK, 0.77)?;
        if button.is_some() && self.book()? {
            return Ok(button);
        }
        Ok(None)
    }
}

struct Driver<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
}

impl Driver<'_> {
    fn read(&self) -> Result<Frame, ActionError> {
        vision::ensure_not_cancelled(self.cancelled)?;
        let raw = adb::raw_screenshot(self.serial).map_err(|e| e.to_string())?;
        let pixels = match raw_image(&raw) {
            Some(image) => image,
            None => {
                image::load_from_memory(&adb::screenshot(self.serial).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
                    .into_rgba8()
            }
        };
        let size = pixels.dimensions();
        if (size.0 as f32 / size.1 as f32 - 9.0 / 16.0).abs() > 0.04 {
            return Err(tr!("fishing-fishing-requires-a-portrait-9-16").into());
        }
        let pixels = if size == BASE_RESOLUTION {
            pixels
        } else {
            image::imageops::resize(
                &pixels,
                BASE_RESOLUTION.0,
                BASE_RESOLUTION.1,
                image::imageops::FilterType::Triangle,
            )
        };
        let image = DynamicImage::ImageRgba8(pixels);
        let gray = image.to_luma8();
        Ok(Frame {
            pixels: image.into_rgba8(),
            gray,
            size,
        })
    }

    fn tap(&self, size: Point, point: Point) -> Result<(), ActionError> {
        vision::ensure_not_cancelled(self.cancelled)?;
        adb::tap_game_point(self.serial, point.0, point.1, size)
            .map_err(|e| e.to_string())
            .map_err(ActionError::from)
    }

    fn settle(&self) -> Result<(), ActionError> {
        vision::settle_cancellable(self.cancelled)
    }
}

fn is_map(frame: &Frame) -> Result<bool, ActionError> {
    let top = image::imageops::crop_imm(&frame.gray, 0, 0, 425, 200).to_image();
    Ok(ocr::recognize(&top)?
        .iter()
        .any(|l| ascii_key(&l.text).contains("explorationprogress")))
}

fn zero_attempts(label: &str) -> bool {
    let compact: String = label.chars().filter(|c| !c.is_whitespace()).collect();
    let Some((first, _)) = compact.split_once('/') else {
        return false;
    };
    first.chars().next_back() == Some('0')
        && !first
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|c| c.is_ascii_digit())
}

fn open_map(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Frame, ActionError> {
    let frame = driver.read()?;
    if is_map(&frame)? {
        return Ok(frame);
    }
    let size = common::navigate_to_current_overview(driver.serial, driver.cancelled, log)?;
    driver.tap(size, (903, 192))?;
    for _ in 0..8 {
        driver.settle()?;
        let frame = driver.read()?;
        if is_map(&frame)? {
            return Ok(frame);
        }
    }
    Err(tr!("fishing-could-not-verify-the-world-map").into())
}

fn next_marker(frame: &Frame, rejected: &[Point]) -> Result<Option<Point>, ActionError> {
    let mut masked;
    let gray = if rejected.is_empty() {
        &frame.gray
    } else {
        masked = frame.gray.clone();
        for &(x, y) in rejected {
            for py in y.saturating_sub(46)..(y + 46).min(masked.height()) {
                for px in x.saturating_sub(46)..(x + 46).min(masked.width()) {
                    masked.put_pixel(px, py, image::Luma([0]));
                }
            }
        }
        &masked
    };
    if let Some((_, full)) = vision::get_match(
        gray,
        &["fishing/map_marker.png", "fishing/map_marker_small.png"],
        MAP_MARKERS,
        0.83,
    )? {
        return Ok(Some(full.center()));
    }
    // The player's location arrow can cover the left half of an active spot.
    Ok(vision::get_match(
        gray,
        &[
            "fishing/map_marker_edge.png",
            "fishing/map_marker_edge_small.png",
        ],
        MAP_MARKERS,
        0.90,
    )?
    .map(|(_, edge)| edge.center()))
}

fn walking_button(frame: &Frame, marker: Point) -> Result<Option<Point>, ActionError> {
    let lines = ocr::recognize(&frame.gray)?;
    let nearby = |line: &&ocr::TextLine| {
        let (x, y) = line.center();
        x.abs_diff(marker.0) <= 450 && y.abs_diff(marker.1) <= 700
    };
    let title = lines
        .iter()
        .filter(nearby)
        .find(|l| ascii_key(&l.text).contains("ingspot"));
    // The title can be clipped at the left map edge. The fish badge beside the
    // counter also identifies the spot panel.
    let mut counter = None;
    for line in lines.iter().filter(nearby).filter(|l| l.text.contains('/')) {
        let (x, y) = line.center();
        if frame
            .find(
                &["fishing/map_counter.png"],
                Rect {
                    x: x.saturating_sub(150),
                    y: y.saturating_sub(38),
                    width: 155,
                    height: 76,
                },
                0.79,
            )?
            .is_some()
        {
            counter = Some(line);
            break;
        }
    }
    if title.is_none() && counter.is_none() {
        return Ok(None);
    }
    if counter.is_some_and(|l| zero_attempts(&l.text))
        || title.is_some_and(|l| zero_attempts(&l.text))
    {
        return Ok(None);
    }
    let top = marker.1.saturating_sub(240);
    let bottom = marker.1.saturating_sub(32);
    frame.find(
        &["fishing/map_walk.png"],
        Rect {
            x: marker.0.saturating_sub(200),
            y: top,
            width: 390,
            height: bottom.saturating_sub(top),
        },
        0.83,
    )
}

fn approach_spot(
    driver: &Driver<'_>,
    mut map: Frame,
    log: &mut impl FnMut(String),
) -> Result<bool, ActionError> {
    let mut rejected = Vec::new();
    let mut zoom_steps = 0;
    for _ in 0..22 {
        let Some(marker) = next_marker(&map, &rejected)? else {
            if zoom_steps == 4 {
                return Ok(false);
            }
            // The zoom-minus button is part of the verified region map.
            driver.tap(map.size, (1020, 1223))?;
            driver.settle()?;
            map = driver.read()?;
            if !is_map(&map)? {
                return Ok(false);
            }
            rejected.clear();
            zoom_steps += 1;
            continue;
        };
        driver.tap(map.size, marker)?;
        driver.settle()?;
        let panel = driver.read()?;
        if let Some(walk) = walking_button(&panel, marker)? {
            log(tr!("fishing-walking-to-the-fishing-spot").into());
            driver.tap(panel.size, walk)?;
            driver.settle()?;
            let deadline = Instant::now() + crate::timing::secs(65);
            while Instant::now() < deadline {
                let frame = driver.read()?;
                if let Some(rod) = frame.find(RODS, ARRIVAL, 0.76)? {
                    driver.tap(frame.size, rod)?;
                    driver.settle()?;
                    return Ok(true);
                }
                driver.settle()?;
            }
            return Err(tr!("fishing-the-character-did-not-reach-the").into());
        }
        rejected.push(marker);
    }
    Ok(false)
}

fn pull_duration(image: &RgbaImage) -> Option<u32> {
    let mut target = [0u16; 864];
    let mut hook = [0u16; 864];
    for x in 108..972 {
        for y in 980..1046 {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            target[x as usize - 108] +=
                u16::from(r.abs_diff(244) <= 15 && g.abs_diff(222) <= 15 && b.abs_diff(105) <= 15);
        }
        for y in 1046..1113 {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            hook[x as usize - 108] +=
                u16::from(r >= 250 && (194..=212).contains(&g) && (58..=83).contains(&b));
        }
    }
    let target_start = target.iter().position(|&total| total >= 2)?;
    let target_end = target.iter().rposition(|&total| total >= 2)?;
    let center = if target_end - target_start > 140 {
        (target_start + target_end) / 2
    } else {
        target_start
    };
    let (hook_left, samples) = hook
        .windows(50)
        .step_by(5)
        .enumerate()
        .map(|(i, window)| (i * 5, window.iter().map(|&v| u32::from(v)).sum::<u32>()))
        .max_by_key(|&(_, count)| count)?;
    if samples < 12 {
        return None;
    }
    match center.saturating_sub(hook_left + 25) {
        601.. => Some(950),
        401.. => Some(750),
        201.. => Some(450),
        101.. => Some(200),
        51.. => Some(100),
        _ => None,
    }
}

fn remaining_status(frame: &Frame) -> Result<Option<bool>, ActionError> {
    let crop = image::imageops::crop_imm(&frame.gray, 160, 120, 760, 135).to_image();
    Ok(ocr::recognize(&crop)?
        .iter()
        .find(|line| ascii_key(&line.text).contains("remainingattempts") && line.text.contains('/'))
        .map(|line| zero_attempts(&line.text)))
}

fn catch_close(frame: &Frame) -> Result<Option<Point>, ActionError> {
    const CLOSE_TOP: u32 = 1420;
    let close_region = image::imageops::crop_imm(
        &frame.gray,
        0,
        CLOSE_TOP,
        BASE_RESOLUTION.0,
        BASE_RESOLUTION.1 - CLOSE_TOP,
    )
    .to_image();
    let close_lines = ocr::recognize(&close_region)?;
    let close = close_lines
        .iter()
        .find(|l| ascii_key(&l.text) == "taptoclose");
    let Some(close) = close else {
        return Ok(None);
    };
    let receipt = ocr::recognize(&frame.gray)?.iter().any(|l| {
        let label = ascii_key(&l.text);
        label.contains("fishcaught")
            || label.contains("youcaught")
            || label.contains("caughtafish")
            || label.contains("newrecord")
            || label.contains("fishinglevel")
    }) || frame
        .find(
            &[
                "fishing/gold_medal.png",
                "fishing/gold_medal2.png",
                "fishing/silver_medal.png",
                "fishing/silver_medal2.png",
                "fishing/fishing_level.png",
                "fishing/fishing_level_big.png",
            ],
            Rect {
                x: 130,
                y: 330,
                width: 820,
                height: 1070,
            },
            0.83,
        )?
        .is_some();
    Ok(receipt.then(|| {
        let (x, y) = close.center();
        (x, y + CLOSE_TOP)
    }))
}

fn fish_spot(
    driver: &Driver<'_>,
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    let mut count = 0;
    let mut casts = 0;
    let mut hooked = false;
    let mut pull_frames = 0;
    let mut bite_visible_since = None::<Instant>;
    let mut last_action = Instant::now();
    let mut next_check = last_action;
    let deadline = Instant::now() + crate::timing::secs(330);
    while Instant::now() < deadline && casts < 24 {
        let frame = driver.read()?;
        if Instant::now() >= next_check {
            next_check = Instant::now() + Duration::from_secs(3);
            if frame.book()? && remaining_status(&frame)? == Some(true) {
                driver.tap(frame.size, (102, 1792))?;
                driver.settle()?;
                return Ok(count);
            }
        }
        if let Some(bite) = frame.find(BITES, HOOK, 0.77)? {
            let bite_since = bite_visible_since.get_or_insert_with(Instant::now);
            // Give the hook timing bar a moment to enter its active range.
            if (!hooked || last_action.elapsed() >= Duration::from_secs(1))
                && bite_since.elapsed() >= Duration::from_millis(500)
            {
                driver.tap(frame.size, bite)?;
                hooked = true;
                pull_frames = 0;
                last_action = Instant::now();
            }
        } else if let Some(control) = frame.find(PULL, HOOK, 0.70)? {
            bite_visible_since = None;
            hooked = true;
            pull_frames += 1;
            if let Some(ms) = pull_duration(&frame.pixels) {
                adb::hold_game_point(driver.serial, control, frame.size, ms)
                    .map_err(|e| e.to_string())?;
            } else if pull_frames % 4 == 0 {
                driver.tap(frame.size, (780, 1290))?;
            }
        } else if let Some(cast) = frame.cast_button()? {
            // Cast is also visible immediately before the cast animation starts.
            if last_action.elapsed() >= Duration::from_secs(2) {
                if remaining_status(&frame)? == Some(true) {
                    driver.tap(frame.size, (102, 1792))?;
                    driver.settle()?;
                    return Ok(count);
                }
                driver.tap(frame.size, cast)?;
                casts += 1;
                hooked = false;
                bite_visible_since = None;
                last_action = Instant::now();
            }
        } else {
            if let Some(close) = if hooked { catch_close(&frame)? } else { None } {
                count += 1;
                report(ActivityEvent::Collected(GameMode::Fishing, 1));
                driver.tap(frame.size, close)?;
                driver.settle()?;
                hooked = false;
                bite_visible_since = None;
                last_action = Instant::now();
            }
        }
        if last_action.elapsed() > crate::timing::secs(45) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(tr!("fishing-fishing-did-not-return-to-a").into())
}

pub fn run(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    report(ActivityEvent::ModeOpening(GameMode::Fishing));
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    let driver = Driver { serial, cancelled };
    let mut count = 0;
    for index in 0..12 {
        let already_fishing = if index == 0 {
            let frame = driver.read()?;
            frame.book()? || remaining_status(&frame)?.is_some()
        } else {
            false
        };
        if !already_fishing && !approach_spot(&driver, open_map(&driver, log)?, log)? {
            return if index > 0 {
                Ok(count)
            } else {
                Err(tr!("fishing-no-reachable-fishing-spot-with-attempts").into())
            };
        }
        report(ActivityEvent::ModeEntered(GameMode::Fishing));
        count += fish_spot(&driver, report)?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_count_distinguishes_zero_from_ten() {
        assert!(zero_attempts("Remaining attempts: 0 / 10"));
        assert!(!zero_attempts("Remaining attempts: 10/10"));
        assert!(!zero_attempts("Remaining attempts: 1/10"));
    }

    #[test]
    #[ignore = "requires captures of the fishing map, marker and cast screen"]
    fn captured_fishing_controls() {
        let captures = std::env::var_os("ADBHELPER_TEST_DAILY_MENU").expect("capture folder");
        let load = |name: &str| {
            let path = std::path::Path::new(&captures).join(format!("{name}.png"));
            let image = image::open(path).unwrap();
            let size = (image.width(), image.height());
            let image = image.resize_exact(
                BASE_RESOLUTION.0,
                BASE_RESOLUTION.1,
                image::imageops::FilterType::Triangle,
            );
            Frame {
                gray: image.to_luma8(),
                pixels: image.to_rgba8(),
                size,
            }
        };
        let map = load("fishing-map");
        assert!(is_map(&map).unwrap());
        let marker = next_marker(&map, &[]).unwrap().expect("fishing marker");
        assert!(marker.0.abs_diff(627) < 20 && marker.1.abs_diff(360) < 20);
        let overlap = next_marker(&load("fishing-route-test-map"), &[])
            .unwrap()
            .expect("marker beneath location arrow");
        assert!(
            overlap.0.abs_diff(570) < 30 && overlap.1.abs_diff(956) < 26,
            "{overlap:?}"
        );
        assert!(walking_button(&load("fishing-marker"), marker)
            .unwrap()
            .is_some());
        assert!(load("fishing-arrived")
            .find(RODS, ARRIVAL, 0.76)
            .unwrap()
            .is_some());
        assert!(load("fishing-after").cast_button().unwrap().is_some());
        let pulling = load("fishing-live3-current");
        assert!(
            pulling.find(PULL, HOOK, 0.70).unwrap().is_some(),
            "pull control"
        );
        assert!(
            matches!(pull_duration(&pulling.pixels), Some(450 | 750)),
            "pull duration"
        );
        assert_eq!(
            remaining_status(&load("fishing-before-optimized")).unwrap(),
            Some(true)
        );
        assert!(load("fishing-before-optimized").book().unwrap());
        let zoomed = next_marker(&load("fishing-live2-end"), &[])
            .unwrap()
            .expect("zoomed-out marker");
        assert!(
            zoomed.0.abs_diff(177) < 30 && zoomed.1.abs_diff(1125) < 30,
            "{zoomed:?}"
        );
        assert!(walking_button(&load("fishing-second-panel"), zoomed)
            .unwrap()
            .is_some());
    }

    #[test]
    #[ignore = "walks to a fishing spot and stops at the first confirmed catch"]
    fn connected_emulator_first_fish() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let cancelled = AtomicBool::new(false);
        let mut caught = 0;
        let result = run(
            &serial,
            &cancelled,
            &mut |line| println!("{line}"),
            &mut |event| {
                println!("{event:?}");
                if let ActivityEvent::Collected(GameMode::Fishing, amount) = event {
                    caught += amount;
                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            },
        );
        assert_eq!(caught, 1, "Fishing result: {result:?}");
    }
}
