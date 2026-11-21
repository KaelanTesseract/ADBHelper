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

// Used by the map navigation that follows in the next step.
#[allow(dead_code)]
mod route;

type Point = (u32, u32);
type Rect = vision::Rect;

const HOOK: Rect = Rect {
    x: 390,
    y: 1250,
    width: 300,
    height: 345,
};
// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
const ARRIVAL: Rect = Rect {
    x: 390,
    y: 1060,
    width: 300,
    height: 340,
};
// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
const MAP_MARKERS: Rect = Rect {
    x: 40,
    y: 115,
    width: 930,
    height: 1310,
};
/// Centre of the cast / bite / Tap & Hold button on the fishing screen.
const ROD_BUTTON: Point = (541, 1449);
/// The Strong Pull skill button beside Tap & Hold ("Skill is available. Use it
/// now!"); tapped every fourth pull frame like ADB AutoPlayer.
const STRONG_PULL: Point = (780, 1290);
const RODS: &[&str] = &["fishing/fishing_rod.png", "fishing/fishing_rod_big.png"];
const BITES: &[&str] = &["fishing/hook_fish.png", "fishing/hook_fish_big.png"];
const PULL: &[&str] = &["fishing/hook.png", "fishing/hook_held.png"];

/// Diagnostic recording for fishing, active only when the environment variable
/// `ADBHELPER_FISHING_TRACE` names a folder. Frames are saved at half size on a
/// background thread so the minigame timing stays as it is.
mod trace {
    use image::RgbaImage;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    struct Sink {
        started: Instant,
        frames: Mutex<Sender<(u32, RgbaImage)>>,
        notes: Mutex<std::fs::File>,
    }

    static SINK: OnceLock<Option<Sink>> = OnceLock::new();
    static FRAME: AtomicU32 = AtomicU32::new(0);

    fn sink() -> Option<&'static Sink> {
        SINK.get_or_init(|| {
            let folder = std::path::PathBuf::from(std::env::var_os("ADBHELPER_FISHING_TRACE")?);
            std::fs::create_dir_all(&folder).ok()?;
            let notes = std::fs::File::create(folder.join("trace.txt")).ok()?;
            let (sender, receiver) = channel::<(u32, RgbaImage)>();
            std::thread::spawn(move || {
                for (number, image) in receiver {
                    let small = image::imageops::resize(
                        &image,
                        image.width() / 2,
                        image.height() / 2,
                        image::imageops::FilterType::Triangle,
                    );
                    let _ = small.save(folder.join(format!("frame-{number:05}.png")));
                }
            });
            Some(Sink {
                started: Instant::now(),
                frames: Mutex::new(sender),
                notes: Mutex::new(notes),
            })
        })
        .as_ref()
    }

    pub(super) fn frame(image: &RgbaImage) {
        static LAST_SAVED: Mutex<Option<Instant>> = Mutex::new(None);
        if let Some(sink) = sink() {
            let number = FRAME.fetch_add(1, Ordering::Relaxed) + 1;
            let due = LAST_SAVED.lock().is_ok_and(|mut last| {
                let due = last.is_none_or(|at| at.elapsed().as_millis() >= 140);
                if due {
                    *last = Some(Instant::now());
                }
                due
            });
            if due {
                if let Ok(sender) = sink.frames.lock() {
                    let _ = sender.send((number, image.clone()));
                }
            }
            note(&format!(
                "frame {number}{}",
                if due { " (saved)" } else { "" }
            ));
        }
    }

    pub(super) fn note(message: &str) {
        if let Some(sink) = sink() {
            let elapsed = sink.started.elapsed().as_secs_f32();
            let frame = FRAME.load(Ordering::Relaxed);
            if let Ok(mut file) = sink.notes.lock() {
                let _ = writeln!(file, "{elapsed:9.3}s  #{frame:05}  {message}");
            }
        }
    }
}

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

