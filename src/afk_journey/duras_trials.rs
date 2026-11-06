use super::afk_stages::{self as common, BattleSettings, Rect, StageResult};
use crate::activity::{ActivityEvent, GameMode};
use crate::adb;
use crate::localization::{format_text, text};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn overview_region() -> Rect {
    Rect {
        x: 756,
        y: 0,
        width: 324,
        height: 384,
    }
}

fn result_next_region() -> Rect {
    Rect {
        x: 648,
        y: 1728,
        width: 432,
        height: 192,
    }
}

fn navigate(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    common::ensure_not_cancelled(cancelled)?;
    let (mut screen, _) = common::screenshot(serial)?;
    if common::get_match(
        &screen,
        &[
            "navigation/confirm.png",
            "confirm_text.png",
            "cancel.png",
            "duras_trials/first_clear_bottom_half.png",
            "duras_trials/end_sunrise.png",
        ],
        common::full_screen(),
        0.8,
    )?
    .is_some()
    {
        adb::press_back(serial).map_err(|e| e.to_string())?;
        common::settle_cancellable(cancelled)?;
        screen = common::screenshot(serial)?.0;
    }
    if common::find_template(
        &screen,
        "duras_trials/sweep.png",
        Rect {
            x: 216,
            y: 1710,
            width: 864,
            height: 180,
        },
        0.9,
    )?
    .is_some()
        && common::find_template(
            &screen,
            "duras_trials/socketed_charms_overview.png",
            overview_region(),
            0.8,
        )?
        .is_none()
    {
        // Dismiss a previously opened quantity dialog without confirming it.
        adb::press_back(serial).map_err(|e| e.to_string())?;
        common::settle_cancellable(cancelled)?;
        screen = common::screenshot(serial)?.0;
    }
    if common::find_template(
        &screen,
        "duras_trials/socketed_charms_overview.png",
        overview_region(),
        0.78,
    )?
    .is_some()
    {
        return Ok(());
    }
    for _ in 0..3 {
        common::navigate_to_battle_modes(serial, cancelled, log)?;
        for swipe in 0..=4 {
            common::ensure_not_cancelled(cancelled)?;
            let (screen, size) = common::screenshot(serial)?;
            if let Some(entry) = common::find_template(
                &screen,
                "battle_modes/duras_trials.png",
                common::full_screen(),
                0.75,
            )? {
                if let Some(coming_soon) = common::find_template(
                    &screen,
                    "battle_modes/coming_soon.png",
                    common::full_screen(),
                    0.78,
                )? {
                    if coming_soon.center().1 < entry.center().1 {
                        return Err(text(
                            "Dura’s Trials is not unlocked yet (Coming Soon).",
                            "Dura’s Trials sind noch nicht freigeschaltet (Coming Soon).",
                        )
                        .into());
                    }
                }
                common::tap_match(serial, entry, size)?;
                for _ in 0..3 {
                    common::settle_cancellable(cancelled)?;
                    let (screen, current_size) = common::screenshot(serial)?;
                    if let Some((_, close)) = common::get_match(
                        &screen,
                        &["guide/close.png", "guide/next.png", "tap_to_close.png"],
                        common::full_screen(),
                        0.8,
                    )? {
                        common::tap_match(serial, close, current_size)?;
                    }
                }
                if common::poll_for_match(
                    serial,
                    &["duras_trials/socketed_charms_overview.png"],
                    overview_region(),
                    Duration::from_secs(20),
                    cancelled,
                )?
                .is_some()
                {
                    return Ok(());
                }
                break;
            }
            if swipe < 4 {
                adb::swipe_game_points(serial, (540, 1200), (540, 700), size, 1500)
                    .map_err(|e| e.to_string())?;
                common::settle_cancellable(cancelled)?;
            }
        }
    }
    Err(text(
        "Could not open Dura’s Trials.",
        "Dura’s Trials konnten nicht geöffnet werden.",
    )
    .into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectedCard {
    left: u32,
    right: u32,
}

fn selected_card(image: &image::RgbImage) -> Option<SelectedCard> {
    if image.dimensions() != (1080, 1920) {
        return None;
    }
    let mut edges = Vec::<u32>::new();
    for x in 0..image.width() {
        let green = (1130..1520)
            .filter(|&y| {
                let [r, g, b] = image.get_pixel(x, y).0;
                g >= 100 && u16::from(g) > u16::from(r) + 30 && u16::from(g) > u16::from(b) + 20
            })
            .count();
        if green >= 250 && edges.last().is_none_or(|&last| x > last + 5) {
            edges.push(x);
        }
    }
    let pairs: Vec<_> = edges
        .windows(2)
        .filter(|e| (350..=600).contains(&(e[1] - e[0])))
        .map(|e| SelectedCard {
            left: e[0],
            right: e[1],
        })
        .collect();
    (pairs.len() == 1).then(|| pairs[0])
}

fn enabled_sweep_color(image: &image::RgbImage, rect: Rect) -> bool {
    let mut orange = 0u32;
    let mut total = 0u32;
    for y in (rect.y..(rect.y + rect.height).min(image.height())).step_by(2) {
        for x in (rect.x..(rect.x + rect.width).min(image.width())).step_by(2) {
            let [r, g, b] = image.get_pixel(x, y).0;
            total += 1;
            if r >= 180 && (80..=220).contains(&g) && b <= 130 && u16::from(r) > u16::from(g) + 20 {
                orange += 1;
            }
        }
    }
    total > 0 && orange * 5 >= total
}

fn exact_number(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn unique_value(values: impl Iterator<Item = u32>) -> Option<u32> {
    let values: Vec<_> = values.collect();
    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}

fn repeated_counter(text: &str) -> Option<u32> {
    let text = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if !text.bytes().all(|c| c.is_ascii_digit()) || ![3, 6].contains(&text.len()) {
        return None;
    }
    let len = text.len() / 3;
    let first = &text[..len];
    (first == &text[len..len * 2] && first == &text[len * 2..])
        .then(|| exact_number(first))
        .flatten()
}

fn repeated_numeric_patch(
    gray: &image::GrayImage,
    roi: Rect,
) -> Result<Vec<super::ocr::TextLine>, String> {
    // Windows OCR omits some isolated one-digit counters. Repeat the same
    // captured glyph three times as a single word; accept only three identical
    // readings. No digit is generated, guessed or taken from the Sweep price.
    let mut left = roi.width;
    let mut top = roi.height;
    let (mut right, mut bottom) = (0, 0);
    for y in 0..roi.height {
        for x in 0..roi.width {
            if gray.get_pixel(roi.x + x, roi.y + y).0[0] > 180 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    if right <= left
        || bottom <= top
        || !(6..=65).contains(&(right - left))
        || !(20..=45).contains(&(bottom - top))
    {
        return Ok(Vec::new());
    }
    let (width, height) = (right - left, bottom - top);
    let mut word = image::GrayImage::from_pixel(width * 3 + 64, height + 60, image::Luma([255]));
    for copy in 0..3 {
        for y in 0..height {
            for x in 0..width {
                let value = gray.get_pixel(roi.x + left + x, roi.y + top + y).0[0];
                word.put_pixel(
                    30 + copy * (width + 2) + x,
                    30 + y,
                    image::Luma([if value > 180 { 0 } else { 255 }]),
                );
            }
        }
    }
    let word = image::imageops::resize(
        &word,
        word.width() * 4,
        word.height() * 4,
        image::imageops::FilterType::CatmullRom,
    );
    super::ocr::recognize(&word)
}

fn isolated_daily_counter(gray: &image::GrayImage) -> Result<Option<u32>, String> {
    Ok(unique_value(
        repeated_numeric_patch(
            gray,
            Rect {
                x: 900,
                y: 10,
                width: 100,
                height: 90,
            },
        )?
        .iter()
        .filter_map(|line| repeated_counter(&line.text)),
    ))
}

fn daily_limit(lines: &[super::ocr::TextLine]) -> Option<u32> {
    unique_value(lines.iter().filter_map(|line| {
        let (_, y) = line.center();
        if !(930..=1110).contains(&y) {
            return None;
        }
        let text: String = line
            .text
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let number = [
            "dailylimit:",
            "tageslimit:",
            "täglicheslimit:",
            "taglicheslimit:",
        ]
        .into_iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .and_then(exact_number)?;
        (1..=999).contains(&number).then_some(number)
    }))
}

fn reached_cap_stage(
    lines: &[super::ocr::TextLine],
    card: Option<SelectedCard>,
    stage: Option<u32>,
    selected_max_template: bool,
) -> Option<u32> {
    let card = card?;
    let stage = stage.filter(|n| (1..=999).contains(n))?;
    // Once the last first-clear is won, the numeric Daily Limit is replaced
    // by this explicit status. Also require Max on THIS selected green card.
    let reached = lines.iter().any(|line| {
        let (x, y) = line.center();
        (250..=830).contains(&x)
            && (1000..=1100).contains(&y)
            && line
                .text
                .trim()
                .eq_ignore_ascii_case("Current Limit Reached")
    });
    let selected_max = lines.iter().any(|line| {
        let (x, y) = line.center();
        x > card.right.saturating_sub(140)
            && x < card.right
            && (1130..=1205).contains(&y)
            && line.text.trim().eq_ignore_ascii_case("Max")
    });
    (reached && (selected_max || selected_max_template)).then_some(stage)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SweepCost {
    available: u32,
    cost: u32,
}

fn text_sweep_cost(text: &str) -> Option<SweepCost> {
    let costs = text
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '/');
            let (available, cost) = token.split_once('/')?;
            let available = exact_number(available)?;
            let cost = exact_number(cost)?;
            (available <= 99 && (1..=99).contains(&cost)).then_some(SweepCost { available, cost })
        })
        .collect::<Vec<_>>();
    (costs.len() == 1).then(|| costs[0])
}

fn repeated_price(text: &str) -> Option<SweepCost> {
    let text = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if !text.bytes().all(|c| c.is_ascii_digit() || c == b'/') || text.len() % 3 != 0 {
        return None;
    }
    let len = text.len() / 3;
    if len == 0 || text[..len] != text[len..len * 2] || text[..len] != text[len * 2..] {
        return None;
    }
    text_sweep_cost(&text[..len])
}

fn sweep_cost(lines: &[super::ocr::TextLine]) -> Option<SweepCost> {
    let values: Vec<_> = lines
        .iter()
        .filter_map(|line| {
            let (x, y) = line.center();
            if !(216..=1040).contains(&x) || !(1540..=1740).contains(&y) {
                return None;
            }
            let text: String = line.text.chars().filter(|c| !c.is_whitespace()).collect();
            let text = text.trim_matches(|c: char| !c.is_ascii_digit() && c != '/');
            let (available, cost) = text.split_once('/')?;
            let available = exact_number(available)?;
            let cost = exact_number(cost)?;
            (available <= 99 && cost > 0 && cost <= 99).then_some(SweepCost { available, cost })
        })
        .collect();
    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    trial: String,
    limit: Option<u32>,
    stage: Option<u32>,
    daily_remaining: Option<u32>,
    overview: bool,
    sweep: bool,
    sweep_enabled: bool,
    sweep_cost: Option<SweepCost>,
    battle: bool,
    blocked: bool,
}

struct Frame {
    gray: image::GrayImage,
    color: image::RgbImage,
    size: (u32, u32),
    lines: Vec<super::ocr::TextLine>,
    snapshot: Snapshot,
}

fn trial_key(title: &str) -> String {
    let key: String = title
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    // OCR reads the oversized R in the Raid overview as a separate word
    // and can omit it entirely from the Sweep dialog.
    match key.as_str() {
        "aidtreasure" => "raidtreasure".into(),
        _ => key,
    }
}

fn overview_trial(lines: &[super::ocr::TextLine]) -> String {
    let mut titles = lines.iter().filter_map(|line| {
        let (_, y) = line.center();
        if !(90..=205).contains(&y)
            || line.x < 25.0
            || line.x + line.width > 850.0
            || line.width <= 120.0
            || line.height < 40.0
        {
            return None;
        }
        let title = trial_key(&line.text);
        (!title.is_empty()).then_some(title)
    });
    let title = titles.next().unwrap_or_default();
    if titles.next().is_some() {
        return String::new();
    }
    title
}

fn read_frame(serial: &str, cancelled: &AtomicBool) -> Result<Frame, String> {
    common::ensure_not_cancelled(cancelled)?;
    let bytes = adb::screenshot(serial).map_err(|e| e.to_string())?;
    let frame = decode_frame(&bytes)?;
    common::ensure_not_cancelled(cancelled)?;
    Ok(frame)
}

fn decode_frame(bytes: &[u8]) -> Result<Frame, String> {
    let (gray, size) = common::normalize_screenshot(bytes)?;
    let color = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .resize_exact(1080, 1920, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let lines = super::ocr::recognize(&gray)?;
    let card = selected_card(&color);
    let mut stage = card.and_then(|card| {
        unique_value(lines.iter().filter_map(|line| {
            let (x, y) = line.center();
            if x > card.left + 20
                && x < card.right - 20
                && (1140..=1290).contains(&y)
                && line.height >= 40.0
            {
                exact_number(&line.text).filter(|&n| (1..=999).contains(&n))
            } else {
                None
            }
        }))
    });
    if stage.is_none() {
        if let Some(card) = card {
            // Decorative portrait backgrounds can hide a digit from full-frame
            // OCR. Read only the number area of the verified selected card,
            // with white padding and extra contrast. Never read a neighbour.
            let left = card.left + 170;
            let width = 170;
            if left + width < card.right {
                let mut number = image::GrayImage::from_pixel(230, 170, image::Luma([255]));
                for y in 0..110 {
                    for x in 0..width {
                        let value = gray.get_pixel(left + x, 1160 + y).0[0];
                        number.put_pixel(
                            x + 30,
                            y + 30,
                            image::Luma([if value < 160 { 0 } else { 255 }]),
                        );
                    }
                }
                let number = image::imageops::resize(
                    &number,
                    920,
                    680,
                    image::imageops::FilterType::Nearest,
                );
                stage = unique_value(super::ocr::recognize(&number)?.iter().filter_map(|line| {
                    exact_number(&line.text).filter(|&n| (1..=99).contains(&n))
                }));
            }
        }
    }
    let selected_max = if let Some(card) = card {
        common::find_template(
            &gray,
            "duras_trials/max_badge.png",
            Rect {
                x: card.right.saturating_sub(140),
                y: 1130,
                width: 140,
                height: 75,
            },
            0.95,
        )?
        .is_some()
    } else {
        false
    };
    let mut daily_remaining = unique_value(lines.iter().filter_map(|line| {
        let (x, y) = line.center();
        if (870..=1050).contains(&x) && (15..=100).contains(&y) {
            exact_number(&line.text).filter(|&n| n <= 99)
        } else {
            None
        }
    }));
    if daily_remaining.is_none() {
        daily_remaining = isolated_daily_counter(&gray)?;
    }
    let blocked = common::get_match(
        &gray,
        &["navigation/confirm.png", "cancel.png", "next.png"],
        common::lower_screen(),
        0.85,
    )?
    .is_some()
        || common::find_template(
            &gray,
            "duras_trials/no_next.png",
            result_next_region(),
            0.85,
        )?
        .is_some()
        || lines.iter().any(|line| {
            let text = line.text.trim().to_lowercase();
            line.y > 350.0
                && line.y < 1250.0
                && ["rewards", "obtained", "sweep", "belohnungen", "erhalten"]
                    .contains(&text.as_str())
        });
    let sweep_button = common::find_template(
        &gray,
        "duras_trials/sweep.png",
        Rect {
            x: 216,
            y: 1520,
            width: 864,
            height: 240,
        },
        0.9,
    )?;
    let snapshot = Snapshot {
        trial: overview_trial(&lines),
        blocked,
        stage,
        daily_remaining,
        limit: daily_limit(&lines).or_else(|| reached_cap_stage(&lines, card, stage, selected_max)),
        overview: common::find_template(
            &gray,
            "duras_trials/socketed_charms_overview.png",
            overview_region(),
            0.85,
        )?
        .is_some(),
        sweep: sweep_button.is_some(),
        sweep_enabled: sweep_button.is_some_and(|found| enabled_sweep_color(&color, found.rect())),
        sweep_cost: sweep_cost(&lines),
        battle: common::find_template(
            &gray,
            "duras_trials/battle.png",
            Rect {
                x: 216,
                y: 1520,
                width: 864,
                height: 240,
            },
            0.8,
        )?
        .is_some(),
    };
    #[cfg(test)]
    {
        println!("Dura observation: {snapshot:?}");
        if snapshot.stage.is_none()
            || snapshot.limit.is_none()
            || snapshot.daily_remaining.is_none()
        {
            let _ = color.save(".update-test/dura-unreadable.png");
            for line in &lines {
                println!("OCR: {line:?}");
            }
        }
    }
    Ok(Frame {
        gray,
        color,
        size,
        lines,
        snapshot,
    })
}

#[derive(Debug)]
struct CapPermit {
    stage: u32,
    daily_remaining: u32,
    trial: String,
}

fn confirm_cap(first: &Snapshot, second: &Snapshot) -> Result<CapPermit, String> {
    for frame in [first, second] {
        if !frame.overview
            || !frame.sweep
            || frame.battle
            || frame.blocked
            || frame.trial.is_empty()
        {
            return Err(text(
                "Sweeps require a verified stage cap.",
                "Die Maximalstufe ist nicht eindeutig bestätigt; Sweep bleibt gesperrt.",
            )
            .into());
        }
        let (Some(limit), Some(stage), Some(_)) = (frame.limit, frame.stage, frame.daily_remaining)
        else {
            return Err(text(
                "Could not read the stage, daily cap, or remaining attempts. Sweeps stopped.",
                "Stufe, Tageslimit oder tägliche Versuche unlesbar; kein Sweep.",
            )
            .into());
        };
        if limit == 0 || limit > 999 || stage != limit || frame.daily_remaining.unwrap() > 99 {
            return Err(text(
                "The selected stage is below the verified cap. Sweeps stopped.",
                "Die ausgewählte Stufe ist nicht die aktuelle Maximalstufe; kein Sweep.",
            )
            .into());
        }
    }
    if first.daily_remaining.unwrap() > 0 {
        for frame in [first, second] {
            if !frame.sweep_enabled
                || frame.sweep_cost
                    != Some(SweepCost {
                        available: frame.daily_remaining.unwrap(),
                        cost: 1,
                    })
            {
                return Err(text("Sweeps require a verified cost of one daily attempt.", "Tägliche Sweep-Kosten nicht eindeutig als ein Tagesversuch bestätigt; kein Sweep.").into());
            }
        }
    }
    if first.trial != second.trial
        || first.limit != second.limit
        || first.stage != second.stage
        || first.daily_remaining != second.daily_remaining
    {
        return Err(text(
            "The stage verification returned inconsistent readings. Sweeps stopped.",
            "Die Stufenprüfung ist nicht stabil; kein Sweep.",
        )
        .into());
    }
    Ok(CapPermit {
        stage: first.stage.unwrap(),
        daily_remaining: first.daily_remaining.unwrap(),
        trial: first.trial.clone(),
    })
}

pub(super) fn ensure_formation(serial: &str, cancelled: &AtomicBool) -> Result<(), String> {
    for _ in 0..6 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if common::get_match(
            &screen,
            &["battle/records.png", "battle/copy.png"],
            common::lower_screen(),
            0.8,
        )?
        .is_some()
        {
            return Ok(());
        }
        if let Some(battle) = common::find_template(
            &screen,
            "duras_trials/battle.png",
            common::lower_screen(),
            0.8,
        )? {
            common::tap_match(serial, battle, size)?;
            common::settle_cancellable(cancelled)?;
            continue;
        }
        if common::find_template(&screen, "battle/battle.png", common::lower_screen(), 0.8)?
            .is_some()
        {
            return Ok(());
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "Could not reopen the Dura formation screen. Sweeps stopped.",
        "Die Dura-Teamaufstellung konnte nicht wieder geöffnet werden. Es wird nicht gesweept.",
    )
    .into())
}

fn after_victory(serial: &str, cancelled: &AtomicBool) -> Result<(), String> {
    for _ in 0..12 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if common::find_template(
            &screen,
            "duras_trials/no_next.png",
            result_next_region(),
            0.85,
        )?
        .is_some()
        {
            adb::press_back(serial).map_err(|e| e.to_string())?;
        } else if let Some((_, next)) = common::get_match(
            &screen,
            &["next.png", "battle/next.png"],
            result_next_region(),
            0.78,
        )? {
            common::tap_match(serial, next, size)?;
        } else if common::get_match(
            &screen,
            &["duras_trials/battle.png", "duras_trials/sweep.png"],
            common::lower_screen(),
            0.8,
        )?
        .is_some()
            && common::find_template(
                &screen,
                "duras_trials/socketed_charms_overview.png",
                overview_region(),
                0.8,
            )?
            .is_some()
        {
            return Ok(());
        } else if common::find_template(&screen, "battle/records.png", common::lower_screen(), 0.8)?
            .is_some()
        {
            // Next opens the following formation directly. Return to the
            // carousel so its selected floor and current daily cap are read
            // again before deciding between a free first clear and Sweep.
            adb::press_back(serial).map_err(|e| e.to_string())?;
        } else if common::get_match(
            &screen,
            &[
                "duras_trials/first_clear_bottom_half.png",
                "duras_trials/end_sunrise.png",
            ],
            common::lower_screen(),
            0.85,
        )?
        .is_some()
        {
            // At the daily limit the only green button is a paid Retry.
            // Return to the overview and independently verify the cap there.
            adb::press_back(serial).map_err(|e| e.to_string())?;
        } else if let Some((_, close)) = common::get_match(
            &screen,
            &["tap_to_close.png", "afk_stages/tap_to_close.png"],
            common::lower_screen(),
            0.8,
        )? {
            common::tap_match(serial, close, size)?;
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "Could not verify that the Dura battle ended. Sweeps stopped.",
        "Dura-Ergebnis nicht eindeutig abgeschlossen; kein Sweep.",
    )
    .into())
}

fn select_next_stage(serial: &str, stage: u32, cancelled: &AtomicBool) -> Result<(), String> {
    for _ in 0..8 {
        let frame = read_frame(serial, cancelled)?;
        if frame.snapshot.stage == Some(stage) {
            return Ok(());
        }
        let numbers: Vec<_> = frame
            .lines
            .iter()
            .filter_map(|line| {
                let (_, y) = line.center();
                if (1140..=1290).contains(&y) && line.height >= 40.0 {
                    exact_number(&line.text).map(|number| (number, line.center()))
                } else {
                    None
                }
            })
            .collect();
        let candidates: Vec<_> = numbers
            .iter()
            .filter(|(number, _)| *number == stage)
            .collect();
        if candidates.len() == 1 {
            let (x, y) = candidates[0].1;
            adb::tap_game_point(serial, x, y, frame.size).map_err(|e| e.to_string())?;
        } else {
            let backwards = numbers
                .iter()
                .map(|(n, _)| *n)
                .min()
                .is_some_and(|n| n > stage);
            let (start, end) = if backwards {
                ((280, 1260), (900, 1260))
            } else {
                ((900, 1260), (280, 1260))
            };
            adb::swipe_game_points(serial, start, end, frame.size, 900)
                .map_err(|e| e.to_string())?;
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "Could not verify the selection of the next Dura stage. Sweeps stopped.",
        "Die nächste Dura-Stufe konnte nicht eindeutig ausgewählt werden; kein Sweep.",
    )
    .into())
}

trait SweepDriver {
    fn observe(&mut self) -> Result<Snapshot, String>;
    fn settle(&mut self) -> Result<(), String>;
    fn sweep(&mut self, permit: &CapPermit, quantity: u32) -> Result<u32, String>;
    fn report(&mut self, count: u32);
}

const MAX_SWEEP_BATCH: u32 = 6;

fn daily_sweeps(driver: &mut impl SweepDriver, budget: u32) -> Result<u32, String> {
    let mut total = 0;
    for _ in 0..100 {
        if total >= budget {
            return Ok(total);
        }
        let first = driver.observe()?;
        driver.settle()?;
        let second = driver.observe()?;
        let permit = confirm_cap(&first, &second)?;
        if permit.daily_remaining == 0 {
            return Ok(total);
        }
        let quantity = permit
            .daily_remaining
            .min(MAX_SWEEP_BATCH)
            .min(budget - total);
        let remaining = driver.sweep(&permit, quantity)?;
        if remaining.checked_add(quantity) != Some(permit.daily_remaining) {
            return Err(text(
                "Could not verify the attempt spent on the sweep. Sweeps stopped.",
                "Sweep-Verbrauch nicht eindeutig bestätigt; kein weiterer Sweep.",
            )
            .into());
        }
        total += quantity;
        driver.report(quantity);
    }
    Err(text(
        "An unexpected number of daily Dura attempts was detected. Sweeps stopped.",
        "Unerwartet viele tägliche Dura-Versuche; Sweep beendet.",
    )
    .into())
}

struct AdbSweep<'a, P> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    progress: &'a mut P,
}

#[derive(Debug, Clone)]
struct SweepDialog {
    trial: String,
    stage: u32,
    daily_remaining: u32,
    quantity: Option<u32>,
    daily_token: bool,
    cost: Option<SweepCost>,
    orange: bool,
}

fn validate_sweep_dialog(
    dialog: &SweepDialog,
    permit: &CapPermit,
    quantity: u32,
) -> Result<(), String> {
    #[cfg(test)]
    println!("Dura sweep dialog: {dialog:?}");
    if trial_key(&dialog.trial) != trial_key(&permit.trial)
        || dialog.stage != permit.stage
        || dialog.daily_remaining != permit.daily_remaining
        || dialog.quantity != Some(quantity)
        || !dialog.daily_token
        || !dialog.orange
        || dialog.cost
            != Some(SweepCost {
                available: permit.daily_remaining,
                cost: quantity,
            })
    {
        return Err(text(
            "Could not verify the stage, quantity, or daily cost in the sweep dialog.",
            "Sweep-Dialog: Stufe, Menge oder Tageskosten nicht bestätigt; kein Verbrauch.",
        )
        .into());
    }
    Ok(())
}

fn sweep_quantity_one(gray: &image::GrayImage) -> Result<bool, String> {
    // With one daily token left, the same quantity 1 is at the RIGHT end of
    // the slider. Inspect the complete track, without moving it.
    Ok(common::find_template(
        gray,
        "duras_trials/sweep_quantity_one.png",
        Rect {
            x: 175,
            y: 1570,
            width: 640,
            height: 80,
        },
        0.98,
    )?
    .is_some())
}

fn sweep_quantity(frame: &Frame) -> Result<Option<u32>, String> {
    if sweep_quantity_one(&frame.gray)? {
        return Ok(Some(1));
    }
    // The green slider thumb moves with the quantity. Its upper edge is
    // separate from the green progress bar, so only the numeral is read.
    let green: Vec<_> = (175..815)
        .filter(|&x| {
            let [r, g, b] = frame.color.get_pixel(x, 1575).0;
            g > 70 && u16::from(g) > u16::from(r) + 18 && u16::from(g) > u16::from(b) + 12
        })
        .collect();
    let (Some(&left), Some(&right)) = (green.first(), green.last()) else {
        return Ok(None);
    };
    if green.len() < 50 || right - left > 100 {
        return Ok(None);
    }
    let center = (left + right) / 2;
    let digit = image::GrayImage::from_fn(46, 39, |x, y| {
        let value = frame.gray.get_pixel(center - 23 + x, 1585 + y).0[0];
        image::Luma([if value < 145 { 255 } else { 0 }])
    });
    Ok(unique_value(
        repeated_numeric_patch(
            &digit,
            Rect {
                x: 0,
                y: 0,
                width: 46,
                height: 39,
            },
        )?
        .iter()
        .filter_map(|line| repeated_counter(&line.text))
        .filter(|number| (1..=MAX_SWEEP_BATCH).contains(number)),
    ))
}

fn find_sweep_plus(gray: &image::GrayImage) -> Result<Option<common::Match>, String> {
    common::find_template(
        gray,
        "duras_trials/sweep_plus.png",
        Rect {
            x: 830,
            y: 1560,
            width: 90,
            height: 90,
        },
        0.95,
    )
}

fn read_sweep_dialog(frame: &Frame) -> Result<Option<(SweepDialog, common::Match)>, String> {
    let heading = frame
        .lines
        .iter()
        .filter_map(|line| {
            let (_, y) = line.center();
            if !(950..=1030).contains(&y) {
                return None;
            }
            let compact = line.text.to_lowercase().replace(' ', "");
            let (trial, floor) = compact.split_once("-floor")?;
            Some((trial.to_owned(), exact_number(floor)?))
        })
        .collect::<Vec<_>>();
    if heading.len() != 1
        || !frame
            .lines
            .iter()
            .any(|l| l.y > 1100.0 && l.y < 1230.0 && l.text.eq_ignore_ascii_case("Rewards"))
        || !frame.lines.iter().any(|l| {
            l.y > 1430.0
                && l.y < 1515.0
                && l.text
                    .replace('+', "")
                    .trim()
                    .eq_ignore_ascii_case("Sweep Attempts")
        })
    {
        return Ok(None);
    }
    let Some(button) = common::find_template(
        &frame.gray,
        "duras_trials/sweep.png",
        Rect {
            x: 216,
            y: 1710,
            width: 864,
            height: 180,
        },
        0.9,
    )?
    else {
        return Ok(None);
    };
    let quantity = sweep_quantity(frame)?;
    let daily_token = common::find_template(
        &frame.gray,
        "duras_trials/sweep_daily_token.png",
        Rect {
            x: 570,
            y: 1800,
            width: 70,
            height: 60,
        },
        0.88,
    )?
    .is_some();
    // Price is the total cost for the selected amount. Read this separately;
    // Windows OCR can omit the small token fraction from a full screenshot.
    let crop = image::imageops::crop_imm(&frame.gray, 635, 1810, 65, 50).to_image();
    let mut padded = image::GrayImage::from_pixel(105, 90, image::Luma([255]));
    image::imageops::replace(&mut padded, &crop, 20, 20);
    let crop = image::imageops::resize(&padded, 420, 360, image::imageops::FilterType::CatmullRom);
    let price_lines = super::ocr::recognize(&crop)?;
    #[cfg(test)]
    {
        let _ = crop.save(".update-test/dura-cost-native.png");
        println!("Dura price OCR: {price_lines:?}");
    }
    let costs = price_lines
        .iter()
        .filter_map(|line| {
            let text = line
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            let text = text.trim_matches(|c: char| !c.is_ascii_digit() && c != '/');
            let (available, cost) = text.split_once('/')?;
            Some(SweepCost {
                available: exact_number(available)?,
                cost: exact_number(cost)?,
            })
        })
        .collect::<Vec<_>>();
    let crop_cost = if costs.len() == 1 {
        Some(costs[0])
    } else {
        None
    };
    let full_costs = frame
        .lines
        .iter()
        .filter_map(|line| {
            let (x, y) = line.center();
            if (550..=800).contains(&x) && (1800..=1870).contains(&y) {
                text_sweep_cost(&line.text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let full_cost = if full_costs.len() == 1 {
        Some(full_costs[0])
    } else {
        None
    };
    let mut cost = match (crop_cost, full_cost) {
        (Some(a), Some(b)) if a != b => None,
        (a, b) => a.or(b),
    };
    if crop_cost.is_none() && full_cost.is_none() {
        let costs = repeated_numeric_patch(
            &frame.gray,
            Rect {
                x: 640,
                y: 1816,
                width: 46,
                height: 31,
            },
        )?
        .iter()
        .filter_map(|line| repeated_price(&line.text))
        .collect::<Vec<_>>();
        if costs.len() == 1 {
            cost = Some(costs[0]);
        }
    }
    let Some(daily_remaining) = frame.snapshot.daily_remaining else {
        return Ok(None);
    };
    Ok(Some((
        SweepDialog {
            trial: heading[0].0.clone(),
            stage: heading[0].1,
            daily_remaining,
            quantity,
            daily_token,
            cost,
            orange: enabled_sweep_color(&frame.color, button.rect()),
        },
        button,
    )))
}

#[derive(Debug, PartialEq, Eq)]
enum SweepCompletion {
    TapClose(u32, u32),
    Back,
}

fn sweep_completion(lines: &[super::ocr::TextLine]) -> Option<SweepCompletion> {
    let letters = |value: &str| -> String {
        value
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphabetic())
            .collect()
    };
    // The title sits near the middle for one reward and at the top for a batch.
    if !lines
        .iter()
        .any(|line| line.y < 900.0 && line.width >= 540.0 && letters(&line.text) == "sweepcomplete")
    {
        return None;
    }
    let close = lines.iter().find(|line| {
        let (x, y) = line.center();
        (320..=760).contains(&x)
            && (1650..=1910).contains(&y)
            && matches!(
                letters(&line.text).as_str(),
                "taptoclose" | "tapanywheretoclose" | "tippenzumschließen"
            )
    });
    Some(match close {
        Some(line) => {
            let (x, y) = line.center();
            SweepCompletion::TapClose(x, y)
        }
        None => SweepCompletion::Back,
    })
}

fn daily_count_after_sweep(snapshot: &Snapshot, permit: &CapPermit) -> Option<u32> {
    if snapshot.overview
        && snapshot.sweep
        && !snapshot.battle
        && !snapshot.blocked
        && snapshot.stage == Some(permit.stage)
        && snapshot.limit == Some(permit.stage)
        && snapshot.trial == permit.trial
    {
        return snapshot.daily_remaining;
    }
    None
}

fn wait_for_sweep_result(
    serial: &str,
    permit: &CapPermit,
    cancelled: &AtomicBool,
) -> Result<u32, String> {
    for _ in 0..12 {
        let frame = read_frame(serial, cancelled)?;
        match sweep_completion(&frame.lines) {
            Some(SweepCompletion::TapClose(x, y)) => {
                common::ensure_not_cancelled(cancelled)?;
                adb::tap_game_point(serial, x, y, frame.size).map_err(|e| e.to_string())?;
            }
            Some(SweepCompletion::Back) => {
                common::ensure_not_cancelled(cancelled)?;
                adb::press_back(serial).map_err(|e| e.to_string())?;
            }
            None => {
                if let Some(remaining) = daily_count_after_sweep(&frame.snapshot, permit) {
                    if remaining < permit.daily_remaining {
                        return Ok(remaining);
                    }
                    if remaining > permit.daily_remaining {
                        return Err(text(
                            "The number of daily attempts changed. Sweeps stopped.",
                            "Tagesversuche haben sich geändert; kein weiterer Sweep.",
                        )
                        .into());
                    }
                }
            }
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "Could not verify the sweep cost. Sweeps stopped.",
        "Kein bestätigter Sweep-Verbrauch erkannt; kein weiterer Sweep.",
    )
    .into())
}

impl<P: FnMut(ActivityEvent)> SweepDriver for AdbSweep<'_, P> {
    fn observe(&mut self) -> Result<Snapshot, String> {
        Ok(read_frame(self.serial, self.cancelled)?.snapshot)
    }
    fn settle(&mut self) -> Result<(), String> {
        common::settle_cancellable(self.cancelled)
    }
    fn sweep(&mut self, permit: &CapPermit, quantity: u32) -> Result<u32, String> {
        // Re-read immediately before the resource-consuming tap. A permit never
        // survives a changed trial, cap, selected card, or daily token count.
        let frame = read_frame(self.serial, self.cancelled)?;
        let fresh = confirm_cap(&frame.snapshot, &frame.snapshot)?;
        if fresh.stage != permit.stage
            || fresh.trial != permit.trial
            || fresh.daily_remaining != permit.daily_remaining
        {
            return Err(text(
                "The Dura selection changed after the cap check. Sweeps stopped.",
                "Dura-Auswahl seit der Cap-Prüfung verändert; kein Sweep.",
            )
            .into());
        }
        let sweep = common::find_template(
            &frame.gray,
            "duras_trials/sweep.png",
            Rect {
                x: 216,
                y: 1520,
                width: 864,
                height: 240,
            },
            0.9,
        )?
        .ok_or(text("Sweep button not found", "Sweep-Schaltfläche fehlt"))?;
        common::ensure_not_cancelled(self.cancelled)?;
        common::tap_match(self.serial, sweep, frame.size)?;
        common::settle_cancellable(self.cancelled)?;
        for _ in 0..12 {
            let mut after = read_frame(self.serial, self.cancelled)?;
            if let Some(remaining) = after.snapshot.daily_remaining {
                if remaining != permit.daily_remaining {
                    return Err(text(
                        "The number of daily attempts changed. Sweeps stopped.",
                        "Tagesversuche haben sich geändert; kein weiterer Sweep.",
                    )
                    .into());
                }
            }
            if let Some((dialog, _)) = read_sweep_dialog(&after)? {
                validate_sweep_dialog(&dialog, permit, 1)?;
                for selected in 2..=quantity {
                    let plus = find_sweep_plus(&after.gray)?.ok_or(text(
                        "Could not verify the Sweep plus button.",
                        "Die Plus-Schaltfläche für Sweeps wurde nicht eindeutig erkannt.",
                    ))?;
                    common::ensure_not_cancelled(self.cancelled)?;
                    common::tap_match(self.serial, plus, after.size)?;
                    common::settle_cancellable(self.cancelled)?;
                    after = read_frame(self.serial, self.cancelled)?;
                    let (dialog, _) = read_sweep_dialog(&after)?.ok_or(text(
                        "The Sweep dialog changed while setting the quantity.",
                        "Der Sweep-Dialog hat sich beim Einstellen der Menge verändert.",
                    ))?;
                    validate_sweep_dialog(&dialog, permit, selected)?;
                }
                common::settle_cancellable(self.cancelled)?;
                let fresh = read_frame(self.serial, self.cancelled)?;
                let (dialog, button) = read_sweep_dialog(&fresh)?.ok_or(text(
                    "The sweep dialog changed before confirmation.",
                    "Sweep-Dialog verändert; kein Verbrauch.",
                ))?;
                validate_sweep_dialog(&dialog, permit, quantity)?;
                common::ensure_not_cancelled(self.cancelled)?;
                common::tap_match(self.serial, button, fresh.size)?;
                common::settle_cancellable(self.cancelled)?;
                return wait_for_sweep_result(self.serial, permit, self.cancelled);
            }
            // A new confirmation dialog is never blindly accepted. Its quantity
            // and token type must be verified before adding support for it.
            if after.lines.iter().any(|line| {
                line.y > 400.0
                    && line.y < 1400.0
                    && ["confirm", "bestätigen"].contains(&line.text.to_lowercase().as_str())
            }) {
                return Err(text(
                    "The sweep dialog was left open because its details could not be verified.",
                    "Sweep-Dialog noch nicht eindeutig verifiziert; keine Bestätigung ausgeführt.",
                )
                .into());
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Err(text(
            "The Sweep dialog could not be verified.",
            "Der Sweep-Dialog konnte nicht bestätigt werden.",
        )
        .into())
    }
    fn report(&mut self, count: u32) {
        (self.progress)(ActivityEvent::Swept(GameMode::DurasTrials, count));
    }
}

pub fn run(
    serial: &str,
    settings: BattleSettings,
    sweep_after_cap: bool,
    cancelled: &AtomicBool,
    log: impl FnMut(String),
    progress: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    run_with_budget(
        serial,
        settings,
        sweep_after_cap,
        u32::MAX,
        cancelled,
        log,
        progress,
    )
}

fn run_with_budget(
    serial: &str,
    settings: BattleSettings,
    sweep_after_cap: bool,
    budget: u32,
    cancelled: &AtomicBool,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    let mode = GameMode::DurasTrials;
    progress(ActivityEvent::ModeOpening(mode));
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    for _ in 0..3 {
        common::settle_cancellable(cancelled)?;
    }
    navigate(serial, cancelled, &mut log)?;
    progress(ActivityEvent::ModeEntered(mode));
    let mut cleared = 0u32;
    loop {
        let frame = read_frame(serial, cancelled)?;
        if !frame.snapshot.overview || frame.snapshot.blocked {
            return Err(text(
                "Could not verify the Dura stage screen. Sweeps stopped.",
                "Dura-Stufenansicht ist nicht eindeutig; kein Sweep.",
            )
            .into());
        }
        let (Some(stage), Some(limit)) = (frame.snapshot.stage, frame.snapshot.limit) else {
            return Err(text(
                "Could not read the selected Dura stage or the daily cap. Sweeps stopped.",
                "Ausgewählte Dura-Stufe oder Tageslimit nicht eindeutig lesbar; kein Sweep.",
            )
            .into());
        };
        if stage > limit {
            return Err(text(
                "The selected stage is above the current daily cap. Sweeps stopped.",
                "Ausgewählte Stufe liegt über dem aktuellen Tageslimit; kein Sweep.",
            )
            .into());
        }
        if frame.snapshot.sweep {
            if stage < limit {
                select_next_stage(serial, stage + 1, cancelled)?;
                continue;
            }
            if sweep_after_cap {
                let count = daily_sweeps(
                    &mut AdbSweep {
                        serial,
                        cancelled,
                        progress: &mut progress,
                    },
                    budget,
                )?;
                log(format_text!(
                    "Verified Dura cap at stage {stage}. Completed {count} daily sweeps.",
                    "Bestätigte Dura-Maximalstufe {stage}: {count} tägliche Sweeps abgeschlossen."
                ));
            }
            return Ok(cleared);
        }
        if !frame.snapshot.battle {
            return Err(text(
                "Could not identify the Dura battle action. Sweeps stopped.",
                "Keine eindeutige Dura-Kampfaktion; kein Sweep.",
            )
            .into());
        }
        ensure_formation(serial, cancelled)?;
        match common::play_one_stage(
            serial,
            &mut log,
            frame.size,
            cancelled,
            BattleSettings { mode, ..settings },
            &mut progress,
        )? {
            StageResult::Won => {
                cleared = cleared.saturating_add(1);
                after_victory(serial, cancelled)?;
            }
            StageResult::FormationFailed | StageResult::FinalStage => {
                log(text("Dura team attempts are exhausted. Sweeps require a verified stage cap.", "Dura-Teamversuche ausgeschöpft. Ohne bestätigte Maximalstufe wird nicht gesweept.").into());
                return Ok(cleared);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_rewards_are_closed_before_reading_the_same_trials_daily_counter() {
        use super::super::ocr::TextLine;
        let mut title = TextLine {
            text: "Sweep Complete".into(),
            x: 55.0,
            y: 53.0,
            width: 951.0,
            height: 138.0,
        };
        let close = TextLine {
            text: "• Tap to Close • —".into(),
            x: 412.0,
            y: 1797.0,
            width: 289.0,
            height: 38.0,
        };
        let (x, y) = close.center();
        assert_eq!(
            sweep_completion(&[title.clone(), close.clone()]),
            Some(SweepCompletion::TapClose(x, y))
        );
        assert_eq!(sweep_completion(&[close]), None);
        title.y = 594.0;
        title.text = "-Sweep Complete—".into();
        assert_eq!(sweep_completion(&[title]), Some(SweepCompletion::Back));

        let before = capped();
        let permit = confirm_cap(&before, &before).unwrap();
        let mut after = before;
        after.daily_remaining = Some(0);
        after.blocked = true;
        assert_eq!(daily_count_after_sweep(&after, &permit), None);
        after.blocked = false;
        assert_eq!(daily_count_after_sweep(&after, &permit), Some(0));
        after.trial = "raidtreasure".into();
        assert_eq!(daily_count_after_sweep(&after, &permit), None);
    }

    #[test]
    #[ignore = "closes an already-open Sweep Complete overlay and reads the daily counter, without starting a sweep"]
    fn connected_emulator_dura_existing_sweep_result() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let overview =
            std::env::var_os("ADBHELPER_TEST_DURA_OVERVIEW").expect("set the pre-Sweep capture");
        let quantity: u32 = std::env::var("ADBHELPER_TEST_DURA_QUANTITY")
            .expect("set the already-completed Sweep quantity")
            .parse()
            .unwrap();
        let before = decode_frame(&std::fs::read(overview).unwrap())
            .unwrap()
            .snapshot;
        let permit = confirm_cap(&before, &before).unwrap();
        let expected = permit.daily_remaining.checked_sub(quantity).unwrap();
        let cancelled = AtomicBool::new(false);
        let result = read_frame(&serial, &cancelled).unwrap();
        assert!(sweep_completion(&result.lines).is_some());
        let remaining = wait_for_sweep_result(&serial, &permit, &cancelled).unwrap();
        assert_eq!(remaining, expected);
        println!(
            "Verified Sweep consumption: {} -> {remaining}",
            permit.daily_remaining
        );
    }

    #[test]
    fn quantity_one_is_read_at_both_ends_of_the_track_but_not_on_other_controls() {
        let one = image::load_from_memory(include_bytes!(
            "../../assets/AFKJourney/duras_trials/sweep_quantity_one.png"
        ))
        .unwrap()
        .to_luma8();
        for x in [213, 737] {
            let mut screen = image::GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &one, x, 1580);
            assert!(sweep_quantity_one(&screen).unwrap());
        }
        let mut screen = image::GrayImage::new(1080, 1920);
        image::imageops::replace(&mut screen, &one, 640, 1800);
        assert!(!sweep_quantity_one(&screen).unwrap());
    }

    #[test]
    fn isolated_counter_requires_three_identical_numeric_readings() {
        for (text, count) in [
            ("000", 0),
            ("111", 1),
            ("555", 5),
            ("6 6 6", 6),
            ("101010", 10),
        ] {
            assert_eq!(repeated_counter(text), Some(count));
        }
        for text in ["5", "55", "5555", "556", "1/1", "I11", "S55", "5 6 5", ""] {
            assert_eq!(repeated_counter(text), None);
        }
    }

    #[test]
    fn token_icon_ocr_does_not_change_the_single_price_fraction() {
        for text in ["@ 4/1", "6) 4/1", "@4/1", "4/1"] {
            assert_eq!(
                text_sweep_cost(text),
                Some(SweepCost {
                    available: 4,
                    cost: 1
                })
            );
        }
        for text in ["4/1 6/1", "4 / 1", "4/I", "4/0", "6)4/1"] {
            assert_eq!(text_sweep_cost(text), None);
        }
        assert_eq!(
            repeated_price("3/13/13/1"),
            Some(SweepCost {
                available: 3,
                cost: 1
            })
        );
        assert_eq!(
            repeated_price("1/1 1/1 1/1"),
            Some(SweepCost {
                available: 1,
                cost: 1
            })
        );
        for text in ["3/13/14/1", "3/1", "", "3/13/I3/1"] {
            assert_eq!(repeated_price(text), None);
        }
    }

    #[test]
    fn sweep_dialog_requires_the_permitted_trial_floor_quantity_and_daily_price() {
        let permit = CapPermit {
            trial: "ale treasure".into(),
            stage: 25,
            daily_remaining: 6,
        };
        let valid = SweepDialog {
            trial: "aletreasure".into(),
            stage: 25,
            daily_remaining: 6,
            quantity: Some(6),
            daily_token: true,
            cost: Some(SweepCost {
                available: 6,
                cost: 6,
            }),
            orange: true,
        };
        assert!(validate_sweep_dialog(&valid, &permit, 6).is_ok());
        for change in 0..9 {
            let mut invalid = valid.clone();
            match change {
                0 => invalid.trial = "firetreasure".into(),
                1 => invalid.stage = 24,
                2 => invalid.daily_remaining = 5,
                3 => invalid.quantity = Some(5),
                4 => invalid.cost = None,
                5 => {
                    invalid.cost = Some(SweepCost {
                        available: 10,
                        cost: 6,
                    })
                }
                6 => {
                    invalid.cost = Some(SweepCost {
                        available: 6,
                        cost: 5,
                    })
                }
                7 => invalid.orange = false,
                _ => invalid.daily_token = false,
            }
            assert!(
                validate_sweep_dialog(&invalid, &permit, 6).is_err(),
                "{invalid:?}"
            );
        }
    }
    fn capped() -> Snapshot {
        Snapshot {
            trial: "gale treasure".into(),
            limit: Some(25),
            stage: Some(25),
            daily_remaining: Some(6),
            overview: true,
            sweep: true,
            sweep_enabled: true,
            sweep_cost: Some(SweepCost {
                available: 6,
                cost: 1,
            }),
            battle: false,
            blocked: false,
        }
    }

    #[test]
    fn raid_heading_matches_the_dialog_despite_its_decorative_initial() {
        let heading = super::super::ocr::TextLine {
            text: "R aid Treasure".into(),
            x: 50.0,
            y: 66.0,
            width: 569.0,
            height: 105.0,
        };
        let counter = super::super::ocr::TextLine {
            text: "@ 10".into(),
            x: 519.0,
            y: 27.0,
            width: 159.0,
            height: 58.0,
        };
        let mut snapshot = capped();
        snapshot.trial = overview_trial(&[counter, heading]);
        assert_eq!(snapshot.trial, "raidtreasure");
        let permit = confirm_cap(&snapshot, &snapshot).unwrap();
        let mut dialog = SweepDialog {
            trial: "aidtreasure".into(),
            stage: 25,
            daily_remaining: 6,
            quantity: Some(1),
            daily_token: true,
            cost: snapshot.sweep_cost,
            orange: true,
        };
        assert!(validate_sweep_dialog(&dialog, &permit, 1).is_ok());
        dialog.trial = "ale treasure".into();
        assert!(validate_sweep_dialog(&dialog, &permit, 1).is_err());
    }

    #[test]
    #[ignore = "validates supplied Dura overview and Sweep dialog captures with local Windows OCR"]
    fn captured_dura_sweep_validation() {
        let load = |name| {
            let path = std::env::var_os(name).expect("set the path to the Dura capture");
            decode_frame(&std::fs::read(path).unwrap()).unwrap()
        };
        let first = load("ADBHELPER_TEST_DURA_OVERVIEW");
        let second = load("ADBHELPER_TEST_DURA_SECOND_OVERVIEW");
        let permit = confirm_cap(&first.snapshot, &second.snapshot).unwrap();
        let quantity = std::env::var("ADBHELPER_TEST_DURA_QUANTITY")
            .map_or(1, |value| value.parse().expect("valid Sweep quantity"));
        let frame = load("ADBHELPER_TEST_DURA_DIALOG");
        let (dialog, _) = read_sweep_dialog(&frame)
            .unwrap()
            .expect("Sweep dialog detected");
        validate_sweep_dialog(&dialog, &permit, quantity).unwrap();
        if quantity < permit.daily_remaining.min(MAX_SWEEP_BATCH) {
            assert!(find_sweep_plus(&frame.gray).unwrap().is_some());
        }
    }

    struct FakeSweep {
        snapshot: Snapshot,
        actions: usize,
        reports: Vec<u32>,
        observe_count: usize,
        changed_second: bool,
        remaining_after: Option<u32>,
    }
    impl SweepDriver for FakeSweep {
        fn observe(&mut self) -> Result<Snapshot, String> {
            self.observe_count += 1;
            let mut snapshot = self.snapshot.clone();
            if self.changed_second && self.observe_count == 2 {
                snapshot.limit = Some(30);
            }
            Ok(snapshot)
        }
        fn settle(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn sweep(&mut self, permit: &CapPermit, quantity: u32) -> Result<u32, String> {
            self.actions += 1;
            let after = self
                .remaining_after
                .unwrap_or(permit.daily_remaining - quantity);
            self.snapshot.daily_remaining = Some(after);
            self.snapshot.sweep_cost = Some(SweepCost {
                available: after,
                cost: 1,
            });
            Ok(after)
        }
        fn report(&mut self, count: u32) {
            self.reports.push(count);
        }
    }
    fn sweeper(snapshot: Snapshot) -> FakeSweep {
        FakeSweep {
            snapshot,
            actions: 0,
            reports: vec![],
            observe_count: 0,
            changed_second: false,
            remaining_after: None,
        }
    }

    #[test]
    fn explicit_live_budget_cannot_consume_more_daily_tokens_than_authorized() {
        let mut driver = sweeper(capped());
        assert_eq!(daily_sweeps(&mut driver, 3).unwrap(), 3);
        assert_eq!(driver.actions, 1);
        assert_eq!(driver.reports, vec![3]);
        assert_eq!(driver.snapshot.daily_remaining, Some(3));
    }

    #[test]
    fn missing_price_extra_token_budget_and_non_daily_costs_block_sweep() {
        for change in 0..4 {
            let mut frame = capped();
            match change {
                0 => frame.sweep_cost = None,
                1 => {
                    frame.sweep_cost = Some(SweepCost {
                        available: 16,
                        cost: 1,
                    })
                }
                2 => {
                    frame.sweep_cost = Some(SweepCost {
                        available: 6,
                        cost: 2,
                    })
                }
                _ => frame.sweep_enabled = false,
            }
            let mut driver = sweeper(frame);
            assert!(daily_sweeps(&mut driver, u32::MAX).is_err());
            assert_eq!(driver.actions, 0);
        }
    }

    #[test]
    fn sweep_must_be_enabled_orange_not_gray_or_dimmed_under_an_overlay() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 40,
        };
        assert!(enabled_sweep_color(
            &image::RgbImage::from_pixel(100, 40, image::Rgb([235, 160, 30])),
            rect
        ));
        for color in [[130, 90, 20], [150, 150, 150], [40, 180, 60]] {
            assert!(!enabled_sweep_color(
                &image::RgbImage::from_pixel(100, 40, image::Rgb(color)),
                rect
            ));
        }
    }

    #[test]
    fn lower_cleared_floor_and_failed_battle_are_never_sweep_permission() {
        let mut lower = capped();
        lower.stage = Some(24);
        let mut failed = capped();
        failed.sweep = false;
        failed.battle = true;
        for frame in [lower, failed] {
            let mut driver = sweeper(frame);
            assert!(daily_sweeps(&mut driver, u32::MAX).is_err());
            assert_eq!(driver.actions, 0);
        }
    }

    #[test]
    fn ambiguous_missing_or_conflicting_cap_evidence_never_spends_tokens() {
        for change in 0..10 {
            let mut frame = capped();
            match change {
                0 => frame.limit = None,
                1 => frame.stage = None,
                2 => frame.daily_remaining = None,
                3 => frame.overview = false,
                4 => frame.sweep = false,
                5 => frame.battle = true,
                6 => frame.blocked = true,
                7 => frame.trial.clear(),
                8 => {
                    frame.limit = Some(1000);
                    frame.stage = Some(1000);
                }
                _ => frame.daily_remaining = Some(100),
            }
            let mut driver = sweeper(frame);
            assert!(
                daily_sweeps(&mut driver, u32::MAX).is_err(),
                "case {change}"
            );
            assert_eq!(driver.actions, 0);
        }
        let mut driver = sweeper(capped());
        driver.changed_second = true;
        assert!(daily_sweeps(&mut driver, u32::MAX).is_err());
        assert_eq!(driver.actions, 0);
    }

    #[test]
    fn permits_require_same_trial_same_cap_and_same_daily_budget_in_both_frames() {
        let first = capped();
        for change in 0..4 {
            let mut second = capped();
            match change {
                0 => second.trial = "fire treasure".into(),
                1 => second.limit = Some(30),
                2 => second.stage = Some(24),
                _ => second.daily_remaining = Some(5),
            }
            assert!(confirm_cap(&first, &second).is_err());
        }
        assert_eq!(confirm_cap(&first, &first).unwrap().stage, 25);
    }

    #[test]
    fn only_remaining_daily_tokens_are_consumed_and_zero_stops_without_any_tap() {
        let mut driver = sweeper(capped());
        assert_eq!(daily_sweeps(&mut driver, u32::MAX).unwrap(), 6);
        assert_eq!(driver.actions, 1);
        assert_eq!(driver.reports, vec![6]);
        let mut more_attempts = capped();
        more_attempts.daily_remaining = Some(7);
        more_attempts.sweep_cost = Some(SweepCost {
            available: 7,
            cost: 1,
        });
        let mut driver = sweeper(more_attempts);
        assert_eq!(daily_sweeps(&mut driver, u32::MAX).unwrap(), 7);
        assert_eq!(driver.reports, vec![6, 1]);
        let mut no_tokens = capped();
        no_tokens.daily_remaining = Some(0);
        let mut driver = sweeper(no_tokens);
        assert_eq!(daily_sweeps(&mut driver, u32::MAX).unwrap(), 0);
        assert_eq!(driver.actions, 0);
    }

    #[test]
    fn unconfirmed_consumption_never_causes_another_sweep() {
        for after in [4, 6, 7] {
            let mut driver = sweeper(capped());
            driver.remaining_after = Some(after);
            assert!(daily_sweeps(&mut driver, u32::MAX).is_err());
            assert_eq!(driver.actions, 1);
            assert!(driver.reports.is_empty());
        }
    }

    #[test]
    fn reached_status_requires_max_on_the_selected_card_and_a_readable_stage() {
        use super::super::ocr::TextLine;
        let card = Some(SelectedCard {
            left: 530,
            right: 1000,
        });
        let reached = TextLine {
            text: "Current Limit Reached".into(),
            x: 354.0,
            y: 1042.0,
            width: 371.0,
            height: 31.0,
        };
        let badge = TextLine {
            text: "Max".into(),
            x: 913.0,
            y: 1148.0,
            width: 61.0,
            height: 23.0,
        };
        let lines = vec![reached.clone(), badge.clone()];
        assert_eq!(reached_cap_stage(&lines, card, Some(25), false), Some(25));
        assert_eq!(reached_cap_stage(&lines, card, None, false), None);
        assert_eq!(reached_cap_stage(&lines, None, Some(25), false), None);
        assert_eq!(
            reached_cap_stage(&[reached.clone()], card, Some(25), false),
            None
        );
        assert_eq!(
            reached_cap_stage(&[badge.clone()], card, Some(25), false),
            None
        );
        assert_eq!(
            reached_cap_stage(&[reached.clone()], card, Some(25), true),
            Some(25)
        );
        assert_eq!(
            reached_cap_stage(&[badge.clone()], card, Some(25), true),
            None
        );
        let neighbour = TextLine { x: 410.0, ..badge };
        assert_eq!(
            reached_cap_stage(&[reached.clone(), neighbour], card, Some(25), false),
            None
        );
        let other_heading = TextLine {
            y: 400.0,
            ..reached
        };
        assert_eq!(
            reached_cap_stage(&[other_heading, lines[1].clone()], card, Some(25), false),
            None
        );
    }

    #[test]
    fn cap_parser_requires_the_explicit_daily_limit_at_its_own_position() {
        let line = |text: &str, y| super::super::ocr::TextLine {
            text: text.into(),
            x: 400.0,
            y,
            width: 250.0,
            height: 40.0,
        };
        assert_eq!(daily_limit(&[line("Daily Limit: 25", 1040.0)]), Some(25));
        assert_eq!(daily_limit(&[line("Tageslimit: 30", 1040.0)]), Some(30));
        assert_eq!(daily_limit(&[line("Daily Limit: 25", 500.0)]), None);
        for text in [
            "Cleared 25",
            "Sweep 25",
            "Daily Limit: 2S",
            "Daily Limit: 25/30",
            "Daily Limit: 0",
        ] {
            assert_eq!(daily_limit(&[line(text, 1040.0)]), None, "{text}");
        }
        assert_eq!(
            daily_limit(&[
                line("Daily Limit: 25", 1040.0),
                line("Daily Limit: 30", 1040.0)
            ]),
            None
        );
    }

    #[test]
    fn selected_stage_requires_one_unique_green_card_not_an_unselected_neighbour() {
        let mut image = image::RgbImage::new(1080, 1920);
        for x in [530, 1000] {
            for y in 1130..1520 {
                image.put_pixel(x, y, image::Rgb([50, 160, 30]));
            }
        }
        assert_eq!(
            selected_card(&image),
            Some(SelectedCard {
                left: 530,
                right: 1000
            })
        );
        for x in [20, 480] {
            for y in 1130..1520 {
                image.put_pixel(x, y, image::Rgb([50, 160, 30]));
            }
        }
        assert_eq!(selected_card(&image), None);
        assert_eq!(selected_card(&image::RgbImage::new(30, 30)), None);
    }

    #[test]
    fn sweep_resources_do_not_become_battle_attempts_or_cleared_stages() {
        let mode = GameMode::DurasTrials;
        let mut run = crate::activity::RunActivity::default();
        run.start(mode, std::time::Instant::now());
        run.apply(ActivityEvent::ModeEntered(mode));
        run.apply(ActivityEvent::Swept(mode, 6));
        assert_eq!(run.modes[&mode].sweeps, 6);
        assert_eq!(run.modes[&mode].attempts, 0);
        assert_eq!(run.modes[&mode].cleared, 0);
        assert_eq!(run.success_rate(), None);
        assert_eq!(run.attempts_per_clear(), None);
    }

    #[test]
    #[ignore = "pushes Dura's free first clears; sweeps only after verified daily cap"]
    fn connected_emulator_dura_push() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let budget: u32 = std::env::var("ADBHELPER_TEST_DURA_SWEEPS")
            .unwrap_or_else(|_| "0".into())
            .parse()
            .expect("valid sweep budget");
        assert!(budget <= 6, "live test may use at most six daily attempts");
        let mode = GameMode::DurasTrials;
        let result = run_with_budget(
            &serial,
            BattleSettings {
                mode,
                use_suggested_formations: true,
                current_team_first: true,
                skip_manual_formations: false,
                max_formations: 2,
                attempts_per_formation: 1,
            },
            true,
            budget,
            &AtomicBool::new(false),
            |m| println!("{m}"),
            |e| println!("{e:?}"),
        );
        std::fs::write(
            ".update-test/duras-after-push.png",
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
        println!("Dura action: {result:?}");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    #[ignore = "reads Dura's selected stage and daily cap; no input or sweep"]
    fn connected_emulator_dura_cap_read() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let frame = read_frame(&serial, &AtomicBool::new(false)).unwrap();
        println!("{:?}", frame.snapshot);
        std::fs::write(
            ".update-test/duras-cap-read.png",
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
    }
    #[test]
    #[ignore = "requires a connected emulator; navigation only, no battles or sweep"]
    fn connected_emulator_dura_navigation() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        navigate(&serial, &AtomicBool::new(false), &mut |m| println!("{m}")).unwrap();
        std::fs::write(
            ".update-test/duras-overview.png",
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
        println!("Dura's overview reached; no battle or sweep performed");
    }
}
