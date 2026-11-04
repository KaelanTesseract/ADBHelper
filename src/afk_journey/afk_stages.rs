use crate::activity::{ActivityEvent as ProgressEvent, BattleOutcome, GameMode};
use crate::adb;
use crate::afk_journey::{BASE_RESOLUTION, BATTLE_TIMEOUT_SECONDS};
use image::{imageops::FilterType, GrayImage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const MATCH_THRESHOLD: f32 = 0.78;
#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug)]
struct Match {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    confidence: f32,
}

impl Match {
    fn center(self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Next,
    VictoryRewards,
    Retry,
    PowerUp,
    AmbiguousResult,
    NetworkError,
    FinalStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StageResult {
    Won,
    FinalStage,
    FormationFailed,
}

#[derive(Clone, Copy, Debug)]
pub struct BattleSettings {
    pub mode: GameMode,
    pub use_suggested_formations: bool,
    pub current_team_first: bool,
    pub max_formations: u32,
    pub attempts_per_formation: u32,
}

#[derive(Clone, Copy)]
pub struct StageSelection {
    pub normal: bool,
    pub season: bool,
}

impl Default for StageSelection {
    fn default() -> Self {
        Self {
            normal: true,
            season: true,
        }
    }
}

impl StageSelection {
    pub fn modes(self) -> Vec<GameMode> {
        let mut modes = Vec::new();
        if self.season {
            modes.push(GameMode::SeasonAfkStages);
        }
        if self.normal {
            modes.push(GameMode::AfkStages);
        }
        modes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeEnd {
    AttemptsExhausted,
    FinalStage,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
pub struct ModeSummary {
    pub cleared: u32,
    pub end: ModeEnd,
}

fn run_sequence(
    selection: StageSelection,
    cancelled: &AtomicBool,
    mut execute: impl FnMut(GameMode) -> Result<ModeSummary, String>,
) -> Result<Vec<ModeSummary>, String> {
    let modes = selection.modes();
    if modes.is_empty() {
        return Err("Bitte mindestens eine Stage-Art auswählen.".into());
    }
    let mut summaries = Vec::new();
    for mode in modes {
        ensure_not_cancelled(cancelled)?;
        let summary = execute(mode)?;
        ensure_not_cancelled(cancelled)?;
        match summary.end {
            ModeEnd::AttemptsExhausted | ModeEnd::FinalStage | ModeEnd::Unavailable => {
                summaries.push(summary)
            }
        }
    }
    Ok(summaries)
}

pub fn run_selected(
    serial: &str,
    settings: BattleSettings,
    selection: StageSelection,
    cancelled: &AtomicBool,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(ProgressEvent),
) -> Result<u32, String> {
    let summaries = run_sequence(selection, cancelled, |mode| {
        progress(ProgressEvent::ModeOpening(mode));
        log(format!("{} öffnen.", mode.label()));
        run(
            serial,
            BattleSettings { mode, ..settings },
            cancelled,
            &mut log,
            &mut progress,
        )
    })?;
    Ok(summaries.iter().map(|s| s.cleared).sum())
}

fn embedded_template(relative_path: &str) -> Result<&'static [u8], String> {
    let bytes: &'static [u8] = match relative_path {
        "battle_modes/afk_stage.png" => {
            include_bytes!("../../assets/AFKJourney/battle_modes/afk_stage.png")
        }
        "navigation/time_of_day.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/time_of_day.png")
        }
        "navigation/resonating_hall_shortcut.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/resonating_hall_shortcut.png")
        }
        "navigation/homestead/homestead_enter.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/homestead/homestead_enter.png")
        }
        "navigation/homestead/homestead_invaded.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/homestead/homestead_invaded.png")
        }
        "navigation/homestead/world.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/homestead/world.png")
        }
        "navigation/notice.png" => include_bytes!("../../assets/AFKJourney/navigation/notice.png"),
        "navigation/x.png" => include_bytes!("../../assets/AFKJourney/navigation/x.png"),
        "navigation/resonating_hall_back.png" => {
            include_bytes!("../../assets/AFKJourney/navigation/resonating_hall_back.png")
        }
        "guide/close.png" => include_bytes!("../../assets/AFKJourney/guide/close.png"),
        "login/claim.png" => include_bytes!("../../assets/AFKJourney/login/claim.png"),
        "arcane_labyrinth/back_arrow.png" => {
            include_bytes!("../../assets/AFKJourney/arcane_labyrinth/back_arrow.png")
        }
        "battle/exit_door.png" => include_bytes!("../../assets/AFKJourney/battle/exit_door.png"),
        "battle/skip.png" => include_bytes!("../../assets/AFKJourney/battle/skip.png"),
        "battle/skip_orange.png" => {
            include_bytes!("../../assets/AFKJourney/battle/skip_orange.png")
        }
        "afk_stages/battle_large.png" => {
            &include_bytes!("../../assets/AFKJourney/afk_stages/battle_large.png")[..]
        }
        "afk_stages/talent_trials_large.png" => {
            include_bytes!("../../assets/AFKJourney/afk_stages/talent_trials_large.png")
        }
        "afk_stages/season_battle.png" => {
            &include_bytes!("../../assets/AFKJourney/afk_stages/season_battle.png")[..]
        }
        "afk_stages/tap_to_close.png" => {
            &include_bytes!("../../assets/AFKJourney/afk_stages/tap_to_close.png")[..]
        }
        "battle/battle.png" => &include_bytes!("../../assets/AFKJourney/battle/battle.png")[..],
        "battle/copy.png" => &include_bytes!("../../assets/AFKJourney/battle/copy.png")[..],
        "battle/formation_next.png" => {
            &include_bytes!("../../assets/AFKJourney/battle/formation_next.png")[..]
        }
        "battle/formations_icon.png" => {
            &include_bytes!("../../assets/AFKJourney/battle/formations_icon.png")[..]
        }
        "battle/gold.png" => &include_bytes!("../../assets/AFKJourney/battle/gold.png")[..],
        "battle/manual_battle.png" => {
            &include_bytes!("../../assets/AFKJourney/battle/manual_battle.png")[..]
        }
        "battle/manual_battle_old1.png" => {
            &include_bytes!("../../assets/AFKJourney/battle/manual_battle_old1.png")[..]
        }
        "battle/power_up.png" => &include_bytes!("../../assets/AFKJourney/battle/power_up.png")[..],
        "battle/records.png" => &include_bytes!("../../assets/AFKJourney/battle/records.png")[..],
        "battle/result.png" => &include_bytes!("../../assets/AFKJourney/battle/result.png")[..],
        "battle/spend.png" => &include_bytes!("../../assets/AFKJourney/battle/spend.png")[..],
        "battle/victory_rewards.png" => {
            &include_bytes!("../../assets/AFKJourney/battle/victory_rewards.png")[..]
        }
        "cancel.png" => &include_bytes!("../../assets/AFKJourney/cancel.png")[..],
        "confirm_text.png" => &include_bytes!("../../assets/AFKJourney/confirm_text.png")[..],
        "navigation/confirm.png" => {
            &include_bytes!("../../assets/AFKJourney/navigation/confirm.png")[..]
        }
        "next.png" => &include_bytes!("../../assets/AFKJourney/next.png")[..],
        "retry.png" => &include_bytes!("../../assets/AFKJourney/retry.png")[..],
        _ => {
            return Err(format!(
                "Template is not embedded in this build: {relative_path}"
            ))
        }
    };
    Ok(bytes)
}