/// The open fishing screen: the fish book at the top right and a minigame
/// control (cast, bite, or pull) at the bottom, as in ADB AutoPlayer.
fn on_fishing_screen(frame: &Frame) -> Result<bool, ActionError> {
    if !frame.book()? {
        return Ok(false);
    }
    let controls: Vec<&str> = RODS.iter().chain(BITES).chain(PULL).copied().collect();
    Ok(frame.find(&controls, HOOK, 0.70)?.is_some())
}

struct Driver<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    /// Live frames for the minigame; `None` falls back to screenshots.
    stream: Option<vision::ScreenStream>,
    seen: std::cell::Cell<u64>,
    /// True while a background hold or Strong Pull tap runs: inputs never
    /// overlap, because Android's `input` has a single pointer.
    input_busy: std::sync::Arc<AtomicBool>,
}

impl Driver<'_> {
    fn read(&self) -> Result<Frame, ActionError> {
        vision::ensure_not_cancelled(self.cancelled)?;
        let streamed = self.stream.as_ref().and_then(|stream| {
            let (number, image) = stream.next_after(self.seen.get(), Duration::from_millis(500))?;
            self.seen.set(number);
            Some(image)
        });
        let pixels = match streamed {
            Some(image) => image,
            None => self.screenshot()?,
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
        trace::frame(&pixels);
        let image = DynamicImage::ImageRgba8(pixels);
        let gray = image.to_luma8();
        Ok(Frame {
            pixels: image.into_rgba8(),
            gray,
            size,
        })
    }

    fn screenshot(&self) -> Result<RgbaImage, ActionError> {
        let raw = adb::raw_screenshot(self.serial).map_err(|e| e.to_string())?;
        Ok(match raw_image(&raw) {
            Some(image) => image,
            None => {
                image::load_from_memory(&adb::screenshot(self.serial).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
                    .into_rgba8()
            }
        })
    }

    /// Runs one input on a background thread unless the previous one still runs.
    fn input_in_background(
        &self,
        lane: &std::sync::Arc<AtomicBool>,
        what: String,
        input: impl FnOnce(&str) -> Result<(), adb::AdbError> + Send + 'static,
    ) -> bool {
        if lane.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return false;
        }
        trace::note(&what);
        let busy = lane.clone();
        let serial = self.serial.to_owned();
        thread::spawn(move || {
            let _ = input(&serial);
            busy.store(false, std::sync::atomic::Ordering::Release);
        });
        true
    }

    fn tap(&self, size: Point, point: Point) -> Result<(), ActionError> {
        vision::ensure_not_cancelled(self.cancelled)?;
        trace::note(&format!("tap {point:?}"));
        adb::tap_game_point(self.serial, point.0, point.1, size)
            .map_err(|e| e.to_string())
            .map_err(ActionError::from)
    }

    fn settle(&self) -> Result<(), ActionError> {
        vision::settle_cancellable(self.cancelled)
    }
}

// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
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
    first.ends_with('0')
        && !first
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|c| c.is_ascii_digit())
}

// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
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

// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
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

// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
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

// Parked: map navigation is off until the minigame works reliably (2026-11-21).
#[allow(dead_code)]
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
        trace::note(&format!("map marker {marker:?}"));
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
                    trace::note("arrived, rod visible");
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

/// The pull bar of the minigame, read by colour like ADB AutoPlayer's
/// `_find_fishing_colors_fast`: the yellow target box and the orange hook, as
/// x positions inside the bar (0 = 10 % from the left edge).
fn pull_bar(image: &RgbaImage) -> Option<(usize, usize)> {
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
    (samples >= 12).then_some((center, hook_left + 25))
}

fn pull_duration(image: &RgbaImage) -> Option<u32> {
    let (center, hook) = pull_bar(image)?;
    // Hold times as in ADB AutoPlayer's _handle_hold_for_distance.
    match center.saturating_sub(hook) {
        601.. => Some(600),
        401.. => Some(450),
        201.. => Some(300),
        101.. => Some(200),
        51.. => Some(100),
        _ => None,
    }
}

