//! The world map for fishing: open it, switch map tabs, zoom out, look at every
//! part of a map for fishing spots, read a spot's panel and walk there.
//!
//! Tapping a marker zooms the map in and moves it, so a map is first searched
//! as a whole (`explore`) and every spot is later approached again (`go_to`).
//! Positions are map points relative to the location arrow every map shows
//! (the character, or where the map is entered): the view's top left is found
//! from where the arrow is on screen (`anchor`), whatever the zoom did.

use super::route::{self, Cell, Explorer, Heading, Tab, MAP_VIEW, TAB_STRIP};
use super::{common, ocr, on_fishing_screen, trace, Driver, Frame, Point, Rect, ARRIVAL, RODS};
use crate::afk_journey::{error::ActionError, text_match::ascii_key, vision};
use crate::localization::tr;
use image::GrayImage;
use std::time::Instant;

/// The "Switch" button at the bottom right exists only on the world map.
const SWITCH: Rect = Rect {
    x: 900,
    y: 1740,
    width: 180,
    height: 180,
};
const MINIMAP: Point = (903, 192);
/// Back arrow at the bottom left of the map.
const BACK: Point = (90, 1790);
/// Band the zoom slider's knob moves in; the knob is about 17 rows of bright
/// pixels across the narrow band, the track is thinner.
const SLIDER: Rect = Rect {
    x: 997,
    y: 520,
    width: 31,
    height: 740,
};
const SLIDER_X: u32 = 1012;
/// Knob centre at or below this: the map is zoomed out as far as it goes
/// (measured 1161).
const ZOOMED_OUT: u32 = 1150;
const ZOOM_DRAG_END: Point = (SLIDER_X, 1240);
const MARKERS: &[&str] = &["fishing/map_marker_far.png"];
const MARKER_CONFIDENCE: f32 = 0.85;
const LOCK: &[&str] = &["fishing/map_tab_lock.png"];
/// The location arrow above the coordinates ("282 ; 156"); 0.85 and more when
/// shown, at most 0.60 elsewhere.
const ARROW: &[&str] = &["fishing/map_arrow.png"];
/// The line of tab names at the bottom of the tab strip (names at y ≈ 1659).
const TAB_LABELS: Rect = Rect {
    x: 0,
    y: 1632,
    width: 1080,
    height: 56,
};
/// Map pixels one grid step moves the view (finger travel 600 px, measured
/// 540–560).
pub(super) const MAP_STEP: i32 = 555;
/// Two markers closer than this are the same spot seen from two views.
const SAME_SPOT: i32 = 80;
/// How far a spot may be from where it was expected after moving back to it.
const FIND_AGAIN: i32 = 220;
const WALK_LIMIT_SECS: u64 = 90;

pub(super) type MapPoint = (i32, i32);

pub(super) fn is_map(frame: &Frame) -> Result<bool, ActionError> {
    Ok(frame
        .find(&["fishing/map_switch.png"], SWITCH, 0.85)?
        .is_some())
}

/// Opens the world map from the game, which shows the character's map centred
/// on the character.
pub(super) fn open_map(
    driver: &Driver<'_>,
    log: &mut impl FnMut(String),
) -> Result<Frame, ActionError> {
    let frame = driver.read()?;
    if is_map(&frame)? {
        return Ok(frame);
    }
    leave_area(driver, &frame)?;
    let size = common::navigate_to_current_overview(driver.serial, driver.cancelled, log)?;
    driver.tap(size, MINIMAP)?;
    for _ in 0..8 {
        driver.settle()?;
        let frame = driver.read()?;
        if is_map(&frame)? {
            trace::note("map open");
            return Ok(frame);
        }
    }
    Err(tr!("fishing-could-not-verify-the-world-map").into())
}

/// Leaves the map (and an open spot panel) back to the game.
pub(super) fn close_map(driver: &Driver<'_>, size: Point) -> Result<(), ActionError> {
    for _ in 0..3 {
        driver.tap(size, BACK)?;
        driver.settle()?;
        let frame = driver.read()?;
        if !is_map(&frame)? {
            trace::note("map closed");
            return_to_world(driver, &frame)?;
            return Ok(());
        }
    }
    Err(tr!("fishing-could-not-verify-the-world-map").into())
}