fn load_template(relative_path: &str) -> Result<GrayImage, String> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, GrayImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(image) = cache
        .lock()
        .map_err(|_| "template cache is unavailable".to_owned())?
        .get(relative_path)
        .cloned()
    {
        return Ok(image);
    }

    let image = image::load_from_memory(embedded_template(relative_path)?)
        .map_err(|error| format!("Could not decode template {relative_path}: {error}"))?
        .to_luma8();
    cache
        .lock()
        .map_err(|_| "template cache is unavailable".to_owned())?
        .insert(relative_path.to_owned(), image.clone());
    Ok(image)
}

fn normalize_screenshot(bytes: &[u8]) -> Result<(GrayImage, (u32, u32)), String> {
    let screenshot = image::load_from_memory(bytes)
        .map_err(|error| format!("Could not decode device screenshot: {error}"))?
        .to_luma8();
    let actual_size = screenshot.dimensions();
    if actual_size.0 == 0 || actual_size.1 == 0 {
        return Err("Device returned an empty screenshot".to_owned());
    }
    let expected_ratio = BASE_RESOLUTION.0 as f32 / BASE_RESOLUTION.1 as f32;
    let actual_ratio = actual_size.0 as f32 / actual_size.1 as f32;
    if (actual_ratio - expected_ratio).abs() > 0.04 {
        return Err(format!(
            "AFK Journey automation expects portrait 9:16 display output; device screenshot is {} × {}. Rotate the emulator to portrait.",
            actual_size.0, actual_size.1
        ));
    }

    let normalized = if actual_size == BASE_RESOLUTION {
        screenshot
    } else {
        image::imageops::resize(
            &screenshot,
            BASE_RESOLUTION.0,
            BASE_RESOLUTION.1,
            FilterType::Lanczos3,
        )
    };
    Ok((normalized, actual_size))
}

#[derive(Clone, Copy)]
struct Sample {
    x: u32,
    y: u32,
    value: f64,
}

fn template_samples(template: &GrayImage, stride: u32) -> Vec<Sample> {
    let mut samples = Vec::new();
    for y in (0..template.height()).step_by(stride as usize) {
        for x in (0..template.width()).step_by(stride as usize) {
            samples.push(Sample {
                x,
                y,
                value: f64::from(template.get_pixel(x, y)[0]),
            });
        }
    }
    samples
}

fn normalized_correlation(image: &GrayImage, origin: (u32, u32), samples: &[Sample]) -> f32 {
    let count = samples.len() as f64;
    let (mut template_sum, mut template_square_sum) = (0.0, 0.0);
    let (mut image_sum, mut image_square_sum, mut product_sum) = (0.0, 0.0, 0.0);
    for sample in samples {
        let image_value = f64::from(image.get_pixel(origin.0 + sample.x, origin.1 + sample.y)[0]);
        template_sum += sample.value;
        template_square_sum += sample.value * sample.value;
        image_sum += image_value;
        image_square_sum += image_value * image_value;
        product_sum += sample.value * image_value;
    }
    let covariance = product_sum - template_sum * image_sum / count;
    let template_variance = template_square_sum - template_sum * template_sum / count;
    let image_variance = image_square_sum - image_sum * image_sum / count;
    let divisor = (template_variance * image_variance).sqrt();
    if divisor <= f64::EPSILON {
        -1.0
    } else {
        (covariance / divisor) as f32
    }
}