/// "Remaining attempts: 3/10" as (3, 10); the separator may be "/", a space,
/// or anything else that is not a digit, because OCR does not always keep "/".
fn remaining_attempts(text: &str) -> Option<(u32, u32)> {
    let lower = text.to_ascii_lowercase();
    let after = &lower[lower.find("attempts")? + "attempts".len()..];
    let mut numbers = after
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::parse::<u32>);
    let left = numbers.next()?.ok()?;
    let total = numbers.next()?.ok()?;
    (numbers.next().is_none() && left <= total && total > 0).then_some((left, total))
}

/// `Some(true)` when the open spot has no attempts left.
fn remaining_status(frame: &Frame) -> Result<Option<bool>, ActionError> {
    let crop = image::imageops::crop_imm(&frame.gray, 160, 120, 760, 135).to_image();
    Ok(ocr::recognize(&crop)?
        .iter()
        .filter(|line| ascii_key(&line.text).contains("remainingattempts"))
        .find_map(|line| remaining_attempts(&line.text))
        .map(|(left, _)| left == 0))
}

/// A weight such as "2.01kg" on the catch card ("201kg" after the key).
fn is_weight(label: &str) -> bool {
    label
        .strip_suffix("kg")
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

/// A "Tap to close" popup in fishing: where to tap, and whether it shows a catch
/// (a treasure popup is closed but not counted, as in ADB AutoPlayer).
fn catch_close(frame: &Frame) -> Result<Option<(Point, bool)>, ActionError> {
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
        label.contains("firstcatch")
            || is_weight(&label)
            || label.contains("fishcaught")
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
    let (x, y) = close.center();
    Ok(Some(((x, y + CLOSE_TOP), receipt)))
}

/// The outcome of one cast. A small fish has no pull phase and no catch card:
/// when the bite was hit and the fish did not escape, it was caught silently.
#[derive(Default)]
struct Round {
    hooked: bool,
    escaped: bool,
    counted: bool,
}

impl Round {
    /// A catch card was closed; returns whether it adds a catch.
    fn card(&mut self, caught: bool) -> bool {
        let new = caught && !self.counted;
        self.counted |= new;
        new
    }

    fn silent_catch(&self) -> bool {
        self.hooked && !self.escaped && !self.counted
    }
}

/// "The fish has escaped! Try again!" in the middle of the fishing screen.
fn fish_escaped(frame: &Frame) -> Result<bool, ActionError> {
    let band = image::imageops::crop_imm(&frame.gray, 0, 1000, BASE_RESOLUTION.0, 300).to_image();
    Ok(ocr::recognize(&band)?
        .iter()
        .any(|line| ascii_key(&line.text).contains("escaped")))
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
    let mut popup_check = last_action;
    let mut round = Round::default();
    let mut cast_visible_since = None::<Instant>;
    fn count_silent(round: &Round, count: &mut u32, report: &mut dyn FnMut(ActivityEvent)) {
        if round.silent_catch() {
            trace::note("small fish caught without a card");
            *count += 1;
            report(ActivityEvent::Collected(GameMode::Fishing, 1));
        }
    }
    let deadline = Instant::now() + crate::timing::secs(330);
    while Instant::now() < deadline && casts < 24 {
        let frame = driver.read()?;
        if Instant::now() >= next_check {
            next_check = Instant::now() + Duration::from_secs(3);
            let status = remaining_status(&frame)?;
            trace::note(&format!(
                "check: book={} remaining_zero={status:?}",
                frame.book()?
            ));
            if frame.book()? && status == Some(true) {
                count_silent(&round, &mut count, report);
                driver.tap(frame.size, (102, 1792))?;
                driver.settle()?;
                return Ok(count);
            }
        }
        if let Some(bite) = frame.find(BITES, HOOK, 0.77)? {
            trace::note(&format!("bite visible at {bite:?}, hooked={hooked}"));
            cast_visible_since = None;
            let bite_since = bite_visible_since.get_or_insert_with(Instant::now);
            // Give the hook timing bar a moment to enter its active range.
            if (!hooked || last_action.elapsed() >= Duration::from_secs(1))
                && bite_since.elapsed() >= Duration::from_millis(500)
            {
                driver.tap(frame.size, bite)?;
                hooked = true;
                round.hooked = true;
                pull_frames = 0;
                last_action = Instant::now();
            }
        } else if let Some(control) = match frame.find(PULL, HOOK, 0.70)? {
            // The Tap & Hold button changes size while it animates; the pull bar
            // colours identify the phase reliably, as in ADB AutoPlayer.
            Some(control) => Some(control),
            None => pull_bar(&frame.pixels).map(|_| ROD_BUTTON),
        } {
            bite_visible_since = None;
            cast_visible_since = None;
            hooked = true;
            round.hooked = true;
            pull_frames += 1;
            let duration = pull_duration(&frame.pixels);
            trace::note(&format!(
                "pull control at {control:?}, frames={pull_frames}, hold={duration:?}"
            ));
            let size = frame.size;
            // Android `input` has one pointer: a tap lifts a running hold. As in
            // ADB AutoPlayer, Strong Pull is tapped only while no hold runs, on
            // the same lane, and no hold starts in that frame.
            let skill = pull_frames % 4 == 0
                && driver.input_in_background(
                    &driver.input_busy,
                    format!("strong pull {STRONG_PULL:?}"),
                    move |serial| adb::tap_game_point(serial, STRONG_PULL.0, STRONG_PULL.1, size),
                );
            if let (false, Some(ms)) = (skill, duration) {
                driver.input_in_background(
                    &driver.input_busy,
                    format!("hold {control:?} {ms} ms"),
                    move |serial| adb::hold_game_point(serial, control, size, ms),
                );
            }
        } else if let Some(cast) = frame.cast_button()? {
            trace::note(&format!(
                "cast button at {cast:?}, since_action={:?}",
                last_action.elapsed()
            ));
            // Cast is also visible immediately before the cast animation starts,
            // and a catch card can still follow it, so it must stay a moment.
            let since = cast_visible_since.get_or_insert_with(Instant::now);
            let settled = !round.hooked || since.elapsed() >= Duration::from_millis(1500);
            if last_action.elapsed() >= Duration::from_secs(2) && settled {
                count_silent(&round, &mut count, report);
                round = Round::default();
                if remaining_status(&frame)? == Some(true) {
                    driver.tap(frame.size, (102, 1792))?;
                    driver.settle()?;
                    return Ok(count);
                }
                driver.tap(frame.size, cast)?;
                casts += 1;
                cast_visible_since = None;
                hooked = false;
                bite_visible_since = None;
                last_action = Instant::now();
            }
        } else {
            cast_visible_since = None;
            // Catch cards can appear after the Cast button briefly showed again,
            // so they are checked in every state, but not on every frame.
            let close = if popup_check.elapsed() >= Duration::from_millis(400) {
                popup_check = Instant::now();
                if round.hooked && !round.escaped && fish_escaped(&frame)? {
                    trace::note("the fish escaped");
                    round.escaped = true;
                }
                catch_close(&frame)?
            } else {
                None
            };
            trace::note(&format!(
                "nothing known, hooked={hooked}, catch_close={close:?}"
            ));
            if let Some((close, caught)) = close {
                if round.card(caught) {
                    count += 1;
                    report(ActivityEvent::Collected(GameMode::Fishing, 1));
                }
                driver.tap(frame.size, close)?;
                driver.settle()?;
                hooked = false;
                bite_visible_since = None;
                last_action = Instant::now();
            }
        }
        if last_action.elapsed() > crate::timing::secs(45) {
            trace::note("stop: no action for 45 s");
            break;
        }
        if driver.stream.is_none() {
            thread::sleep(Duration::from_millis(50));
        }
    }
    trace::note(&format!("stop: casts={casts} count={count}"));
    Err(tr!("fishing-fishing-did-not-return-to-a").into())
}

pub fn run(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    report(ActivityEvent::ModeOpening(GameMode::Fishing));
    // A launch intent makes the running game leave the fishing screen, so the
    // minigame never sends one. It starts where the player opened fishing.
    let open_first = || ActionError::from(tr!("fishing-open-the-fishing-screen-first"));
    if adb::game_state(serial).map_err(|e| e.to_string())? != adb::GameState::Running {
        return Err(open_first());
    }
    crate::afk_journey::recovery::expect_running(serial);
    let mut driver = Driver {
        serial,
        cancelled,
        stream: None,
        seen: std::cell::Cell::new(0),
        input_busy: std::sync::Arc::new(AtomicBool::new(false)),
    };
    if !on_fishing_screen(&driver.read()?)? {
        return Err(open_first());
    }
    let size = adb::display_size(serial).map_err(|e| e.to_string())?;
    driver.stream = vision::ScreenStream::start(serial, size, Duration::from_secs(4));
    trace::note(&format!("live stream: {}", driver.stream.is_some()));
    if driver.stream.is_none() {
        log(tr!("fishing-no-live-stream").into());
    }
    log(tr!("fishing-fishing-at-the-open-spot").into());
    report(ActivityEvent::ModeEntered(GameMode::Fishing));
    fish_spot(&driver, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_bite_without_card_or_escape_is_a_silent_small_catch() {
        let mut round = Round::default();
        assert!(!round.silent_catch(), "no bite, no catch");
        round.hooked = true;
        assert!(
            round.silent_catch(),
            "small fish: bite hit, nothing else shown"
        );
        let mut escaped = Round {
            hooked: true,
            ..Round::default()
        };
        escaped.escaped = true;
        assert!(!escaped.silent_catch());
        let mut carded = Round {
            hooked: true,
            ..Round::default()
        };
        assert!(carded.card(true), "the card counts once");
        assert!(!carded.card(true));
        assert!(!carded.silent_catch(), "not counted twice");
        let mut treasure = Round {
            hooked: true,
            ..Round::default()
        };
        assert!(!treasure.card(false));
        assert!(treasure.silent_catch());
    }

    #[test]
    fn remaining_attempts_are_read_with_or_without_the_slash() {
        assert_eq!(
            remaining_attempts("O Remaining attempts: 0/10"),
            Some((0, 10))
        );
        // Windows OCR sometimes drops the slash.
        assert_eq!(
            remaining_attempts("Remaining attempts: 0 10"),
            Some((0, 10))
        );
        assert_eq!(
            remaining_attempts("Remaining attempts: 10/10"),
            Some((10, 10))
        );
        assert_eq!(
            remaining_attempts("Remaining attempts: 5 / 5"),
            Some((5, 5))
        );
        assert_eq!(remaining_attempts("Remaining attempts: 010"), None);
        assert_eq!(remaining_attempts("Gravebloom Fields 2 (212, 276)"), None);
    }

    #[test]
    fn catch_weights_are_recognised_after_the_key_drops_the_dot() {
        assert!(is_weight("201kg"));
        assert!(is_weight("12kg"));
        assert!(!is_weight("kg"));
        assert!(!is_weight("taptoclose"));
        assert!(!is_weight("2x01kg"));
    }

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
        // The minigame starts only on the open fishing screen, never on the map.
        assert!(on_fishing_screen(&load("fishing-after")).unwrap());
        assert!(!on_fishing_screen(&map).unwrap());
        // In the live stream the Tap & Hold button is smaller than the template;
        // the pull bar colours still identify the pull phase.
        assert!(pull_bar(&load("fishing-stream-pull").pixels).is_some());
        assert!(fish_escaped(&load("fishing-escaped")).unwrap());
        assert!(!fish_escaped(&load("fishing-first-catch")).unwrap());
        assert!(!fish_escaped(&load("fishing-stream-pull")).unwrap());
        assert_eq!(
            remaining_status(&load("fishing-stream-no-attempts")).unwrap(),
            Some(true)
        );
        // A first catch shows "First Catch", the weight, and "Tap to close".
        let (close, caught) = catch_close(&load("fishing-first-catch"))
            .unwrap()
            .expect("first catch popup");
        assert!(caught, "first catch counts as a catch");
        assert!(
            close.0.abs_diff(540) < 120 && close.1.abs_diff(1812) < 40,
            "{close:?}"
        );
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
    #[ignore = "prints what fishing sees in one capture (ADBHELPER_FISHING_FRAME)"]
    fn fishing_frame_diagnostic() {
        let path = std::env::var_os("ADBHELPER_FISHING_FRAME").expect("capture path");
        let image = image::open(path).unwrap();
        let size = (image.width(), image.height());
        let image = image.resize_exact(
            BASE_RESOLUTION.0,
            BASE_RESOLUTION.1,
            image::imageops::FilterType::Triangle,
        );
        let frame = Frame {
            gray: image.to_luma8(),
            pixels: image.to_rgba8(),
            size,
        };
        let top = image::imageops::crop_imm(&frame.gray, 0, 0, 425, 200).to_image();
        for line in ocr::recognize(&top).unwrap() {
            println!("map title OCR: {:?} at {:?}", line.text, line.center());
        }
        println!("is_map: {:?}", is_map(&frame));
        println!("next_marker: {:?}", next_marker(&frame, &[]));
        for path in [
            "fishing/map_marker.png",
            "fishing/map_marker_small.png",
            "fishing/map_marker_edge.png",
            "fishing/map_marker_edge_small.png",
        ] {
            let best = vision::find_template(&frame.gray, path, MAP_MARKERS, 0.0).unwrap();
            println!("{path}: {:?}", best.map(|m| (m.center(), m.confidence)));
        }
        println!(
            "book: {:?}, remaining: {:?}",
            frame.book(),
            remaining_status(&frame)
        );
        let crop = image::imageops::crop_imm(&frame.gray, 160, 120, 760, 135).to_image();
        for line in ocr::recognize(&crop).unwrap() {
            println!("remaining OCR: {:?}", line.text);
        }
        let best = |paths: &[&str], region: Rect| {
            paths
                .iter()
                .map(|path| {
                    let found = vision::find_template(&frame.gray, path, region, 0.0).unwrap();
                    format!("{path}={:?}", found.map(|m| (m.center(), m.confidence)))
                })
                .collect::<Vec<_>>()
                .join("  ")
        };
        let book_region = Rect {
            x: 890,
            y: 0,
            width: 190,
            height: 208,
        };
        println!("book: {}", best(&["fishing/book.png"], book_region));
        println!("rods in HOOK: {}", best(RODS, HOOK));
        println!("rods in ARRIVAL: {}", best(RODS, ARRIVAL));
        println!("bites: {}", best(BITES, HOOK));
        println!("pull: {}", best(PULL, HOOK));
        println!(
            "pull bar: {:?}, hold: {:?}",
            pull_bar(&frame.pixels),
            pull_duration(&frame.pixels)
        );
    }

    #[test]
    #[ignore = "records a fishing run in a connected emulator for diagnosis"]
    fn connected_emulator_fishing_trace() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let minutes: u64 = std::env::var("ADBHELPER_TRACE_MINUTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(minutes * 60));
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        let result = run(
            &serial,
            &cancelled,
            &mut |line| {
                trace::note(&format!("log: {line}"));
                println!("{line}");
            },
            &mut |event| {
                trace::note(&format!("event: {event:?}"));
                println!("{event:?}");
            },
        );
        trace::note(&format!("result: {result:?}"));
        println!("result: {result:?}");
        std::thread::sleep(Duration::from_secs(3));
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
