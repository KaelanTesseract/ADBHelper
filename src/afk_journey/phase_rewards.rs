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
    claim: Claim,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Claim {
    Stage(Stage),
    Quick,
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

pub(super) fn green_control(color: &RgbImage, line: &ocr::TextLine) -> bool {
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

fn ranking_control(frame: &Frame, label: &str, top: std::ops::Range<f32>) -> Option<f32> {
    frame
        .lines
        .iter()
        .find(|line| line.x > 850.0 && top.contains(&line.y) && compact(&line.text) == label)
        .map(|line| line.y)
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
        && ranking_control(frame, "phase", 100.0..300.0).is_some()
        && ranking_control(frame, "season", 300.0..570.0).is_some()
}

fn selected_phase(frame: &Frame) -> Option<u32> {
    let tabs = phase_tabs(&frame.lines);
    let selected: Vec<_> = tabs
        .iter()
        .filter_map(|(&number, &point)| {
            let line = frame.lines.iter().find(|l| l.center() == point)?;
            // The active tab's underline has a green segment left of the label.
            // The lettering itself changes color across ranking views.
            let left = line.x as u32;
            let green = (792..804).step_by(2).any(|y| {
                (left.saturating_sub(90)..left.saturating_sub(4))
                    .filter(|&x| {
                        let [r, g, b] = frame.color.get_pixel(x, y).0;
                        u16::from(g) > u16::from(r) + 18 && u16::from(g) > u16::from(b) + 15
                    })
                    .count()
                    >= 24
            });
            green.then_some(number)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeEntry {
    Available((u32, u32)),
    Unavailable,
}

fn scope_entry(frame: &Frame, phase: u32) -> Option<ScopeEntry> {
    if !ranking(frame) || selected_phase(frame) != Some(phase) {
        return None;
    }
    if let Some(point) = first_clear_entry(frame) {
        return Some(ScopeEntry::Available(point));
    }
    let phase_button = ranking_control(frame, "phase", 100.0..300.0)?;
    let season_button = ranking_control(frame, "season", 300.0..570.0)?;
    // With no First Clear button, Season occupies the slot directly below Phase.
    (100.0..170.0)
        .contains(&(season_button - phase_button))
        .then_some(ScopeEntry::Unavailable)
}

fn reward_badge(frame: &Frame, scope: RankingScope) -> bool {
    let center = match scope {
        RankingScope::District => 515,
        RankingScope::Server => 768,
        RankingScope::Guild => 1025,
    };
    let mut orange = 0;
    let mut gold = 0;
    for y in (1740..1784).step_by(2) {
        for x in (center - 20..center + 20).step_by(2) {
            let [r, g, b] = frame.color.get_pixel(x, y).0;
            if r > 220 && (70..210).contains(&g) && b < 110 {
                orange += 1;
            }
            // The yellow gift glyph sits inside the orange badge.
            if r >= 235 && g >= 195 && (70..=150).contains(&b) {
                gold += 1;
            }
        }
    }
    orange > 60 && gold > 20
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

fn quick_claim(frame: &Frame) -> Option<(u32, u32)> {
    frame
        .lines
        .iter()
        .find(|l| {
            l.x > 600.0
                && (1500.0..1700.0).contains(&l.y)
                && compact(&l.text) == "quickclaim"
                && green_control(&frame.color, l)
        })
        .map(ocr::TextLine::center)
}

fn claim_control(frame: &Frame) -> Result<Option<(Claim, (u32, u32))>, String> {
    let quick = quick_claim(frame).map(|point| (Claim::Quick, point));
    match rows(frame) {
        Ok(rows) => Ok(rows
            .into_iter()
            .find(|row| row.status == Status::Claimable)
            .map(|row| (Claim::Stage(row.stage), row.point))
            .or(quick)),
        Err(_) if quick.is_some() => Ok(quick),
        Err(error) => Err(error),
    }
}

pub(super) fn loot_close(lines: &[ocr::TextLine]) -> Option<(u32, u32)> {
    let title = lines.iter().any(|l| {
        l.y < 1000.0
            && l.x > 40.0
            && l.width > 150.0
            && matches!(
                compact(&l.text).as_str(),
                "rewards"
                    | "obtained"
                    | "rewardsobtained"
                    | "rewardsbtained"
                    | "youreceived"
                    | "youobtained"
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
    let mut phase_tab = None;
    for index in 0..8 {
        phase_tab = phase_tabs(&frame.lines).get(&phase).copied();
        if phase_tab.is_some() || index == 7 {
            break;
        }
        // The left arrow returns toward newer phases.
        tap(serial, (88, 746), &frame, cancelled)?;
        common::settle_cancellable(cancelled)?;
        frame = wait_ranking(serial, cancelled)?;
    }
    let phase_tab = phase_tab.ok_or_else(|| {
        text(
            "The selected ranking phase could not be found.",
            "Die gewählte Ranglistenphase wurde nicht gefunden.",
        )
        .to_owned()
    })?;
    if selected_phase(&frame) != Some(phase) {
        tap(serial, phase_tab, &frame, cancelled)?;
        frame = read(serial, cancelled)?;
    }
    let mut unavailable = false;
    let mut opened = false;
    for _ in 0..18 {
        let entry = scope_entry(&frame, phase);
        match entry {
            Some(ScopeEntry::Available(point)) => {
                tap(serial, point, &frame, cancelled)?;
                opened = true;
                break;
            }
            Some(ScopeEntry::Unavailable) if unavailable => return Ok(None),
            _ => unavailable = entry == Some(ScopeEntry::Unavailable),
        }
        common::settle_cancellable(cancelled)?;
        frame = read(serial, cancelled)?;
    }
    if !opened {
        return Err(text(
            "The First Clear entry for the selected phase could not be verified.",
            "Der First-Clear-Einstieg für die ausgewählte Phase konnte nicht bestätigt werden.",
        )
        .into());
    }
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
    let mut badge_absent = false;
    for attempt in 0..18 {
        let frame = read(serial, cancelled)?;
        if let Some(point) = loot_close(&frame.lines) {
            badge_absent = false;
            tap(serial, point, &frame, cancelled)?;
        } else if first_clear(&frame) == Some((receipt.phase, receipt.scope)) {
            let Claim::Stage(stage) = &receipt.claim else {
                let absent = !reward_badge(&frame, receipt.scope);
                if absent && badge_absent {
                    cursor.pending = None;
                    cursor.collected += 1;
                    report(ActivityEvent::Collected(
                        GameMode::SeasonRankingRewards(receipt.scope),
                        1,
                    ));
                    return Ok(());
                }
                badge_absent = absent;
                common::settle_cancellable(cancelled)?;
                continue;
            };
            let candidates = rows(&frame)?;
            if candidates
                .iter()
                .any(|r| r.stage == *stage && r.status == Status::Claimed)
            {
                cursor.pending = None;
                cursor.collected += 1;
                report(ActivityEvent::Collected(
                    GameMode::SeasonRankingRewards(receipt.scope),
                    1,
                ));
                return Ok(());
            }
            if attempt >= 4 && !candidates.iter().any(|r| r.stage == *stage) {
                let same_kind: Vec<_> = candidates
                    .iter()
                    .filter(|r| r.stage.kind == stage.kind)
                    .collect();
                let direction = if !same_kind.is_empty()
                    && same_kind.iter().all(|r| r.stage.number < stage.number)
                {
                    Some(((540, 1510), (540, 930)))
                } else if !same_kind.is_empty()
                    && same_kind.iter().all(|r| r.stage.number > stage.number)
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
        } else {
            badge_absent = false;
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
        if !reward_badge(&frame, scope) {
            quiet += 1;
            if quiet >= 2 {
                return Ok(());
            }
            common::settle_cancellable(cancelled)?;
            continue;
        }
        quiet = 0;
        let Some((claim, _)) = claim_control(&frame)? else {
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
            continue;
        };
        common::settle_cancellable(cancelled)?;
        let fresh = read(serial, cancelled)?;
        if first_clear(&fresh) != Some((phase, scope)) {
            return Err("Ranking reward selection changed.".into());
        }
        if !reward_badge(&fresh, scope) {
            continue;
        }
        let Some((verified, point)) = claim_control(&fresh)? else {
            continue;
        };
        if verified != claim {
            continue;
        }
        cursor.pending = Some(Receipt {
            phase,
            scope,
            claim,
        });
        tap(serial, point, &fresh, cancelled)?;
        finish_claim(serial, cursor, cancelled, report)?;
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
    fn first_clear_waits_for_the_selected_phase_and_uses_the_button_layout() {
        let mut frame = Frame {
            gray: GrayImage::new(1080, 1920),
            color: RgbImage::new(1080, 1920),
            lines: vec![
                line("Phase 2", 280.0, 726.0),
                line("Phase 1", 735.0, 726.0),
                line("Phase Rankings", 525.0, 1879.0),
                line("Phase", 942.0, 235.0),
                line("Season", 937.0, 368.0),
            ],
            size: (1080, 1920),
        };
        assert_eq!(scope_entry(&frame, 1), None);
        for x in 670..710 {
            frame.color.put_pixel(x, 796, image::Rgb([75, 147, 83]));
        }
        assert_eq!(scope_entry(&frame, 2), None);
        assert_eq!(scope_entry(&frame, 1), Some(ScopeEntry::Unavailable));
        frame.lines.last_mut().unwrap().y = 500.0;
        assert_eq!(scope_entry(&frame, 1), None);
        frame.lines.push(line("Ist Clear", 928.0, 367.0));
        assert_eq!(
            scope_entry(&frame, 1),
            Some(ScopeEntry::Available((988, 342)))
        );
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
        let current = load("after-back.png");
        assert!(ranking(&current));
        assert_eq!(selected_phase(&current), Some(2));
        assert!(matches!(
            scope_entry(&current, 2),
            Some(ScopeEntry::Available(_))
        ));
        assert_eq!(scope_entry(&current, 1), None);
        let older = load("phase-one.png");
        assert!(ranking(&older));
        assert_eq!(selected_phase(&older), Some(1));
        assert_eq!(scope_entry(&older, 1), Some(ScopeEntry::Unavailable));
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
                assert!(quick_claim(&frame).is_none());
                assert!(!reward_badge(&frame, scope));
            } else {
                assert_eq!(claimable.len(), 1);
                assert_eq!(claimable[0].stage.number, 500);
                assert!(quick_claim(&frame).is_some());
                assert!(reward_badge(&frame, scope));
            }
        }
    }
    #[test]
    #[ignore = "reads the Server reward view before and after Quick Claim; no device input"]
    fn captured_quick_claim_with_offscreen_rewards() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_PHASE_REWARDS").expect("capture folder"),
        );
        let load = |name: &str| decode(&std::fs::read(root.join(name)).unwrap()).unwrap();
        let before = load("current-badge.png");
        let after = load("quick-claim-result.png");
        for frame in [&before, &after] {
            assert_eq!(first_clear(frame), Some((2, RankingScope::Server)));
            assert!(!reward_badge(frame, RankingScope::District));
            assert!(reward_badge(frame, RankingScope::Guild));
            assert!(rows(frame)
                .unwrap()
                .iter()
                .all(|row| row.status != Status::Claimable));
        }
        assert!(reward_badge(&before, RankingScope::Server));
        let Some((Claim::Quick, (x, y))) = claim_control(&before).unwrap() else {
            panic!("Quick Claim must be available for the off-screen rewards");
        };
        assert!((642..1045).contains(&x) && (1580..1690).contains(&y));
        assert!(!reward_badge(&after, RankingScope::Server));
        assert_eq!(claim_control(&after).unwrap(), None);
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