fn find_template(
    screenshot: &GrayImage,
    relative_path: &str,
    roi: Rect,
    threshold: f32,
) -> Result<Option<Match>, String> {
    let template = load_template(relative_path)?;
    let right = roi.x.saturating_add(roi.width).min(screenshot.width());
    let bottom = roi.y.saturating_add(roi.height).min(screenshot.height());
    if roi.x >= right
        || roi.y >= bottom
        || template.width() > right - roi.x
        || template.height() > bottom - roi.y
    {
        return Ok(None);
    }

    let coarse_samples = template_samples(&template, 4);
    // This narrow two-button marker has a high-contrast edge. A four-pixel
    // search grid can miss its true peak and choose the adjacent button edge.
    // Search every position for this marker; retain the fast grid elsewhere.
    let search_step = if relative_path == "afk_stages/season_battle.png" {
        1
    } else {
        4
    };
    let x_end = right - template.width();
    let y_end = bottom - template.height();
    let mut candidates = Vec::<(f32, u32, u32)>::new();
    for y in (roi.y..=y_end).step_by(search_step) {
        for x in (roi.x..=x_end).step_by(search_step) {
            let confidence = normalized_correlation(screenshot, (x, y), &coarse_samples);
            if candidates.len() < 5 || confidence > candidates[0].0 {
                candidates.push((confidence, x, y));
                candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
                if candidates.len() > 5 {
                    candidates.remove(0);
                }
            }
        }
    }

    let fine_samples = template_samples(&template, 2);
    let mut best = None::<(f32, u32, u32)>;
    for (_, coarse_x, coarse_y) in candidates {
        let start_x = coarse_x.saturating_sub(3).max(roi.x);
        let start_y = coarse_y.saturating_sub(3).max(roi.y);
        let end_x = (coarse_x + 3).min(x_end);
        let end_y = (coarse_y + 3).min(y_end);
        for y in start_y..=end_y {
            for x in start_x..=end_x {
                let confidence = normalized_correlation(screenshot, (x, y), &fine_samples);
                if best.is_none_or(|(best_confidence, _, _)| confidence > best_confidence) {
                    best = Some((confidence, x, y));
                }
            }
        }
    }

    Ok(best.and_then(|(confidence, x, y)| {
        (confidence >= threshold).then_some(Match {
            x,
            y,
            width: template.width(),
            height: template.height(),
            confidence,
        })
    }))
}

fn full_screen() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: BASE_RESOLUTION.0,
        height: BASE_RESOLUTION.1,
    }
}

fn lower_screen() -> Rect {
    Rect {
        x: 0,
        y: BASE_RESOLUTION.1 * 2 / 5,
        width: BASE_RESOLUTION.0,
        height: BASE_RESOLUTION.1 * 3 / 5,
    }
}

fn get_match(
    screenshot: &GrayImage,
    paths: &[&str],
    roi: Rect,
    threshold: f32,
) -> Result<Option<(String, Match)>, String> {
    let mut best = None::<(&str, Match)>;
    for path in paths {
        if let Some(found) = find_template(screenshot, path, roi, threshold)? {
            if best.is_none_or(|(_, current)| found.confidence > current.confidence) {
                best = Some((path, found));
            }
        }
    }
    Ok(best.map(|(path, found)| (path.to_owned(), found)))
}

fn screenshot(serial: &str) -> Result<(GrayImage, (u32, u32)), String> {
    let bytes = adb::screenshot(serial).map_err(|error| error.to_string())?;
    normalize_screenshot(&bytes)
}

fn tap_match(serial: &str, found: Match, size: (u32, u32)) -> Result<(), String> {
    let (x, y) = found.center();
    adb::tap_game_point(serial, x, y, size).map_err(|error| error.to_string())
}

fn wait_for_match(
    serial: &str,
    paths: &[&str],
    roi: Rect,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<(String, Match, (u32, u32)), String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        ensure_not_cancelled(cancelled)?;
        let (screen, size) = screenshot(serial)?;
        if let Some((path, found)) = get_match(&screen, paths, roi, MATCH_THRESHOLD)? {
            return Ok((path, found, size));
        }
        thread::sleep(Duration::from_millis(700));
    }
    Err(format!("Timed out waiting for {}.", paths.join(" or ")))
}

fn ensure_not_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("Cancelled by user.".to_owned())
    } else {
        Ok(())
    }
}

