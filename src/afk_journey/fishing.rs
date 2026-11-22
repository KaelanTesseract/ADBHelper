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

mod map;
mod route;

type Point = (u32, u32);
type Rect = vision::Rect;

const HOOK: Rect = Rect {
    x: 390,
    y: 1250,
    width: 300,
    height: 345,
};
/// Where the fishing button shows when the character stands at a spot.
const ARRIVAL: Rect = Rect {
    x: 390,
    y: 1060,
    width: 300,
    height: 340,
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
/// The fishing screen is open: the book with a fishing control, or, while a
/// round runs and the book is hidden, the "Remaining attempts" line.
fn on_fishing_screen(frame: &Frame) -> Result<bool, ActionError> {
    if frame.book()? {
        let controls: Vec<&str> = RODS.iter().chain(BITES).chain(PULL).copied().collect();
        if frame.find(&controls, HOOK, 0.70)?.is_some() {
            return Ok(true);
        }
    }
    Ok(remaining_status(frame)?.is_some())
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

    fn swipe(&self, size: Point, from: Point, to: Point) -> Result<(), ActionError> {
        vision::ensure_not_cancelled(self.cancelled)?;
        trace::note(&format!("swipe {from:?} -> {to:?}"));
        adb::swipe_game_points(self.serial, from, to, size, route::SWIPE_MS)
            .map_err(|e| e.to_string())?;
        self.settle()
    }

    fn settle(&self) -> Result<(), ActionError> {
        vision::settle_cancellable(self.cancelled)
    }
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
    hold_for_distance(center.saturating_sub(hook))
}

/// Hold time for the hook's distance to the target: ADB AutoPlayer's
/// _handle_hold_for_distance steps (600/450/300/200/100 ms), held 50 % longer
/// and then another 25 % (about 1.875 times) so fewer fish escape.
fn hold_for_distance(distance: usize) -> Option<u32> {
    match distance {
        601.. => Some(1125),
        401.. => Some(845),
        201.. => Some(565),
        101.. => Some(375),
        51.. => Some(190),
        _ => None,
    }
}

/// The timing bar shown when a fish bites: an orange centre between two green
/// parts, and a white pointer swinging from end to end above it (one swing
/// about 2.6 s, up to 600 px/s in the middle; measured at Novaris).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BiteBar {
    /// First and last x of the orange centre.
    orange: (u32, u32),
    /// x of the pointer.
    pointer: u32,
}

const BITE_BAR_X: (u32, u32) = (250, 800);
/// Rows through the bar's colours (measured 1028–1045).
const BITE_BAR_ROWS: (u32, u32) = (1026, 1046);
/// Rows of the pointer above the bar (measured 1005–1040, the tip overlaps).
const BITE_POINTER_ROWS: (u32, u32) = (1000, 1024);
/// Time from deciding to tap until the game sees it (input 100–170 ms).
const BITE_TAP_DELAY_S: f32 = 0.15;

fn bite_bar(image: &RgbaImage) -> Option<BiteBar> {
    let green = |[r, g, b, _]: [u8; 4]| g > 150 && r < 170 && b < 130 && g > r.saturating_add(20);
    let orange = |[r, g, b, _]: [u8; 4]| r > 200 && (100..200).contains(&g) && b < 110;
    let (left, right) = BITE_BAR_X;
    // The pointer's tip covers a few rows and the borders between the colours
    // are dark, so every row is read and the widest centre is kept.
    let orange = (BITE_BAR_ROWS.0..BITE_BAR_ROWS.1)
        .step_by(3)
        .filter_map(|y| {
            let pixel = |x: u32| image.get_pixel(x, y).0;
            let mut x = left;
            while x < right {
                if !orange(pixel(x)) {
                    x += 1;
                    continue;
                }
                let start = x;
                let mut end = x;
                // The pointer's tip may cross the centre: short gaps of other
                // colours do not end it, green does.
                while x < right && !green(pixel(x)) && x - end <= 25 {
                    if orange(pixel(x)) {
                        end = x;
                    }
                    x += 1;
                }
                // The centre has green on both sides, past a dark border.
                let green_left = (start.saturating_sub(20)..start).any(|x| green(pixel(x)));
                let green_right = (end + 1..(end + 21).min(right)).any(|x| green(pixel(x)));
                if green_left && green_right && end - start >= 40 {
                    return Some((start, end));
                }
            }
            None
        })
        .max_by_key(|(start, end)| end - start)?;
    let (mut sum, mut count) = (0u64, 0u64);
    for y in BITE_POINTER_ROWS.0..BITE_POINTER_ROWS.1 {
        for x in left..right {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            if r > 220 && g > 220 && b > 220 {
                sum += u64::from(x);
                count += 1;
            }
        }
    }
    (count > 15).then(|| BiteBar {
        orange,
        pointer: (sum / count) as u32,
    })
}

