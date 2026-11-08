//! Season Phase Rankings: free District / Server / Guild first-clear rewards.
use super::{afk_stages as common, daily, ocr};
use crate::{
    activity::{ActivityEvent, GameMode, RankingScope},
    adb,
    localization::{format_text, text},
};
use image::{GrayImage, RgbImage};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};

#[derive(Default)]
pub struct Cursor {
    completed: BTreeSet<(u32, RankingScope)>,
    pending: Option<Receipt>,
    collected: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Stage {
    kind: String,
    number: u32,
}

#[derive(Clone)]
struct Receipt {
    phase: u32,
    scope: RankingScope,
    stage: Stage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Claimable,
    Claimed,
    Locked,
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    stage: Stage,
    status: Status,
    point: (u32, u32),
}

struct Frame {
    gray: GrayImage,
    color: RgbImage,
    lines: Vec<ocr::TextLine>,
    size: (u32, u32),
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn digits(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn decode(bytes: &[u8]) -> Result<Frame, String> {
    let (gray, size) = common::normalize_screenshot(bytes)?;
    let color = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .resize_exact(1080, 1920, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let lines = ocr::recognize(&gray)?;
    Ok(Frame {
        gray,
        color,
        lines,
        size,
    })
}

fn read(serial: &str, cancelled: &AtomicBool) -> Result<Frame, String> {
    common::ensure_not_cancelled(cancelled)?;
    let frame = decode(&adb::screenshot(serial).map_err(|e| e.to_string())?)?;
    common::ensure_not_cancelled(cancelled)?;
    Ok(frame)
}

fn green_control(color: &RgbImage, line: &ocr::TextLine) -> bool {
    let left = (line.x as u32).saturating_sub(24);
    let top = (line.y as u32).saturating_sub(18);
    let right = ((line.x + line.width) as u32 + 24).min(color.width());
    let bottom = ((line.y + line.height) as u32 + 18).min(color.height());
    let mut green = 0;
    let mut total = 0;
    for y in (top..bottom).step_by(3) {
        for x in (left..right).step_by(3) {
            let [r, g, b] = color.get_pixel(x, y).0;
            total += 1;
            if g > 70 && u16::from(g) > u16::from(r) + 18 && u16::from(g) > u16::from(b) + 15 {
                green += 1;
            }
        }
    }
    total > 0 && green * 100 > total * 35
}

fn scope_tab<'a>(lines: &'a [ocr::TextLine], scope: RankingScope) -> Option<&'a ocr::TextLine> {
    lines.iter().find(|l| {
        l.y > 1730.0
            && l.y < 1830.0
            && l.x > 250.0
            && compact(&l.text).starts_with(&scope.label().to_lowercase())
    })
}

fn first_clear_header(lines: &[ocr::TextLine]) -> Option<(u32, RankingScope)> {
    let headers: Vec<_> = lines
        .iter()
        .filter_map(|l| {
            if !(440.0..620.0).contains(&l.y) {
                return None;
            }
            let title = compact(&l.text).replace("istclear", "1stclear");
            // The decorative final word can become "Rewa1üs" in Windows
            // OCR. Phase/scope/1st Clear plus the independently verified table
            // and active footer tab identify this page without guessing it.
            let (prefix, _) = title.split_once("1stclear")?;
            for scope in RankingScope::ALL {
                if let Some(phase) = prefix.strip_suffix(&scope.label().to_lowercase()) {
                    let number = phase
                        .strip_prefix("phase")
                        .or_else(|| phase.strip_prefix("hase"))?;
                    return digits(number)
                        .filter(|n| (1..=99).contains(n))
                        .map(|n| (n, scope));
                }
            }
            None
        })
        .collect();
    if headers.len() == 1 {
        Some(headers[0])
    } else {
        None
    }
}

fn first_clear(frame: &Frame) -> Option<(u32, RankingScope)> {
    let (phase, scope) = first_clear_header(&frame.lines)?;
    if RankingScope::ALL
        .iter()
        .any(|s| scope_tab(&frame.lines, *s).is_none())
    {
        return None;
    }
    for (label, minimum_x) in [
        ("stageprogress", 0.0),
        ("istclear", 350.0),
        ("istplayer", 600.0),
    ] {
        if !frame.lines.iter().any(|l| {
            (700.0..830.0).contains(&l.y)
                && l.x > minimum_x
                && compact(&l.text).replace("1st", "ist") == label
        }) {
            return None;
        }
    }
    let tab = scope_tab(&frame.lines, scope)?;
    green_control(&frame.color, tab).then_some((phase, scope))
}

fn phase_tabs(lines: &[ocr::TextLine]) -> BTreeMap<u32, (u32, u32)> {
    lines
        .iter()
        .filter_map(|l| {
            if !(700..=800).contains(&l.center().1) {
                return None;
            }
            let label = compact(&l.text);
            let suffix = label.strip_prefix("phase")?;
            let number = digits(suffix).or_else(|| matches!(suffix, "i" | "l").then_some(1))?;
            (1..=99).contains(&number).then_some((number, l.center()))
        })
        .collect()
}

fn ranking(frame: &Frame) -> bool {
    first_clear_header(&frame.lines).is_none()
        && !frame.lines.iter().any(|l| {
            l.x > 150.0
                && l.x < 850.0
                && l.y > 350.0
                && l.y < 600.0
                && (compact(&l.text).contains("rankingrewards")
                    || compact(&l.text).contains("seasonprogressrewards"))
        })
        && frame
            .lines
            .iter()
            .any(|l| l.y > 1800.0 && compact(&l.text) == "phaserankings")
        && !phase_tabs(&frame.lines).is_empty()
        && frame
            .lines
            .iter()
            .any(|l| l.x > 850.0 && l.y > 100.0 && l.y < 300.0 && compact(&l.text) == "phase")
        && frame
            .lines
            .iter()
            .any(|l| l.x > 850.0 && l.y > 300.0 && l.y < 570.0 && compact(&l.text) == "season")
}

fn selected_phase(frame: &Frame) -> Option<u32> {
    let tabs = phase_tabs(&frame.lines);
    let selected: Vec<_> = tabs
        .iter()
        .filter_map(|(&number, &point)| {
            let line = frame.lines.iter().find(|l| l.center() == point)?;
            let mut green = 0;
            let mut total = 0;
            for y in (line.y as u32..(line.y + line.height) as u32).step_by(2) {
                for x in (line.x as u32..(line.x + line.width) as u32).step_by(2) {
                    let [r, g, b] = frame.color.get_pixel(x, y).0;
                    total += 1;
                    if u16::from(g) > u16::from(r) + 18 && u16::from(g) > u16::from(b) + 15 {
                        green += 1;
                    }
                }
            }
            (total > 0 && green * 100 > total * 8).then_some(number)
        })
        .collect();
    (selected.len() == 1).then(|| selected[0])
}

fn first_clear_entry(frame: &Frame) -> Option<(u32, u32)> {
    frame
        .lines
        .iter()
        .find(|l| {
            l.x > 850.0
                && (150.0..590.0).contains(&l.y)
                && matches!(compact(&l.text).as_str(), "1stclear" | "istclear")
        })
        .map(|l| (l.center().0, l.center().1.saturating_sub(45)))
}

fn reward_badge(frame: &Frame, scope: RankingScope) -> bool {
    let center = match scope {
        RankingScope::District => 515,
        RankingScope::Server => 768,
        RankingScope::Guild => 1025,
    };
    let mut orange = 0;
    for y in (1740..1780).step_by(2) {
        for x in (center - 20..center + 20).step_by(2) {
            let [r, g, b] = frame.color.get_pixel(x, y).0;
            if r > 220 && (70..210).contains(&g) && b < 110 {
                orange += 1;
            }
        }
    }
    orange > 60
}

fn rows(frame: &Frame) -> Result<Vec<Row>, String> {
    let mut result = Vec::new();
    for status_line in &frame.lines {
        if status_line.x < 820.0 || !(810.0..1600.0).contains(&status_line.y) {
            continue;
        }
        let status = match compact(&status_line.text).as_str() {
            "claim" if green_control(&frame.color, status_line) => Status::Claimable,
            "claimed" => Status::Claimed,
            "notcleared" => Status::Locked,
            _ => continue,
        };
        let candidates: Vec<_> = frame
            .lines
            .iter()
            .filter_map(|number| {
                if number.x > 300.0
                    || number.x < 50.0
                    || number.height < 38.0
                    || (number.center().1 as i32 - status_line.center().1 as i32).abs() > 90
                {
                    return None;
                }
                let floor = digits(number.text.trim())?;
                let kind = frame.lines.iter().find(|l| {
                    l.x < 300.0
                        && l.y < number.y
                        && l.y > number.y - 120.0
                        && matches!(compact(&l.text).as_str(), "apex" | "stage" | "afkstage")
                })?;
                Some(Stage {
                    kind: compact(&kind.text),
                    number: floor,
                })
            })
            .collect();
        if candidates.len() == 1 {
            result.push(Row {
                stage: candidates[0].clone(),
                status,
                point: status_line.center(),
            });
        } else if status == Status::Claimable {
            return Err(text(
                "The ranking reward's stage could not be verified.",
                "Die Stufe der Ranglistenbelohnung konnte nicht bestätigt werden.",
            )
            .into());
        }
    }
    Ok(result)
}

fn quick_claim(frame: &Frame) -> bool {
    frame.lines.iter().any(|l| {
        l.x > 600.0
            && (1500.0..1700.0).contains(&l.y)
            && compact(&l.text) == "quickclaim"
            && green_control(&frame.color, l)
    })
}

fn loot_close(lines: &[ocr::TextLine]) -> Option<(u32, u32)> {
    let title = lines.iter().any(|l| {
        l.y < 1000.0
            && l.x > 40.0
            && l.width > 150.0
            && matches!(
                compact(&l.text).as_str(),
                "rewards" | "obtained" | "youreceived" | "youobtained"
            )
    });
    if !title {
        return None;
    }
    lines
        .iter()
        .find(|l| {
            l.y > 1500.0
                && matches!(
                    compact(&l.text).as_str(),
                    "taptoclose" | "tapanywheretoclose"
                )
        })
        .map(ocr::TextLine::center)
}

fn tap(
    serial: &str,
    point: (u32, u32),
    frame: &Frame,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    common::ensure_not_cancelled(cancelled)?;
    adb::tap_game_point(serial, point.0, point.1, frame.size).map_err(|e| e.to_string())?;
    common::settle_cancellable(cancelled)
}

fn wait_ranking(serial: &str, cancelled: &AtomicBool) -> Result<Frame, String> {
    for _ in 0..18 {
        let frame = read(serial, cancelled)?;
        if ranking(&frame) {
            return Ok(frame);
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "Season Phase Rankings could not be verified.",
        "Season Phase Rankings konnte nicht bestätigt werden.",
    )
    .into())
}

fn open_ranking(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Frame, String> {
    let frame = read(serial, cancelled)?;
    if ranking(&frame) {
        return Ok(frame);
    }
    if first_clear(&frame).is_some() {
        tap(serial, (90, 1790), &frame, cancelled)?;
        return wait_ranking(serial, cancelled);
    }
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    let frame = read(serial, cancelled)?;
    if !daily::page(&frame.gray)? {
        return Err(text(
            "The AFK page was not verified.",
            "Die AFK-Ansicht wurde nicht bestätigt.",
        )
        .into());
    }
    let entry = common::find_template(
        &frame.gray,
        "afk_stages/ranking_button.png",
        common::Rect {
            x: 850,
            y: 0,
            width: 230,
            height: 210,
        },
        0.8,
    )?
    .ok_or(text(
        "Season Rankings entry not found.",
        "Einstieg zu Season Rankings nicht gefunden.",
    ))?;
    common::ensure_not_cancelled(cancelled)?;
    common::tap_match(serial, entry, frame.size)?;
    common::settle_cancellable(cancelled)?;
    wait_ranking(serial, cancelled)
}

fn discover_phases(serial: &str, cancelled: &AtomicBool) -> Result<Vec<u32>, String> {
    let mut phases = BTreeSet::new();
    for _ in 0..8 {
        let frame = wait_ranking(serial, cancelled)?;
        let visible = phase_tabs(&frame.lines);
        let before = phases.len();
        phases.extend(visible.keys().copied());
        if phases.len() == before {
            return Ok(phases.into_iter().rev().collect());
        }
        // AdbAutoPlayer uses the right arrow to reveal older phases.
        tap(serial, (1023, 746), &frame, cancelled)?;
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The ranking phase list did not stabilize.",
        "Die Phasenliste der Rangliste ist nicht stabil.",
    )
    .into())
}

fn open_scope(
    serial: &str,
    phase: u32,
    scope: RankingScope,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Option<Frame>, String> {
    let frame = read(serial, cancelled)?;
    if first_clear(&frame) == Some((phase, scope)) {
        return Ok(Some(frame));
    }
    let mut frame = open_ranking(serial, cancelled, log)?;
    let mut found = false;
    for _ in 0..8 {
        if let Some(point) = phase_tabs(&frame.lines).get(&phase) {
            tap(serial, *point, &frame, cancelled)?;
            common::settle_cancellable(cancelled)?;
            frame = wait_ranking(serial, cancelled)?;
            found = true;
            break;
        }
        // The left arrow returns toward newer phases.
        tap(serial, (88, 746), &frame, cancelled)?;
        common::settle_cancellable(cancelled)?;
        frame = wait_ranking(serial, cancelled)?;
    }
    if !found {
        return Err(text(
            "The selected ranking phase could not be found.",
            "Die gewählte Ranglistenphase wurde nicht gefunden.",
        )
        .into());
    }
    for _ in 0..18 {
        if selected_phase(&frame) == Some(phase) {
            break;
        }
        common::settle_cancellable(cancelled)?;
        frame = wait_ranking(serial, cancelled)?;
    }
    if selected_phase(&frame) != Some(phase) {
        return Err(text(
            "The active ranking phase could not be verified.",
            "Die aktive Ranglistenphase konnte nicht bestätigt werden.",
        )
        .into());
    }
    if first_clear_entry(&frame).is_none() {
        common::settle_cancellable(cancelled)?;
        frame = wait_ranking(serial, cancelled)?;
        if selected_phase(&frame) == Some(phase) && first_clear_entry(&frame).is_none() {
            return Ok(None);
        }
    }
    let entry = first_clear_entry(&frame).ok_or("First-clear entry changed.")?;
    tap(serial, entry, &frame, cancelled)?;
    for _ in 0..18 {
        let next = read(serial, cancelled)?;
        if let Some((number, _)) = first_clear(&next) {
            if number != phase {
                return Err(text(
                    "The ranking phase changed.",
                    "Die Ranglistenphase hat sich verändert.",
                )
                .into());
            }
            let tab = scope_tab(&next.lines, scope).expect("first clear tabs verified");
            tap(serial, tab.center(), &next, cancelled)?;
            break;
        }
        common::settle_cancellable(cancelled)?;
    }
    for _ in 0..18 {
        let next = read(serial, cancelled)?;
        if first_clear(&next) == Some((phase, scope)) {
            return Ok(Some(next));
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The ranking reward tab could not be verified.",
        "Der Reiter für Ranglistenbelohnungen konnte nicht bestätigt werden.",
    )
    .into())
}

fn finish_claim(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<(), String> {
    let receipt = cursor.pending.clone().ok_or("No pending ranking reward")?;
    for attempt in 0..18 {
        let frame = read(serial, cancelled)?;
        if let Some(point) = loot_close(&frame.lines) {
            tap(serial, point, &frame, cancelled)?;
        } else if first_clear(&frame) == Some((receipt.phase, receipt.scope)) {
            let candidates = rows(&frame)?;
            if candidates
                .iter()
                .any(|r| r.stage == receipt.stage && r.status == Status::Claimed)
            {
                cursor.pending = None;
                cursor.collected += 1;
                report(ActivityEvent::Collected(
                    GameMode::SeasonRankingRewards(receipt.scope),
                    1,
                ));
                return Ok(());
            }
            if attempt >= 4 && !candidates.iter().any(|r| r.stage == receipt.stage) {
                let same_kind: Vec<_> = candidates
                    .iter()
                    .filter(|r| r.stage.kind == receipt.stage.kind)
                    .collect();
                let direction = if !same_kind.is_empty()
                    && same_kind
                        .iter()
                        .all(|r| r.stage.number < receipt.stage.number)
                {
                    Some(((540, 1510), (540, 930)))
                } else if !same_kind.is_empty()
                    && same_kind
                        .iter()
                        .all(|r| r.stage.number > receipt.stage.number)
                {
                    Some(((540, 930), (540, 1510)))
                } else {
                    None
                };
                if let Some((start, end)) = direction {
                    common::ensure_not_cancelled(cancelled)?;
                    adb::swipe_game_points(serial, start, end, frame.size, 650)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The ranking reward was not confirmed as claimed. No further claim was attempted.",
        "Die Ranglistenbelohnung wurde nicht als abgeholt bestätigt. Kein weiterer Abholversuch.",
    )
    .into())
}

fn collect_scope(
    serial: &str,
    phase: u32,
    scope: RankingScope,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<(), String> {
    let mut quiet = 0;
    let mut scrolls = 0;
    for _ in 0..120 {
        let frame = read(serial, cancelled)?;
        if first_clear(&frame) != Some((phase, scope)) {
            return Err(text(
                "The ranking reward view changed.",
                "Die Ansicht der Ranglistenbelohnungen hat sich verändert.",
            )
            .into());
        }
        let candidates = rows(&frame)?;
        if let Some(row) = candidates.iter().find(|r| r.status == Status::Claimable) {
            quiet = 0;
            common::settle_cancellable(cancelled)?;
            let fresh = read(serial, cancelled)?;
            if first_clear(&fresh) != Some((phase, scope)) {
                return Err("Ranking reward selection changed.".into());
            }
            let fresh_rows = rows(&fresh)?;
            let selected = fresh_rows
                .iter()
                .find(|r| r.stage == row.stage && r.status == Status::Claimable)
                .ok_or(text(
                    "The ranking claim button changed.",
                    "Die Abholschaltfläche hat sich verändert.",
                ))?;
            cursor.pending = Some(Receipt {
                phase,
                scope,
                stage: selected.stage.clone(),
            });
            tap(serial, selected.point, &fresh, cancelled)?;
            finish_claim(serial, cursor, cancelled, report)?;
        } else if quick_claim(&frame) || reward_badge(&frame, scope) {
            // Quick Claim proves more rewards exist, but individual rows give
            // independently verifiable receipts and accurate collection counts.
            quiet = 0;
            if scrolls >= 30 {
                return Err(text(
                    "Claimable ranking rewards could not be located.",
                    "Abholbare Ranglistenbelohnungen konnten nicht gefunden werden.",
                )
                .into());
            }
            common::ensure_not_cancelled(cancelled)?;
            adb::swipe_game_points(serial, (540, 1510), (540, 930), frame.size, 650)
                .map_err(|e| e.to_string())?;
            scrolls += 1;
            common::settle_cancellable(cancelled)?;
        } else if !candidates.is_empty() {
            quiet += 1;
            if quiet >= 2 {
                return Ok(());
            }
            common::settle_cancellable(cancelled)?;
        } else {
            return Err(text(
                "Ranking reward status could not be read.",
                "Der Status der Ranglistenbelohnungen konnte nicht gelesen werden.",
            )
            .into());
        }
    }
    Err(text(
        "Ranking rewards reached the collection limit.",
        "Das Abhollimit für Ranglistenbelohnungen wurde erreicht.",
    )
    .into())
}

pub fn run(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    if let Some(pending) = cursor.pending.clone() {
        let mode = GameMode::SeasonRankingRewards(pending.scope);
        report(ActivityEvent::ModeOpening(mode));
        open_scope(serial, pending.phase, pending.scope, cancelled, log)?.ok_or(text(
            "The pending reward's phase is no longer available.",
            "Die Phase der unbestätigten Belohnung ist nicht mehr verfügbar.",
        ))?;
        report(ActivityEvent::ModeEntered(mode));
        finish_claim(serial, cursor, cancelled, report)?;
    }
    report(ActivityEvent::ModeOpening(GameMode::SeasonRankingRewards(
        RankingScope::District,
    )));
    open_ranking(serial, cancelled, log)?;
    let phases = discover_phases(serial, cancelled)?;
    for phase in phases {
        for scope in RankingScope::ALL {
            if cursor.completed.contains(&(phase, scope)) {
                continue;
            }
            let mode = GameMode::SeasonRankingRewards(scope);
            report(ActivityEvent::ModeOpening(mode));
            if open_scope(serial, phase, scope, cancelled, log)?.is_none() {
                for skipped in RankingScope::ALL {
                    cursor.completed.insert((phase, skipped));
                }
                log(format_text!(
                    "Phase {phase} has no first-clear rewards section.",
                    "Phase {phase} hat keinen First-Clear-Belohnungsbereich."
                ));
                break;
            }
            report(ActivityEvent::ModeEntered(mode));
            collect_scope(serial, phase, scope, cursor, cancelled, report)?;
            cursor.completed.insert((phase, scope));
            log(format_text!(
                "Phase {phase} · {} rewards checked.",
                "Phase {phase} · {}-Belohnungen geprüft.",
                scope.label()
            ));
        }
    }
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    Ok(cursor.collected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(value: &str, x: f32, y: f32) -> ocr::TextLine {
        ocr::TextLine {
            text: value.into(),
            x,
            y,
            width: 120.0,
            height: 40.0,
        }
    }
    #[test]
    fn first_clear_context_is_specific_to_phase_and_scope() {
        assert_eq!(
            first_clear_header(&[line("hase 2 Server Ist ClearRewards", 160.0, 507.0)]),
            Some((2, RankingScope::Server))
        );
        assert_eq!(
            first_clear_header(&[line("hase 2 District Ist ClearRewa1üs", 207.0, 508.0)]),
            Some((2, RankingScope::District))
        );
        assert_eq!(
            first_clear_header(&[line("Phase 1 Guild 1st Clear Rewards", 160.0, 507.0)]),
            Some((1, RankingScope::Guild))
        );
        for wrong in [
            "Phase 2 Ranking Rewards",
            "Season Progress Rewards",
            "Phase l Server 1st Clear Rewards",
            "Claim",
        ] {
            assert_eq!(first_clear_header(&[line(wrong, 160.0, 507.0)]), None);
        }
        assert_eq!(
            first_clear_header(&[
                line("Phase 2 Server 1st Clear Rewards", 160.0, 507.0),
                line("Phase 2 Guild 1st Clear Rewards", 160.0, 550.0)
            ]),
            None
        );
    }
    #[test]
    fn phase_tabs_exclude_player_progress_and_reward_modals() {
        let found = phase_tabs(&[
            line("Phase 2", 280.0, 726.0),
            line("Phase 1", 740.0, 726.0),
            line("Phase Progress", 840.0, 870.0),
            line("Phase 2 Ranking Rewards", 200.0, 429.0),
        ]);
        assert_eq!(found.keys().copied().collect::<Vec<_>>(), [1, 2]);
    }
    #[test]
    fn stop_before_navigation_does_not_start_adb() {
        let mut cursor = Cursor::default();
        let mut events = Vec::new();
        assert!(run(
            "invalid-device",
            &mut cursor,
            &AtomicBool::new(true),
            &mut |_| {},
            &mut |e| events.push(e)
        )
        .is_err());
        assert!(events.is_empty());
    }
    #[test]
    #[ignore = "reads supplied ranking reward captures with local Windows OCR; no device input"]
    fn captured_ranking_reward_views() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_PHASE_REWARDS").expect("capture folder"),
        );
        let load = |name: &str| decode(&std::fs::read(root.join(name)).unwrap()).unwrap();
        assert!(ranking(&load("after-back.png")));
        assert_eq!(selected_phase(&load("after-back.png")), Some(2));
        let older = load("phase-one.png");
        assert!(ranking(&older));
        assert_eq!(selected_phase(&older), Some(1));
        assert!(first_clear_entry(&older).is_none());
        assert_eq!(
            first_clear(&load("district-ocr-variant.png")),
            Some((2, RankingScope::District))
        );
        assert!(!ranking(&load("phase.png")));
        assert!(!ranking(&load("season.png")));
        for scope in RankingScope::ALL {
            let frame = load(&format!("first-{}.png", scope.label().to_lowercase()));
            assert_eq!(first_clear(&frame), Some((2, scope)));
            let rows = rows(&frame).unwrap();
            let claimable: Vec<_> = rows
                .iter()
                .filter(|r| r.status == Status::Claimable)
                .collect();
            if scope == RankingScope::District {
                assert!(claimable.is_empty());
                assert!(!quick_claim(&frame));
                assert!(!reward_badge(&frame, scope));
            } else {
                assert_eq!(claimable.len(), 1);
                assert_eq!(claimable[0].stage.number, 500);
                assert!(quick_claim(&frame));
                assert!(reward_badge(&frame, scope));
            }
        }
    }
    #[test]
    #[ignore = "collects free Season Phase first-clear rewards on a connected emulator, then reruns to check that claimed rows are skipped"]
    fn connected_emulator_collect_rewards() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("serial");
        let cancelled = AtomicBool::new(false);
        let mut events = Vec::new();
        let total = run(
            &serial,
            &mut Cursor::default(),
            &cancelled,
            &mut |s| println!("{s}"),
            &mut |e| {
                println!("{e:?}");
                events.push(e);
            },
        )
        .unwrap();
        let confirmed: u32 = events
            .iter()
            .filter_map(|e| match e {
                ActivityEvent::Collected(GameMode::SeasonRankingRewards(_), n) => Some(*n),
                _ => None,
            })
            .sum();
        assert_eq!(confirmed, total);
        println!("Confirmed ranking rewards: {total}");
        assert_eq!(
            run(
                &serial,
                &mut Cursor::default(),
                &cancelled,
                &mut |s| println!("{s}"),
                &mut |e| println!("{e:?}")
            )
            .unwrap(),
            0
        );
    }
}