fn settle_cancellable(cancelled: &AtomicBool) -> Result<(), String> {
    for _ in 0..9 {
        ensure_not_cancelled(cancelled)?;
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn choose_suggested_formation(
    serial: &str,
    formation_number: u32,
    skip_manual: bool,
    only_manual: bool,
    log: &mut impl FnMut(String),
    cancelled: &AtomicBool,
) -> Result<bool, String> {
    let (mut screen, mut size) = screenshot(serial)?;
    let mut copy = find_template(&screen, "battle/copy.png", lower_screen(), 0.78)?;

    if copy.is_none() {
        let (path, records, records_size) = wait_for_match(
            serial,
            &["battle/records.png", "battle/battle.png"],
            lower_screen(),
            Duration::from_secs(20),
            cancelled,
        )?;
        if path != "battle/records.png" {
            log("Suggested formations are not available on this battle screen.".into());
            return Ok(false);
        }
        tap_match(serial, records, records_size)?;
        settle_cancellable(cancelled)?;
        let (_, found_copy, found_size) = wait_for_match(
            serial,
            &["battle/copy.png"],
            lower_screen(),
            Duration::from_secs(20),
            cancelled,
        )?;
        screen = screenshot(serial)?.0;
        size = found_size;
        copy = Some(found_copy);
    }

    if formation_number > 1 {
        for _ in 1..formation_number {
            let (current_screen, current_size) = screenshot(serial)?;
            let next = find_template(
                &current_screen,
                "battle/formation_next.png",
                Rect {
                    x: BASE_RESOLUTION.0 * 4 / 5,
                    y: BASE_RESOLUTION.1 / 2,
                    width: BASE_RESOLUTION.0 / 5,
                    height: BASE_RESOLUTION.1 / 3,
                },
                0.72,
            )?;
            let Some(next) = next else {
                log(format!(
                    "No more suggested formations after #{}.",
                    formation_number - 1
                ));
                return Ok(false);
            };
            tap_match(serial, next, current_size)?;
            settle_cancellable(cancelled)?;
            (screen, size) = screenshot(serial)?;
            copy = find_template(&screen, "battle/copy.png", lower_screen(), 0.78)?;
        }
    }

    let Some(copy) = copy else {
        log(format!(
            "Could not detect Copy for formation #{formation_number}."
        ));
        return Ok(false);
    };

    let manual = get_match(
        &screen,
        &["battle/manual_battle.png", "battle/manual_battle_old1.png"],
        full_screen(),
        0.74,
    )?
    .is_some();
    if (skip_manual && manual) || (only_manual && !manual) {
        log(format!(
            "Skipping {} formation #{formation_number}.",
            if manual { "manual" } else { "non-manual" }
        ));
        return Ok(false);
    }

    log(format!(
        "Copying suggested formation #{formation_number}{}.",
        if manual { " (manual)" } else { "" }
    ));
    tap_match(serial, copy, size)?;
    settle_cancellable(cancelled)?;

    let popup_roi = Rect {
        x: BASE_RESOLUTION.0 / 10,
        y: BASE_RESOLUTION.1 * 3 / 5,
        width: BASE_RESOLUTION.0 * 4 / 10,
        height: BASE_RESOLUTION.1 * 3 / 10,
    };
    let copy_deadline = Instant::now() + Duration::from_secs(8);
    let screen;
    let size;
    loop {
        ensure_not_cancelled(cancelled)?;
        let (current_screen, current_size) = screenshot(serial)?;
        if let Some(cancel) = find_template(&current_screen, "cancel.png", popup_roi, 0.72)? {
            log("That formation contains locked content; moving to the next suggestion.".into());
            tap_match(serial, cancel, current_size)?;
            settle_cancellable(cancelled)?;
            return Ok(false);
        }
        if find_template(&current_screen, "battle/copy.png", lower_screen(), 0.78)?.is_none() {
            screen = current_screen;
            size = current_size;
            break;
        }
        if Instant::now() >= copy_deadline {
            return Err("Copy button did not close after selecting the suggested team.".into());
        }
        thread::sleep(Duration::from_millis(400));
    }

    if let Some((_, confirm)) = get_match(
        &screen,
        &["navigation/confirm.png", "confirm_text.png"],
        full_screen(),
        0.76,
    )? {
        tap_match(serial, confirm, size)?;
        settle_cancellable(cancelled)?;
    }
    Ok(true)
}

fn start_battle(
    serial: &str,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
) -> Result<(), String> {
    ensure_not_cancelled(cancelled)?;
    let (path, _, _) = wait_for_match(
        serial,
        &[
            "battle/records.png",
            "battle/formations_icon.png",
            "battle/battle.png",
        ],
        lower_screen(),
        Duration::from_secs(20),
        cancelled,
    )?;
    log(format!(
        "Battle setup detected ({path}); starting with the current formation."
    ));
    adb::tap_game_point(serial, 850, 1780, size).map_err(|error| error.to_string())?;
    settle_cancellable(cancelled)?;

    let (screen, size) = screenshot(serial)?;
    if get_match(
        &screen,
        &["battle/spend.png", "battle/gold.png"],
        full_screen(),
        0.72,
    )?
    .is_some()
    {
        adb::press_back(serial).map_err(|error| error.to_string())?;
        return Err("Game requested currency. The Rust preview will not spend gold.".into());
    }

    if let Some((_, confirm)) = get_match(
        &screen,
        &["navigation/confirm.png", "confirm_text.png"],
        full_screen(),
        0.78,
    )? {
        tap_match(serial, confirm, size)?;
        settle_cancellable(cancelled)?;
    }
    Ok(())
}

fn wait_for_outcome(
    serial: &str,
    log: &mut impl FnMut(String),
    cancelled: &AtomicBool,
) -> Result<(Outcome, Match, (u32, u32)), String> {
    let paths = [
        ("next.png", Outcome::Next),
        ("battle/victory_rewards.png", Outcome::VictoryRewards),
        ("retry.png", Outcome::Retry),
        ("battle/power_up.png", Outcome::PowerUp),
        ("battle/result.png", Outcome::AmbiguousResult),
        ("navigation/confirm.png", Outcome::NetworkError),
        ("afk_stages/tap_to_close.png", Outcome::FinalStage),
    ];
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(BATTLE_TIMEOUT_SECONDS) {
        ensure_not_cancelled(cancelled)?;
        let (screen, size) = screenshot(serial)?;
        for (path, outcome) in paths {
            if let Some(found) = find_template(&screen, path, lower_screen(), MATCH_THRESHOLD)? {
                log(format!("Battle result detected: {path}."));
                return Ok((outcome, found, size));
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err("Battle result was not detected within four minutes.".to_owned())
}

fn battle_with_current_team(
    serial: &str,
    mode: GameMode,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
    attempts: u32,
    progress: &mut impl FnMut(ProgressEvent),
) -> Result<StageResult, String> {
    for attempt in 1..=attempts {
        ensure_not_cancelled(cancelled)?;
        log(format!("Starting battle attempt {attempt}/{attempts}."));
        start_battle(serial, log, size, cancelled)?;
        progress(ProgressEvent::BattleStarted(mode));
        let (outcome, found, size) = wait_for_outcome(serial, log, cancelled).map_err(|error| {
            progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
            error
        })?;
        match outcome {
            Outcome::Next => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                tap_match(serial, found, size)?;
                settle_cancellable(cancelled)?;
                return Ok(StageResult::Won);
            }
            Outcome::VictoryRewards => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                adb::tap_game_point(serial, 550, 1800, size).map_err(|error| error.to_string())?;
                settle_cancellable(cancelled)?;
                return Ok(StageResult::Won);
            }
            Outcome::Retry => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Lost));
                tap_match(serial, found, size)?;
                settle_cancellable(cancelled)?;
            }
            Outcome::PowerUp => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Lost));
                let (screen, screen_size) = screenshot(serial)?;
                if let Some(retry) = find_template(&screen, "retry.png", lower_screen(), 0.7)? {
                    tap_match(serial, retry, screen_size)?;
                    settle_cancellable(cancelled)?;
                    continue;
                }
                adb::press_back(serial).map_err(|error| error.to_string())?;
                log("This team did not clear the stage; trying another suggested team.".into());
                return Ok(StageResult::FormationFailed);
            }
            Outcome::AmbiguousResult => {
                let (screen, screen_size) = screenshot(serial)?;
                if let Some(retry) = find_template(&screen, "retry.png", lower_screen(), 0.7)? {
                    progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Lost));
                    tap_match(serial, retry, screen_size)?;
                    settle_cancellable(cancelled)?;
                    continue;
                }
                adb::tap_game_point(serial, 950, 1800, screen_size)
                    .map_err(|error| error.to_string())?;
                settle_cancellable(cancelled)?;
                let (_, next, next_size) = wait_for_match(
                    serial,
                    &["next.png"],
                    lower_screen(),
                    Duration::from_secs(5),
                    cancelled,
                )
                .map_err(|_| {
                    progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                    "Kampfergebnis unklar; es wird keine Stufe als geschafft gezählt.".to_owned()
                })?;
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                tap_match(serial, next, next_size)?;
                settle_cancellable(cancelled)?;
                return Ok(StageResult::Won);
            }
            Outcome::NetworkError => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                tap_match(serial, found, size)?;
                return Err("A confirmation popup appeared after battle. Check the game connection and screen.".into());
            }
            Outcome::FinalStage => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                return Ok(StageResult::FinalStage);
            }
        }
    }
    log("Reached the battle-attempt limit for this team.".into());
    Ok(StageResult::FormationFailed)
}