/// Lower right part of the screen where "Return to World" shows.
const RETURN_AREA: Rect = Rect {
    x: 540,
    y: 1300,
    width: 540,
    height: 620,
};

/// The "Return to World" button among OCR lines of `RETURN_AREA` read at
/// twice the size, in screen points.
fn return_button(lines: &[ocr::TextLine]) -> Option<Point> {
    lines
        .iter()
        .find(|line| {
            let key = ascii_key(&line.text);
            key.contains("returnto") || key.contains("toworld")
        })
        .map(|line| {
            let (x, y) = line.center();
            (x / 2 + RETURN_AREA.x, y / 2 + RETURN_AREA.y)
        })
}

/// Asks the game whether to quit; "No" is at `EXIT_NO`.
fn exit_prompt(lines: &[ocr::TextLine]) -> bool {
    lines
        .iter()
        .any(|line| ascii_key(&line.text).contains("youwanttoexit"))
}

const EXIT_NO: Point = (635, 1270);
/// The tick of "Leave the current map and return to the entrance?".
const LEAVE_YES: Point = (865, 1270);

/// "Return to World" asks whether to leave the map; the tick is `LEAVE_YES`.
fn leave_question(lines: &[ocr::TextLine]) -> bool {
    lines.iter().any(|line| {
        let key = ascii_key(&line.text);
        key.contains("leavethecurrentmap") || key.contains("returntotheentrance")
    })
}

/// Leaves a map area such as Refinery that the way to the main screen does not
/// know: answers a quit question with "No", then taps "Return to World".
fn leave_area(driver: &Driver<'_>, frame: &Frame) -> Result<(), ActionError> {
    let lines = ocr::recognize(&frame.gray)?;
    if leave_question(&lines) {
        trace::note("leave the map: yes");
        driver.tap(frame.size, LEAVE_YES)?;
        for _ in 0..4 {
            driver.settle()?;
        }
        return Ok(());
    }
    if exit_prompt(&lines) {
        trace::note("exit prompt, staying");
        driver.tap(frame.size, EXIT_NO)?;
        driver.settle()?;
        return return_to_world(driver, &driver.read()?);
    }
    return_to_world(driver, frame)
}

/// The world's own buttons (Homestead, Guild) show only when the view is
/// already on the character, so there is no "Return to World" to wait for.
fn in_world(lines: &[ocr::TextLine]) -> bool {
    lines.iter().any(|line| {
        let key = ascii_key(&line.text);
        key.contains("homestead") || key.contains("guild")
    })
}