/// True when a tap now lands with the pointer in the middle of the orange
/// centre, given the pointer's speed in px/s (positive = moving right).
fn aim_bite(bar: BiteBar, speed: f32) -> bool {
    let middle = (bar.orange.0 + bar.orange.1) as f32 / 2.0;
    let tolerance = ((bar.orange.1 - bar.orange.0) as f32 / 8.0).max(15.0);
    let landing = bar.pointer as f32 + speed * BITE_TAP_DELAY_S;
    (landing - middle).abs() <= tolerance
}

/// How long to hold when the hook is far from the target (`ms` of 450 or
/// more): nearly until the next frame (`frame_ms` apart), so a far hook does
/// not drop back between holds. Not longer: the next hold is only sent when
/// the previous one has ended. Measured at a Lucky Spot: one 300–450 ms hold
/// moved the hook about 70 px, frames came 0.58 s apart, and it fell back in
/// between.
fn chained_hold(ms: u32, frame_ms: u32) -> u32 {
    if ms < 450 {
        return ms;
    }
    ms.max(frame_ms.saturating_sub(50)).min(1500)
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
    let mut last_pull_frame = None::<Instant>;
    let mut bite_pointer = None::<(u32, Instant)>;
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
            let bite_since = *bite_visible_since.get_or_insert_with(Instant::now);
            // Tap when the timing bar's pointer will be in the middle of the
            // orange centre, where the bigger fish are.
            let bar = bite_bar(&frame.pixels);
            let speed = match (bar, bite_pointer) {
                (Some(bar), Some((x, at))) if at.elapsed() < Duration::from_millis(300) => {
                    (bar.pointer as f32 - x as f32) / at.elapsed().as_secs_f32().max(0.001)
                }
                _ => 0.0,
            };
            bite_pointer = bar.map(|bar| (bar.pointer, Instant::now()));
            let timed = match bar {
                Some(bar) => {
                    let aimed = aim_bite(bar, speed);
                    trace::note(&format!(
                        "bite bar {bar:?}, speed {speed:.0}, aimed {aimed}"
                    ));
                    // Never let the fish go: late, any orange will do.
                    aimed
                        || (bite_since.elapsed() >= Duration::from_secs(3)
                            && (bar.orange.0..=bar.orange.1).contains(&bar.pointer))
                }
                // Without a readable bar, as before.
                None => bite_since.elapsed() >= Duration::from_millis(1500),
            };
            if (!hooked || last_action.elapsed() >= Duration::from_secs(1)) && timed {
                driver.tap(frame.size, bite)?;
                hooked = true;
                round.hooked = true;
                pull_frames = 0;
                last_pull_frame = None;
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
            let frame_ms = last_pull_frame
                .replace(Instant::now())
                .map_or(0, |at| at.elapsed().as_millis() as u32);
            let duration = pull_duration(&frame.pixels).map(|ms| chained_hold(ms, frame_ms));
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
    // Still at the spot between two rounds (escaped fish may not cost an
    // attempt): give this spot up and leave, so the run goes on elsewhere.
    let frame = driver.read()?;
    if frame.book()? && on_fishing_screen(&frame)? {
        trace::note("spot given up");
        count_silent(&round, &mut count, report);
        driver.tap(frame.size, (102, 1792))?;
        driver.settle()?;
        return Ok(count);
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
    // A launch intent makes the running game leave the fishing screen, so
    // fishing never sends one; the game has to be running already.
    if adb::game_state(serial).map_err(|e| e.to_string())? != adb::GameState::Running {
        return Err(tr!("afk-stages-could-not-reach-the-main-screen").into());
    }
    crate::afk_journey::recovery::expect_running(serial);
    let mut driver = Driver {
        serial,
        cancelled,
        stream: None,
        seen: std::cell::Cell::new(0),
        input_busy: std::sync::Arc::new(AtomicBool::new(false)),
    };
    report(ActivityEvent::ModeEntered(GameMode::Fishing));
    let mut count = 0;
    // First the spot the player already stands at.
    let frame = driver.read()?;
    if on_fishing_screen(&frame)? {
        log(tr!("fishing-fishing-at-the-open-spot").into());
        count += fish_streamed(&mut driver, log, report)?;
    } else if let Some(rod) = frame.find(RODS, ARRIVAL, 0.76)? {
        driver.tap(frame.size, rod)?;
        for _ in 0..6 {
            driver.settle()?;
            if on_fishing_screen(&driver.read()?)? {
                log(tr!("fishing-fishing-at-the-open-spot").into());
                count += fish_streamed(&mut driver, log, report)?;
                break;
            }
        }
    }
    let frame = map::open_map(&driver, log)?;
    let tabs = map::read_tabs(&frame)?;
    trace::note(&format!("tabs {tabs:?}"));
    let mut visits = route::Visits::new(route::MAX_VISITS_PER_MAP);
    for tab in &tabs {
        count += fish_map(&mut driver, &tab.name, &mut visits, log, report)?;
    }
    if let Ok(frame) = driver.read() {
        if map::is_map(&frame)? {
            map::close_map(&driver, frame.size)?;
        }
    }
    log(tr!("fishing-no-spots-with-attempts-left").into());
    Ok(count)
}

/// Plays the minigame at the open fishing screen with the live stream, which
/// only runs here; the map steps use plain screenshots.
fn fish_streamed(
    driver: &mut Driver<'_>,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    let size = adb::display_size(driver.serial).map_err(|e| e.to_string())?;
    driver.stream = vision::ScreenStream::start(driver.serial, size, Duration::from_secs(4));
    trace::note(&format!("live stream: {}", driver.stream.is_some()));
    if driver.stream.is_none() {
        log(tr!("fishing-no-live-stream").into());
    }
    let caught = fish_spot(driver, report);
    driver.stream = None;
    caught
}

/// Searches one map tab, then walks to each spot found there and fishes until
/// it has no attempts left. Spots are map points with the location arrow at
/// (0, 0) when the search began; after walking, the arrow is the character at
/// the spot it walked to.
fn fish_map(
    driver: &mut Driver<'_>,
    name: &str,
    visits: &mut route::Visits,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    log(tr!("fishing-searching-map", map = name.to_owned()));
    let Some((view, arrow)) = map::anchor(driver, name)? else {
        trace::note(&format!("{name}: no arrow, skipped"));
        return Ok(0);
    };
    let spots = map::explore(driver, view, map::origin((0, 0), arrow))?;
    trace::note(&format!("{name}: spots {spots:?}"));
    let mut arrow_at = (0, 0);
    let mut count = 0;
    for spot in spots {
        // A marker tap zooms in, so every spot starts from a fresh map view.
        reopen_map(driver, log)?;
        let Some((view, arrow)) = map::anchor(driver, name)? else {
            break;
        };
        let Some((marker, frame)) = map::go_to(driver, view, map::origin(arrow_at, arrow), spot)?
        else {
            continue;
        };
        driver.tap(frame.size, marker)?;
        driver.settle()?;
        let panel = driver.read()?;
        let map::PanelDecision::Walk(walk) = map::read_panel(&panel, marker)? else {
            continue;
        };
        if !visits.visit(name) {
            trace::note(&format!("{name}: visit limit"));
            break;
        }
        match map::walk_to(driver, panel.size, walk, log)? {
            map::Arrival::Arrived => {
                arrow_at = spot;
                count += fish_streamed(driver, log, report)?;
            }
            map::Arrival::Blocked => {
                // Where the character stopped is unknown, so the map's
                // remaining spots cannot be found again from the arrow.
                log(tr!("fishing-spot-skipped").into());
                break;
            }
        }
    }
    reopen_map(driver, log)?;
    Ok(count)
}

/// Closes the map if it is open (the back arrow is only tapped on the map),
/// then opens it again at the character.
fn reopen_map(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<(), ActionError> {
    let frame = driver.read()?;
    if map::is_map(&frame)? {
        map::close_map(driver, frame.size)?;
    }
    map::open_map(driver, log)?;
    Ok(())
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
    fn pull_holds_are_longer_than_adb_autoplayers() {
        assert_eq!(hold_for_distance(700), Some(1125));
        assert_eq!(hold_for_distance(450), Some(845));
        assert_eq!(hold_for_distance(250), Some(565));
        assert_eq!(hold_for_distance(150), Some(375));
        assert_eq!(hold_for_distance(60), Some(190));
        assert_eq!(hold_for_distance(50), None);
    }

    /// A bite bar as measured at a Novaris spot: green, orange centre, green,
    /// with the white pointer above it at `pointer`.
    fn bite_bar_image(pointer: u32) -> RgbaImage {
        let mut image = RgbaImage::from_pixel(1080, 1920, image::Rgba([40, 60, 90, 255]));
        for y in 1028..1045 {
            for x in 295..774 {
                let colour = if (435..627).contains(&x) {
                    [235, 150, 50, 255]
                } else {
                    [110, 190, 90, 255]
                };
                image.put_pixel(x, y, image::Rgba(colour));
            }
        }
        for y in 1005..1022 {
            for x in pointer - 8..pointer + 8 {
                image.put_pixel(x, y, image::Rgba([250, 250, 250, 255]));
            }
        }
        image
    }

    #[test]
    fn the_bite_bar_gives_the_orange_centre_and_the_pointer() {
        let bar = bite_bar(&bite_bar_image(624)).expect("bar");
        assert!(
            bar.orange.0.abs_diff(435) <= 2 && bar.orange.1.abs_diff(626) <= 2,
            "{bar:?}"
        );
        assert!(bar.pointer.abs_diff(624) <= 2, "{bar:?}");
        let plain = RgbaImage::from_pixel(1080, 1920, image::Rgba([40, 60, 90, 255]));
        assert_eq!(bite_bar(&plain), None);
    }

    #[test]
    fn the_bite_is_tapped_so_the_pointer_lands_in_the_middle() {
        let bar = |pointer| BiteBar {
            orange: (435, 626),
            pointer,
        };
        // Moving right at 600 px/s, 150 ms tap delay: it lands 90 px further.
        assert!(aim_bite(bar(440), 600.0));
        assert!(!aim_bite(bar(300), 600.0), "would land left of the middle");
        assert!(!aim_bite(bar(530), 600.0), "would land right of the middle");
        // Moving left.
        assert!(aim_bite(bar(620), -600.0));
        // Slow near the turn: no lead needed.
        assert!(aim_bite(bar(535), 0.0));
        assert!(!aim_bite(bar(700), 0.0));
    }

    #[test]
    fn far_holds_last_until_the_next_frame_is_read() {
        // Slow frames: a far hook is held without a gap, at most 1.5 s.
        assert_eq!(chained_hold(450, 700), 650);
        assert_eq!(chained_hold(675, 580), 675);
        assert_eq!(chained_hold(900, 2000), 1500);
        // Fast frames keep ADB AutoPlayer's times.
        assert_eq!(chained_hold(450, 40), 450);
        // Close to the target the short taps stay short.
        assert_eq!(chained_hold(300, 580), 300);
        assert_eq!(chained_hold(150, 580), 150);
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
        assert!(map::zero_attempts("Remaining attempts: 0 / 10"));
        assert!(!map::zero_attempts("Remaining attempts: 10/10"));
        assert!(!map::zero_attempts("Remaining attempts: 1/10"));
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
        assert!(map::is_map(&map).unwrap());
        assert!(matches!(
            map::read_panel(&load("fishing-marker"), (627, 360)).unwrap(),
            map::PanelDecision::Walk(_)
        ));
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
            matches!(pull_duration(&pulling.pixels), Some(845)),
            "pull duration"
        );
        assert_eq!(
            remaining_status(&load("fishing-before-optimized")).unwrap(),
            Some(true)
        );
        assert!(load("fishing-before-optimized").book().unwrap());
        assert!(matches!(
            map::read_panel(&load("fishing-second-panel"), (177, 1125)).unwrap(),
            map::PanelDecision::Walk(_)
        ));
    }

    #[test]
    #[ignore = "prints the OCR lines of one image (ADBHELPER_OCR_IMAGE)"]
    fn ocr_image_diagnostic() {
        let path = std::env::var_os("ADBHELPER_OCR_IMAGE").expect("image path");
        let image = image::open(path).unwrap().to_luma8();
        for line in ocr::recognize(&image).unwrap() {
            println!("OCR {:?} at {:?}", line.text, line.center());
        }
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
        println!("is_map: {:?}", map::is_map(&frame));
        println!("empty view: {}", route::is_empty_view(&frame.gray));
        println!("open sides: {:?}", route::open_sides(&frame.gray));
        println!("arrow: {:?}", map::find_arrow(&frame));
        println!("zoom knob: {:?}", map::slider_knob(&frame.gray));
        println!("markers: {:?}", map::markers(&frame));
        println!("tabs: {:?}", map::read_tabs(&frame));
        if std::env::var_os("ADBHELPER_FISHING_OCR").is_some() {
            for line in ocr::recognize(&frame.gray).unwrap() {
                println!("OCR {:?} at {:?}", line.text, line.center());
            }
        }
        // ADBHELPER_FISHING_MARKER=x,y reads the spot panel of that marker.
        if let Some(marker) = std::env::var("ADBHELPER_FISHING_MARKER")
            .ok()
            .and_then(|v| {
                v.split_once(',')
                    .map(|(x, y)| (x.parse().ok(), y.parse().ok()))
            })
            .and_then(|p| Some((p.0?, p.1?)))
        {
            println!("panel: {:?}", map::read_panel(&frame, marker));
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
        println!("bite bar: {:?}", bite_bar(&frame.pixels));
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
    #[ignore = "searches every map tab for fishing spots and reads their panels, without walking"]
    fn connected_emulator_map_survey() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let cancelled = AtomicBool::new(false);
        let driver = Driver {
            serial: &serial,
            cancelled: &cancelled,
            stream: None,
            seen: std::cell::Cell::new(0),
            input_busy: std::sync::Arc::new(AtomicBool::new(false)),
        };
        let mut log = |line: String| {
            trace::note(&format!("log: {line}"));
            println!("{line}");
        };
        let frame = map::open_map(&driver, &mut log).unwrap();
        let tabs = map::read_tabs(&frame).unwrap();
        println!("tabs: {tabs:?}");
        for tab in &tabs {
            let started = Instant::now();
            let Some((view, arrow)) = map::anchor(&driver, &tab.name).unwrap() else {
                println!("{}: no arrow", tab.name);
                continue;
            };
            let spots = map::explore(&driver, view, map::origin((0, 0), arrow)).unwrap();
            println!(
                "{}: {} spots {spots:?} in {:.0} s",
                tab.name,
                spots.len(),
                started.elapsed().as_secs_f32()
            );
            for spot in spots {
                // Back to a known view: leave the map, open it, pick the tab.
                let size = driver.read().unwrap().size;
                map::close_map(&driver, size).unwrap();
                map::open_map(&driver, &mut log).unwrap();
                let (view, arrow) = map::anchor(&driver, &tab.name)
                    .unwrap()
                    .expect("arrow again");
                match map::go_to(&driver, view, map::origin((0, 0), arrow), spot).unwrap() {
                    Some((marker, frame)) => {
                        driver.tap(frame.size, marker).unwrap();
                        driver.settle().unwrap();
                        let panel = driver.read().unwrap();
                        let decision = map::read_panel(&panel, marker).unwrap();
                        println!(
                            "{}: spot {spot:?} marker {marker:?} -> {decision:?}",
                            tab.name
                        );
                    }
                    None => println!("{}: spot {spot:?} not found again", tab.name),
                }
            }
            let size = driver.read().unwrap().size;
            map::close_map(&driver, size).unwrap();
            map::open_map(&driver, &mut log).unwrap();
        }
        let size = driver.read().unwrap().size;
        map::close_map(&driver, size).unwrap();
        std::thread::sleep(Duration::from_secs(2));
    }

    #[test]
    #[ignore = "casts once at the open fishing screen and records the bite bar without tapping"]
    fn connected_emulator_bite_bar_recording() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let cancelled = AtomicBool::new(false);
        let mut driver = Driver {
            serial: &serial,
            cancelled: &cancelled,
            stream: None,
            seen: std::cell::Cell::new(0),
            input_busy: std::sync::Arc::new(AtomicBool::new(false)),
        };
        let frame = driver.read().unwrap();
        assert!(on_fishing_screen(&frame).unwrap(), "open fishing first");
        let cast = frame.cast_button().unwrap().expect("cast button");
        let size = adb::display_size(&serial).unwrap();
        driver.stream = vision::ScreenStream::start(&serial, size, Duration::from_secs(4));
        assert!(driver.stream.is_some(), "live stream");
        driver.tap(frame.size, cast).unwrap();
        let started = Instant::now();
        let mut bite_at = None::<Instant>;
        // Every frame is saved by the trace; nothing is tapped after the cast.
        while started.elapsed() < Duration::from_secs(40) {
            let frame = driver.read().unwrap();
            if let Some(bite) = frame.find(BITES, HOOK, 0.77).unwrap() {
                trace::note(&format!("bite visible at {bite:?}"));
                bite_at.get_or_insert_with(Instant::now);
            }
            if bite_at.is_some_and(|at| at.elapsed() > Duration::from_secs(6)) {
                break;
            }
        }
        println!("bite seen: {}", bite_at.is_some());
    }

    #[test]
    #[ignore = "searches every map tab, walks to each spot and fishes it empty"]
    fn connected_emulator_fishing_run() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let cancelled = AtomicBool::new(false);
        let result = run(
            &serial,
            &cancelled,
            &mut |line| {
                trace::note(&format!("log: {line}"));
                println!("{line}");
            },
            &mut |event| println!("{event:?}"),
        );
        println!("result: {result:?}");
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