fn play_one_stage(
    serial: &str,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
    settings: BattleSettings,
    progress: &mut impl FnMut(ProgressEvent),
) -> Result<StageResult, String> {
    let (screen, _) = screenshot(serial)?;
    let copy_is_open = find_template(&screen, "battle/copy.png", lower_screen(), 0.78)?.is_some();

    if settings.current_team_first && !copy_is_open {
        log("Trying the currently selected team first.".into());
        match battle_with_current_team(
            serial,
            settings.mode,
            log,
            size,
            cancelled,
            settings.attempts_per_formation,
            progress,
        )? {
            StageResult::Won => return Ok(StageResult::Won),
            StageResult::FinalStage => return Ok(StageResult::FinalStage),
            StageResult::FormationFailed => {}
        }
    }

    if !settings.use_suggested_formations {
        return Ok(StageResult::FormationFailed);
    }

    for (pass_index, (skip_manual, only_manual)) in
        [(true, false), (false, true)].into_iter().enumerate()
    {
        if pass_index == 1 {
            let (screen, _) = screenshot(serial)?;
            if find_template(&screen, "battle/copy.png", lower_screen(), 0.78)?.is_some() {
                adb::press_back(serial).map_err(|error| error.to_string())?;
                settle_cancellable(cancelled)?;
            }
            log("No automatic suggestion cleared the stage; trying manual formations last.".into());
        }
        for formation_number in 1..=settings.max_formations {
            ensure_not_cancelled(cancelled)?;
            let selected = choose_suggested_formation(
                serial,
                formation_number,
                skip_manual,
                only_manual,
                log,
                cancelled,
            )?;
            if !selected {
                continue;
            }
            match battle_with_current_team(
                serial,
                settings.mode,
                log,
                size,
                cancelled,
                settings.attempts_per_formation,
                progress,
            )? {
                StageResult::Won => return Ok(StageResult::Won),
                StageResult::FinalStage => return Ok(StageResult::FinalStage),
                StageResult::FormationFailed => continue,
            }
        }
    }
    Ok(StageResult::FormationFailed)
}