/// After looking at a map the character is not on, leaving the map shows that
/// map's area; "Return to World" brings the view back to the character. The
/// button fades in, so a few frames are read.
fn return_to_world(driver: &Driver<'_>, frame: &Frame) -> Result<(), ActionError> {
    let mut frame = Frame {
        pixels: frame.pixels.clone(),
        gray: frame.gray.clone(),
        size: frame.size,
    };
    for attempt in 0..3 {
        let area = image::imageops::crop_imm(
            &frame.gray,
            RETURN_AREA.x,
            RETURN_AREA.y,
            RETURN_AREA.width,
            RETURN_AREA.height,
        )
        .to_image();
        let area = image::imageops::resize(
            &area,
            RETURN_AREA.width * 2,
            RETURN_AREA.height * 2,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = ocr::recognize(&area)?;
        if let Some(button) = return_button(&lines) {
            trace::note(&format!("return to world {button:?}"));
            driver.tap(frame.size, button)?;
            driver.settle()?;
            let asked = driver.read()?;
            if leave_question(&ocr::recognize(&asked.gray)?) {
                trace::note("leave the map: yes");
                driver.tap(asked.size, LEAVE_YES)?;
                for _ in 0..4 {
                    driver.settle()?;
                }
            }
            return Ok(());
        }
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        trace::note(&format!("no return to world ({attempt}): {texts:?}"));
        if in_world(&lines) {
            return Ok(());
        }
        if attempt < 2 {
            driver.settle()?;
            frame = driver.read()?;
        }
    }
    Ok(())
}

/// Centre row of the zoom slider's knob.
pub(super) fn slider_knob(gray: &GrayImage) -> Option<u32> {
    let bright = |y: u32| {
        (SLIDER.x..SLIDER.x + SLIDER.width)
            .filter(|&x| gray.get_pixel(x, y).0[0] > 200)
            .count()
    };
    let mut run_start = None;
    for y in SLIDER.y..=SLIDER.y + SLIDER.height {
        let wide = y < SLIDER.y + SLIDER.height && bright(y) >= 22;
        match (wide, run_start) {
            (true, None) => run_start = Some(y),
            (false, Some(start)) => {
                if (12..=24).contains(&(y - start)) {
                    return Some((start + y - 1) / 2);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    None
}

/// Zooms the map out as far as it goes by dragging the slider's knob down.
pub(super) fn zoom_out(driver: &Driver<'_>, mut frame: Frame) -> Result<Frame, ActionError> {
    for _ in 0..3 {
        match slider_knob(&frame.gray) {
            Some(knob) if knob >= ZOOMED_OUT => return Ok(frame),
            Some(knob) => {
                trace::note(&format!("zoom out from knob {knob}"));
                driver.swipe(frame.size, (SLIDER_X, knob), ZOOM_DRAG_END)?;
            }
            None => {
                trace::note("zoom knob not found");
                return Ok(frame);
            }
        }
        frame = driver.read()?;
    }
    Ok(frame)
}

/// Every match of `paths` in `region`, best first, each found area masked
/// before looking for the next.
fn find_all(
    frame: &Frame,
    paths: &[&str],
    region: Rect,
    confidence: f32,
    limit: usize,
) -> Result<Vec<Point>, ActionError> {
    let mut gray = frame.gray.clone();
    let mut found = Vec::new();
    while found.len() < limit {
        let Some((_, hit)) = vision::get_match(&gray, paths, region, confidence)? else {
            break;
        };
        let r = hit.rect();
        for y in r.y..(r.y + r.height).min(gray.height()) {
            for x in r.x..(r.x + r.width).min(gray.width()) {
                gray.put_pixel(x, y, image::Luma([0]));
            }
        }
        found.push(hit.center());
    }
    Ok(found)
}

/// Fishing spot markers in the zoomed-out view; markers under the side
/// buttons are left to a neighbouring view.
pub(super) fn markers(frame: &Frame) -> Result<Vec<Point>, ActionError> {
    Ok(find_all(frame, MARKERS, MAP_VIEW, MARKER_CONFIDENCE, 12)?
        .into_iter()
        .filter(|&(x, y)| !route::on_button(x, y))
        .collect())
}

/// Unlocked map tabs in the strip below the map, left to right.
pub(super) fn read_tabs(frame: &Frame) -> Result<Vec<Tab>, ActionError> {
    // Only the line with the names; the tab pictures read as noise.
    let strip = image::imageops::crop_imm(
        &frame.gray,
        TAB_LABELS.x,
        TAB_LABELS.y,
        TAB_LABELS.width,
        TAB_LABELS.height,
    )
    .to_image();
    // Enlarged, so short names such as "Refinery" are not dropped.
    let strip = image::imageops::resize(
        &strip,
        TAB_LABELS.width * 2,
        TAB_LABELS.height * 2,
        image::imageops::FilterType::CatmullRom,
    );
    let lines: Vec<ocr::TextLine> = ocr::recognize(&strip)?
        .into_iter()
        .map(|mut line| {
            line.x = line.x / 2.0 + TAB_LABELS.x as f32;
            line.y = line.y / 2.0 + TAB_LABELS.y as f32;
            line.width /= 2.0;
            line.height /= 2.0;
            line
        })
        .collect();
    let locks = find_all(frame, LOCK, TAB_STRIP, 0.85, 6)?;
    let tabs = route::map_tabs(&lines, &locks);
    trace::note(&format!("map tabs {tabs:?}, locks {locks:?}"));
    Ok(tabs)
}

/// Shows the map named `name` (as `read_tabs` names it) zoomed out. `None` when
/// no such tab is visible.
pub(super) fn open_tab(driver: &Driver<'_>, name: &str) -> Result<Option<Frame>, ActionError> {
    let frame = driver.read()?;
    let Some(tab) = read_tabs(&frame)?
        .into_iter()
        .find(|tab| route::same_tab(&tab.name, name))
    else {
        trace::note(&format!("tab {name} not visible"));
        return Ok(None);
    };
    driver.tap(frame.size, tab.at)?;
    for _ in 0..6 {
        driver.settle()?;
        let frame = driver.read()?;
        if is_map(&frame)? {
            trace::note(&format!("tab {name} open"));
            return Ok(Some(zoom_out(driver, frame)?));
        }
    }
    Err(tr!("fishing-could-not-verify-the-world-map").into())
}

/// The location arrow on screen. The arrow turns with the character, so the
/// template only fits while it points right; otherwise the coordinate label
/// below it ("182 ; 299") is read and the arrow is `LABEL_BELOW_ARROW` above.
pub(super) fn find_arrow(frame: &Frame) -> Result<Option<Point>, ActionError> {
    if let Some(arrow) = frame.find(ARROW, MAP_VIEW, 0.80)? {
        return Ok(Some(arrow));
    }
    let view = image::imageops::crop_imm(
        &frame.gray,
        MAP_VIEW.x,
        MAP_VIEW.y,
        MAP_VIEW.width,
        MAP_VIEW.height,
    )
    .to_image();
    let arrow =
        arrow_from_label(&ocr::recognize(&view)?).map(|(x, y)| (x + MAP_VIEW.x, y + MAP_VIEW.y));
    trace::note(&format!("arrow from label {arrow:?}"));
    Ok(arrow)
}

/// Pixels from the arrow's centre down to its coordinate label's centre
/// (measured 958 and 1003).
const LABEL_BELOW_ARROW: u32 = 45;

/// The arrow above a coordinate label such as "182 ; 299" or "182; 299".
fn arrow_from_label(lines: &[ocr::TextLine]) -> Option<Point> {
    lines
        .iter()
        .find(|line| {
            let text: String = line.text.chars().filter(|c| !c.is_whitespace()).collect();
            let Some((x, y)) = text.split_once(';') else {
                return false;
            };
            let number = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
            number(x) && number(y)
        })
        .map(|line| {
            let (x, y) = line.center();
            (x, y.saturating_sub(LABEL_BELOW_ARROW))
        })
}

/// Shows the map tab `name` zoomed out and returns the view with the arrow's
/// screen position. Tapping the tab centres the map on the arrow, but zooming
/// out afterwards can move it, so the arrow is looked for, and the tab tapped
/// once more when it is gone.
pub(super) fn anchor(
    driver: &Driver<'_>,
    name: &str,
) -> Result<Option<(Frame, Point)>, ActionError> {
    let Some(frame) = open_tab(driver, name)? else {
        return Ok(None);
    };
    if let Some(arrow) = find_arrow(&frame)? {
        trace::note(&format!("arrow {arrow:?}"));
        return Ok(Some((frame, arrow)));
    }
    let Some(frame) = open_tab(driver, name)? else {
        return Ok(None);
    };
    let arrow = find_arrow(&frame)?;
    trace::note(&format!("arrow after second tab tap {arrow:?}"));
    Ok(arrow.map(|arrow| (frame, arrow)))
}

/// Top left of the view in map points, when the arrow at map point `arrow_at`
/// shows at `arrow_on_screen`.
pub(super) fn origin(arrow_at: MapPoint, arrow_on_screen: Point) -> MapPoint {
    (
        arrow_at.0 - arrow_on_screen.0 as i32,
        arrow_at.1 - arrow_on_screen.1 as i32,
    )
}

/// One grid step; the frame afterwards must still be the map.
fn step(driver: &Driver<'_>, size: Point, heading: Heading) -> Result<Frame, ActionError> {
    let (from, to) = route::swipe_for(heading);
    driver.swipe(size, from, to)?;
    let frame = driver.read()?;
    if !is_map(&frame)? {
        return Err(tr!("fishing-could-not-verify-the-world-map").into());
    }
    Ok(frame)
}

pub(super) fn to_map(cell: Cell, screen: Point) -> MapPoint {
    (
        cell.0 * MAP_STEP + screen.0 as i32,
        cell.1 * MAP_STEP + screen.1 as i32,
    )
}

fn distance(a: MapPoint, b: MapPoint) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// Looks at every part of the map shown in `frame`, whose top left is at map
/// point `origin`, and returns the fishing spots in map points.
pub(super) fn explore(
    driver: &Driver<'_>,
    mut frame: Frame,
    origin: MapPoint,
) -> Result<Vec<MapPoint>, ActionError> {
    let mut explorer = Explorer::new(route::MAX_VIEWS_PER_MAP);
    let mut spots: Vec<MapPoint> = Vec::new();
    loop {
        let empty = route::is_empty_view(&frame.gray);
        let open = route::open_sides(&frame.gray);
        if explorer.record(empty, open) {
            for marker in markers(&frame)? {
                let (x, y) = to_map(explorer.at(), marker);
                let spot = (origin.0 + x, origin.1 + y);
                if spots.iter().all(|&s| distance(s, spot) > SAME_SPOT) {
                    trace::note(&format!("spot {spot:?} at {:?}", explorer.at()));
                    spots.push(spot);
                }
            }
        }
        trace::note(&format!(
            "view {:?} empty {empty} open {open:?}",
            explorer.at()
        ));
        let Some(heading) = explorer.next_move() else {
            return Ok(spots);
        };
        let next = step(driver, frame.size, heading)?;
        if !route::moved(&frame.gray, &next.gray) {
            explorer.stuck();
            trace::note(&format!("{heading:?} clamped, back at {:?}", explorer.at()));
        }
        frame = next;
    }
}

/// Where a spot is on screen after a step towards `heading` did not move the
/// map, given where it was expected had the step moved it.
fn unmoved(expected: MapPoint, heading: Heading) -> MapPoint {
    match heading {
        Heading::Right => (expected.0 + MAP_STEP, expected.1),
        Heading::Left => (expected.0 - MAP_STEP, expected.1),
        Heading::Down => (expected.0, expected.1 + MAP_STEP),
        Heading::Up => (expected.0, expected.1 - MAP_STEP),
    }
}

/// The grid steps that bring `target` near the middle of the map view when the
/// view's top left is at map point `origin`, and where on screen it then is.
pub(super) fn steps_to(target: MapPoint, origin: MapPoint) -> (Vec<Heading>, MapPoint) {
    let centre = (
        (MAP_VIEW.x + MAP_VIEW.width / 2) as i32,
        (MAP_VIEW.y + MAP_VIEW.height / 2) as i32,
    );
    let screen = (target.0 - origin.0, target.1 - origin.1);
    let grid = |offset: i32| ((offset as f32) / MAP_STEP as f32).round() as i32;
    let (dx, dy) = (grid(screen.0 - centre.0), grid(screen.1 - centre.1));
    let mut headings = Vec::new();
    let horizontal = if dx > 0 {
        Heading::Right
    } else {
        Heading::Left
    };
    let vertical = if dy > 0 { Heading::Down } else { Heading::Up };
    headings.extend(std::iter::repeat_n(horizontal, dx.unsigned_abs() as usize));
    headings.extend(std::iter::repeat_n(vertical, dy.unsigned_abs() as usize));
    (
        headings,
        (screen.0 - dx * MAP_STEP, screen.1 - dy * MAP_STEP),
    )
}

/// Moves from the view `frame` (top left at `origin`) to the spot `target` and
/// returns the marker's screen position, or `None` when it is not there.
pub(super) fn go_to(
    driver: &Driver<'_>,
    mut frame: Frame,
    origin: MapPoint,
    target: MapPoint,
) -> Result<Option<(Point, Frame)>, ActionError> {
    let (headings, mut expected) = steps_to(target, origin);
    for heading in headings {
        let next = step(driver, frame.size, heading)?;
        if !route::moved(&frame.gray, &next.gray) {
            expected = unmoved(expected, heading);
        }
        frame = next;
    }
    let found = markers(&frame)?
        .into_iter()
        .map(|m| (m, distance((m.0 as i32, m.1 as i32), expected)))
        .filter(|&(_, d)| d <= FIND_AGAIN)
        .min_by_key(|&(_, d)| d);
    trace::note(&format!(
        "spot {target:?} expected at {expected:?}, found {found:?}"
    ));
    Ok(found.map(|(m, _)| (m, frame)))
}

/// What a spot panel says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PanelDecision {
    /// Attempts are left; tap the walking button here.
    Walk(Point),
    /// No attempts left.
    Empty,
    /// No spot panel, or no walking button: tap nothing.
    Unknown,
}

pub(super) fn zero_attempts(label: &str) -> bool {
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

pub(super) fn panel_decision(
    title: Option<&str>,
    counter: Option<&str>,
    walk: Option<Point>,
) -> PanelDecision {
    if title.is_none() && counter.is_none() {
        return PanelDecision::Unknown;
    }
    if counter.is_some_and(zero_attempts) || title.is_some_and(zero_attempts) {
        return PanelDecision::Empty;
    }
    walk.map_or(PanelDecision::Unknown, PanelDecision::Walk)
}

/// Reads the spot panel that opened after tapping the marker at `marker`.
pub(super) fn read_panel(frame: &Frame, marker: Point) -> Result<PanelDecision, ActionError> {
    let lines = ocr::recognize(&frame.gray)?;
    let nearby = |line: &&ocr::TextLine| {
        let (x, y) = line.center();
        x.abs_diff(marker.0) <= 650 && y.abs_diff(marker.1) <= 700
    };
    // "Fishing Spot", "Lucky Spot", …
    let title = lines
        .iter()
        .filter(nearby)
        .find(|l| ascii_key(&l.text).contains("spot"));
    // The title can be clipped at the left map edge. The fish badge beside the
    // counter also identifies the spot panel.
    let mut counter = None;
    for line in lines.iter().filter(nearby).filter(|l| l.text.contains('/')) {
        let (x, y) = line.center();
        let badge = Rect {
            x: x.saturating_sub(150),
            y: y.saturating_sub(38),
            width: 155,
            height: 76,
        };
        if frame
            .find(&["fishing/map_counter.png"], badge, 0.79)?
            .is_some()
        {
            counter = Some(line);
            break;
        }
    }
    // Windows OCR does not read the italic counter of the zoomed-out map
    // ("5/5"); without it the spot is walked to, and the minigame finds out
    // within seconds when no attempts are left.
    let top = marker.1.saturating_sub(240);
    let bottom = marker.1.saturating_sub(32);
    let walk = frame.find(
        &["fishing/map_walk.png"],
        Rect {
            x: marker.0.saturating_sub(200),
            y: top,
            width: 390,
            height: bottom.saturating_sub(top),
        },
        0.83,
    )?;
    let decision = panel_decision(
        title.map(|l| l.text.as_str()),
        counter.map(|l| l.text.as_str()),
        walk,
    );
    trace::note(&format!(
        "panel title {:?} counter {:?} walk {walk:?} -> {decision:?}",
        title.map(|l| &l.text),
        counter.map(|l| &l.text)
    ));
    Ok(decision)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arrival {
    /// The fishing screen is open.
    Arrived,
    /// The character did not get there in time.
    Blocked,
}

/// Taps the walking button and waits until the character stands at the spot,
/// then opens fishing there.
pub(super) fn walk_to(
    driver: &Driver<'_>,
    size: Point,
    walk: Point,
    log: &mut impl FnMut(String),
) -> Result<Arrival, ActionError> {
    log(tr!("fishing-walking-to-the-fishing-spot").into());
    driver.tap(size, walk)?;
    let deadline = Instant::now() + crate::timing::secs(WALK_LIMIT_SECS);
    while Instant::now() < deadline {
        driver.settle()?;
        let frame = driver.read()?;
        // The game opens fishing by itself when the walk ends at a spot.
        if on_fishing_screen(&frame)? {
            trace::note("arrived, fishing open");
            return Ok(Arrival::Arrived);
        }
        let Some(rod) = frame.find(RODS, ARRIVAL, 0.76)? else {
            continue;
        };
        trace::note("arrived, rod visible");
        driver.tap(frame.size, rod)?;
        for _ in 0..6 {
            driver.settle()?;
            if on_fishing_screen(&driver.read()?)? {
                return Ok(Arrival::Arrived);
            }
        }
        break;
    }
    trace::note("did not arrive");
    Ok(Arrival::Blocked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_panel_masks_marker() {
        assert_eq!(panel_decision(None, None, None), PanelDecision::Unknown);
        assert_eq!(
            panel_decision(Some("Fishing Spot 0/10"), None, Some((500, 900))),
            PanelDecision::Empty
        );
        assert_eq!(
            panel_decision(None, Some("3/10"), Some((500, 900))),
            PanelDecision::Walk((500, 900))
        );
        assert_eq!(
            panel_decision(Some("Lucky Spot"), Some("5/5"), Some((938, 750))),
            PanelDecision::Walk((938, 750))
        );
        assert_eq!(
            panel_decision(None, Some("3/10"), None),
            PanelDecision::Unknown,
            "without a walking button nothing is tapped"
        );
    }

    #[test]
    fn the_arrow_is_found_above_its_coordinate_label() {
        let line = |text: &str, y: f32| ocr::TextLine {
            text: text.into(),
            x: 440.0,
            y,
            width: 120.0,
            height: 40.0,
        };
        assert_eq!(
            arrow_from_label(&[
                line("Exploration Progress 45%", 10.0),
                line("182; 299", 868.0)
            ]),
            Some((500, 843))
        );
        assert_eq!(
            arrow_from_label(&[line("126 ; 107", 100.0)]),
            Some((500, 75))
        );
        assert_eq!(arrow_from_label(&[line("Lvl. 306", 100.0)]), None);
    }

    #[test]
    fn the_quit_question_is_recognised() {
        let line = |text: &str| ocr::TextLine {
            text: text.into(),
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(exit_prompt(&[line(
            "Are you sure you want to exit the game?"
        )]));
        assert!(!exit_prompt(&[line("Return to World")]));
        assert!(leave_question(&[line(
            "Leave the current map and return to"
        )]));
        assert!(!leave_question(&[line(
            "Are you sure you want to exit the"
        )]));
    }

    #[test]
    fn return_to_world_is_found_in_the_lower_right() {
        let line = |text: &str| ocr::TextLine {
            text: text.into(),
            x: 100.0,
            y: 500.0,
            width: 200.0,
            height: 40.0,
        };
        assert_eq!(
            return_button(&[line("Track"), line("Return to World")]),
            Some((640, 1560))
        );
        assert_eq!(return_button(&[line("Refinery Door")]), None);
        assert!(in_world(&[line("Track"), line("Momestead"), line("Guild")]));
        assert!(!in_world(&[line("Track"), line("Return to World")]));
    }

    #[test]
    fn a_clamped_step_leaves_the_spot_where_it_was() {
        assert_eq!(unmoved((505, 770), Heading::Right), (505 + MAP_STEP, 770));
        assert_eq!(unmoved((505, 770), Heading::Up), (505, 770 - MAP_STEP));
    }

    #[test]
    fn steps_bring_a_spot_to_the_middle_of_the_view() {
        let centre = (505, 770);
        let (headings, screen) = steps_to((centre.0 + 2 * MAP_STEP, centre.1 - MAP_STEP), (0, 0));
        assert_eq!(headings, [Heading::Right, Heading::Right, Heading::Up]);
        assert_eq!(screen, centre);
        let (headings, screen) = steps_to((700, 900), (0, 0));
        assert!(headings.is_empty(), "already in view");
        assert_eq!(screen, (700, 900));
        // After walking, the view opens around the character.
        let (headings, _) = steps_to((300, 300), origin((2542, 958), (542, 958)));
        assert_eq!(
            headings,
            [
                Heading::Left,
                Heading::Left,
                Heading::Left,
                Heading::Left,
                Heading::Up
            ]
        );
    }

    #[test]
    fn the_arrow_fixes_the_view() {
        // Arrow at map point (0, 0) shown at (542, 958): the view's top left.
        assert_eq!(origin((0, 0), (542, 958)), (-542, -958));
        // A spot found at (700, 400) on that view is in reach again later.
        let o = origin((0, 0), (542, 958));
        let spot = (o.0 + 700, o.1 + 400);
        let later = origin((0, 0), (300, 1200));
        let (_, screen) = steps_to(spot, later);
        assert_eq!((screen.0, screen.1), (458, 642));
    }

    #[test]
    fn map_points_add_the_grid_steps() {
        assert_eq!(to_map((0, 0), (500, 400)), (500, 400));
        assert_eq!(
            to_map((2, -1), (500, 400)),
            (500 + 2 * MAP_STEP, 400 - MAP_STEP)
        );
    }

    #[test]
    fn the_slider_knob_is_the_short_wide_bright_run() {
        let mut gray = GrayImage::from_pixel(1080, 1920, image::Luma([20]));
        // Thin track along the whole slider.
        for y in 540..1220 {
            for x in 1008..1016 {
                gray.put_pixel(x, y, image::Luma([230]));
            }
        }
        assert_eq!(slider_knob(&gray), None);
        for y in 1153..1170 {
            for x in 998..1027 {
                gray.put_pixel(x, y, image::Luma([250]));
            }
        }
        assert_eq!(slider_knob(&gray), Some(1161));
    }
}