fn season_button_region() -> Rect {
    Rect {
        x: BASE_RESOLUTION.0 / 2,
        y: BASE_RESOLUTION.1 * 7 / 10,
        width: BASE_RESOLUTION.0 / 2,
        height: BASE_RESOLUTION.1 * 3 / 10,
    }
}

fn afk_stages_visible(screen: &GrayImage) -> Result<bool, String> {
    Ok(find_template(
        screen,
        "afk_stages/season_battle.png",
        season_button_region(),
        0.72,
    )?
    .is_some()
        || find_template(
            screen,
            "afk_stages/battle_large.png",
            Rect {
                x: BASE_RESOLUTION.0 * 3 / 10,
                y: BASE_RESOLUTION.1 / 2,
                width: BASE_RESOLUTION.0 * 4 / 10,
                height: BASE_RESOLUTION.1 / 2,
            },
            0.8,
        )?
        .is_some()
        || find_template(
            screen,
            "afk_stages/talent_trials_large.png",
            lower_screen(),
            0.8,
        )?
        .is_some())
}

fn stage_entry_point(screen: &GrayImage, mode: GameMode) -> Result<Option<(u32, u32)>, String> {
    let large = find_template(
        screen,
        "afk_stages/battle_large.png",
        Rect {
            x: BASE_RESOLUTION.0 * 3 / 10,
            y: BASE_RESOLUTION.1 / 2,
            width: BASE_RESOLUTION.0 * 4 / 10,
            height: BASE_RESOLUTION.1 / 2,
        },
        0.8,
    )?;
    match mode {
        GameMode::SeasonAfkStages if large.is_some() => {
            return Ok(None);
        }
        GameMode::AfkStages
            if find_template(
                screen,
                "afk_stages/talent_trials_large.png",
                Rect {
                    x: BASE_RESOLUTION.0 / 5,
                    y: BASE_RESOLUTION.1 / 2,
                    width: BASE_RESOLUTION.0 * 3 / 5,
                    height: BASE_RESOLUTION.1 / 2,
                },
                0.8,
            )?
            .is_some() =>
        {
            return Ok(None);
        }
        _ => {}
    }
    if let Some(button) = find_template(
        screen,
        "afk_stages/season_battle.png",
        season_button_region(),
        0.72,
    )? {
        return Ok(Some(match mode {
            GameMode::SeasonAfkStages => (215, button.center().1),
            GameMode::AfkStages => button.center(),
        }));
    }
    if mode == GameMode::AfkStages {
        if let Some(button) = large {
            return Ok(Some(button.center()));
        }
    }
    Err(format!(
        "Der Einstieg für {} wurde nicht erkannt.",
        mode.label()
    ))
}

fn navigate_to_afk_stages(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(GrayImage, (u32, u32)), String> {
    ensure_not_cancelled(cancelled)?;
    adb::launch_afk_journey(serial).map_err(|error| error.to_string())?;
    for _ in 0..3 {
        settle_cancellable(cancelled)?;
    }
    let started = Instant::now();
    let mut confirm_exit = false;
    let mut opened_afk = false;
    for attempt in 0..40 {
        ensure_not_cancelled(cancelled)?;
        if started.elapsed() > Duration::from_secs(240) {
            break;
        }
        // Recover once if Back cannot leave a particular submenu or battle.
        if attempt == 20 {
            log("Navigation wird nach einem Spielneustart erneut versucht.".into());
            adb::stop_afk_journey(serial).map_err(|error| error.to_string())?;
            adb::launch_afk_journey(serial).map_err(|error| error.to_string())?;
            confirm_exit = false;
            opened_afk = false;
            for _ in 0..5 {
                settle_cancellable(cancelled)?;
            }
        }
        let (screen, size) = screenshot(serial)?;
        if afk_stages_visible(&screen)? {
            if opened_afk {
                // The AFK entry can open the idle-rewards overlay before the stage selector.
                adb::tap_game_point(serial, 550, 1080, size).map_err(|error| error.to_string())?;
                settle_cancellable(cancelled)?;
                opened_afk = false;
                continue;
            }
            return Ok((screen, size));
        }
        if let Some((_, entry)) = get_match(
            &screen,
            &["battle_modes/afk_stage.png"],
            full_screen(),
            0.75,
        )? {
            tap_match(serial, entry, size)?;
            opened_afk = true;
        } else if get_match(
            &screen,
            &["battle/spend.png", "battle/gold.png"],
            full_screen(),
            0.78,
        )?
        .is_some()
        {
            adb::press_back(serial).map_err(|error| error.to_string())?;
            confirm_exit = false;
        } else if confirm_exit {
            if let Some((_, confirm)) = get_match(
                &screen,
                &["navigation/confirm.png", "confirm_text.png"],
                full_screen(),
                0.8,
            )? {
                tap_match(serial, confirm, size)?;
            } else {
                adb::press_back(serial).map_err(|error| error.to_string())?;
            }
            confirm_exit = false;
        } else if let Some((path, close)) = get_match(
            &screen,
            &[
                "cancel.png",
                "guide/close.png",
                "navigation/x.png",
                "login/claim.png",
                "arcane_labyrinth/back_arrow.png",
                "navigation/resonating_hall_back.png",
                "battle/exit_door.png",
                "battle/skip.png",
                "battle/skip_orange.png",
            ],
            full_screen(),
            0.8,
        )? {
            tap_match(serial, close, size)?;
            confirm_exit = path == "battle/exit_door.png";
        } else if get_match(
            &screen,
            &[
                "navigation/time_of_day.png",
                "navigation/resonating_hall_shortcut.png",
                "navigation/homestead/homestead_enter.png",
                "navigation/homestead/homestead_invaded.png",
                "navigation/homestead/world.png",
            ],
            full_screen(),
            0.8,
        )?
        .is_some()
        {
            // Shared bottom navigation in the world and homestead overview.
            adb::tap_game_point(serial, 460, 1830, size).map_err(|error| error.to_string())?;
        } else if find_template(&screen, "navigation/notice.png", full_screen(), 0.8)?.is_some() {
            adb::tap_game_point(serial, 540, 960, size).map_err(|error| error.to_string())?;
            for _ in 0..3 {
                settle_cancellable(cancelled)?;
            }
        } else {
            adb::press_back(serial).map_err(|error| error.to_string())?;
        }
        settle_cancellable(cancelled)?;
    }
    Err("Die AFK-Seite konnte nicht automatisch geöffnet werden. Bitte Spielverbindung und freigeschaltete Spielmodi prüfen.".into())
}

/// Navigates from the current game menu to the selected normal or Season AFK Stages.
///
/// Continues until the final stage is reached or no available team can clear
/// the next stage. The caller receives progress log messages and the final count.
pub fn run<F, P>(
    serial: &str,
    battle_settings: BattleSettings,
    cancelled: &AtomicBool,
    mut log: F,
    mut progress: P,
) -> Result<ModeSummary, String>
where
    F: FnMut(String),
    P: FnMut(ProgressEvent),
{
    let mode = battle_settings.mode;
    ensure_not_cancelled(cancelled)?;
    let display_size = adb::display_size(serial).map_err(|error| error.to_string())?;
    log(format!(
        "Connected display: {} × {}.",
        display_size.0, display_size.1
    ));

    let (screen, size) = navigate_to_afk_stages(serial, cancelled, &mut log)?;
    let Some((x, y)) = stage_entry_point(&screen, mode)? else {
        log(format!(
            "{} nicht verfügbar oder bereits abgeschlossen.",
            mode.label()
        ));
        return Ok(ModeSummary {
            cleared: 0,
            end: ModeEnd::Unavailable,
        });
    };
    adb::tap_game_point(serial, x, y, size).map_err(|error| error.to_string())?;
    log(format!("{} ausgewählt.", mode.label()));
    settle_cancellable(cancelled)?;

    let (screen, size) = screenshot(serial)?;
    if let Some((_, confirm)) = get_match(
        &screen,
        &["navigation/confirm.png", "confirm_text.png"],
        full_screen(),
        0.78,
    )? {
        tap_match(serial, confirm, size)?;
        settle_cancellable(cancelled)?;
    }

    progress(ProgressEvent::ModeEntered(mode));
    let mut cleared: u32 = 0;
    loop {
        ensure_not_cancelled(cancelled)?;
        log(format!(
            "{}: Stufe {} vorbereiten.",
            mode.label(),
            cleared + 1
        ));
        match play_one_stage(
            serial,
            &mut log,
            size,
            cancelled,
            battle_settings,
            &mut progress,
        ) {
            Ok(StageResult::Won) => {
                cleared = cleared.saturating_add(1);
                log(format!("{}: {cleared} Stufen geschafft.", mode.label()));
            }
            Ok(StageResult::FinalStage) => {
                log(format!("{}: Letzte Stufe erreicht.", mode.label()));
                return Ok(ModeSummary {
                    cleared,
                    end: ModeEnd::FinalStage,
                });
            }
            Ok(StageResult::FormationFailed) => {
                log(format!(
                    "Stopping after {cleared} cleared stage(s): no team cleared the next stage."
                ));
                return Ok(ModeSummary {
                    cleared,
                    end: ModeEnd::AttemptsExhausted,
                });
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_modes_always_run_season_first_and_skip_unselected_modes() {
        for (season, normal, expected) in [
            (
                true,
                true,
                vec![GameMode::SeasonAfkStages, GameMode::AfkStages],
            ),
            (true, false, vec![GameMode::SeasonAfkStages]),
            (false, true, vec![GameMode::AfkStages]),
            (false, false, vec![]),
        ] {
            let mut visited = Vec::new();
            let result = run_sequence(
                StageSelection { season, normal },
                &AtomicBool::new(false),
                |mode| {
                    visited.push(mode);
                    Ok(ModeSummary {
                        cleared: 0,
                        end: ModeEnd::AttemptsExhausted,
                    })
                },
            );
            assert_eq!(visited, expected);
            assert_eq!(result.is_ok(), season || normal);
        }
    }

    #[test]
    fn normal_stages_follow_exhaustion_even_when_season_clears_zero_stages() {
        for end in [
            ModeEnd::AttemptsExhausted,
            ModeEnd::FinalStage,
            ModeEnd::Unavailable,
        ] {
            let mut visited = Vec::new();
            let summaries =
                run_sequence(StageSelection::default(), &AtomicBool::new(false), |mode| {
                    visited.push(mode);
                    Ok(ModeSummary {
                        cleared: if mode == GameMode::AfkStages { 3 } else { 0 },
                        end,
                    })
                })
                .unwrap();
            assert_eq!(
                visited,
                vec![GameMode::SeasonAfkStages, GameMode::AfkStages]
            );
            assert_eq!(summaries.iter().map(|s| s.cleared).sum::<u32>(), 3);
        }
    }

    #[test]
    fn stop_during_season_prevents_starting_normal_stages() {
        let cancelled = AtomicBool::new(false);
        let mut visited = Vec::new();
        let result = run_sequence(StageSelection::default(), &cancelled, |mode| {
            visited.push(mode);
            cancelled.store(true, Ordering::Relaxed);
            Ok(ModeSummary {
                cleared: 1,
                end: ModeEnd::AttemptsExhausted,
            })
        });
        assert!(result.is_err());
        assert_eq!(visited, vec![GameMode::SeasonAfkStages]);
    }

    #[test]
    fn technical_failure_prevents_the_next_mode() {
        let mut visited = Vec::new();
        let result = run_sequence(StageSelection::default(), &AtomicBool::new(false), |mode| {
            visited.push(mode);
            Err("Verbindung unterbrochen".into())
        });
        assert_eq!(result.unwrap_err(), "Verbindung unterbrochen");
        assert_eq!(visited, vec![GameMode::SeasonAfkStages]);
    }

    #[test]
    fn already_cancelled_sequence_does_not_start_any_mode() {
        let result = run_sequence(StageSelection::default(), &AtomicBool::new(true), |_| {
            panic!("No mode may start after Stop");
        });
        assert!(result.is_err());
    }

    #[test]
    fn recognizes_season_entry_only_in_its_expected_screen_region() {
        let template = load_template("afk_stages/season_battle.png").unwrap();
        let mut screen = GrayImage::new(BASE_RESOLUTION.0, BASE_RESOLUTION.1);
        image::imageops::replace(&mut screen, &template, 20, 20);
        assert!(!afk_stages_visible(&screen).unwrap());
        image::imageops::replace(
            &mut screen,
            &template,
            i64::from(BASE_RESOLUTION.0 - template.width() - 16),
            i64::from(BASE_RESOLUTION.1 - template.height() - 100),
        );
        assert!(afk_stages_visible(&screen).unwrap());
    }

    #[test]
    fn recognizes_afk_screen_when_season_is_unavailable() {
        let template = load_template("afk_stages/battle_large.png").unwrap();
        let mut screen = GrayImage::new(BASE_RESOLUTION.0, BASE_RESOLUTION.1);
        image::imageops::replace(&mut screen, &template, 330, 1200);
        assert!(afk_stages_visible(&screen).unwrap());
        assert_eq!(
            stage_entry_point(&screen, GameMode::AfkStages)
                .unwrap()
                .unwrap(),
            (330 + template.width() / 2, 1200 + template.height() / 2)
        );
        assert!(stage_entry_point(&screen, GameMode::SeasonAfkStages)
            .unwrap()
            .is_none());
    }

    #[test]
    fn dual_entry_selects_season_on_left_and_normal_battle_on_right() {
        let template = load_template("afk_stages/season_battle.png").unwrap();
        let mut screen = GrayImage::new(BASE_RESOLUTION.0, BASE_RESOLUTION.1);
        let x = BASE_RESOLUTION.0 - template.width() - 16;
        let y = BASE_RESOLUTION.1 - template.height() - 100;
        image::imageops::replace(&mut screen, &template, i64::from(x), i64::from(y));
        assert_eq!(
            stage_entry_point(&screen, GameMode::SeasonAfkStages)
                .unwrap()
                .unwrap(),
            (215, y + template.height() / 2)
        );
        assert_eq!(
            stage_entry_point(&screen, GameMode::AfkStages)
                .unwrap()
                .unwrap(),
            (x + template.width() / 2, y + template.height() / 2)
        );
    }

    #[test]
    fn completed_normal_stages_are_not_mistaken_for_a_battle_entry() {
        let template = load_template("afk_stages/talent_trials_large.png").unwrap();
        let mut screen = GrayImage::new(BASE_RESOLUTION.0, BASE_RESOLUTION.1);
        image::imageops::replace(
            &mut screen,
            &template,
            i64::from((BASE_RESOLUTION.0 - template.width()) / 2),
            1200,
        );
        assert!(afk_stages_visible(&screen).unwrap());
        assert!(stage_entry_point(&screen, GameMode::AfkStages)
            .unwrap()
            .is_none());
    }

    #[test]
    fn cancellation_before_navigation_does_not_start_adb_or_report_progress() {
        for mode in [GameMode::SeasonAfkStages, GameMode::AfkStages] {
            let mut events = Vec::new();
            let result = run(
                "not-a-device",
                BattleSettings {
                    mode,
                    use_suggested_formations: true,
                    current_team_first: true,
                    max_formations: 10,
                    attempts_per_formation: 5,
                },
                &AtomicBool::new(true),
                |_| {},
                |event| events.push(event),
            );
            assert_eq!(result.unwrap_err(), "Cancelled by user.");
            assert!(events.is_empty());
        }
    }
}
