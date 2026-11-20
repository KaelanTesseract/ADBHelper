use super::vision::*;
use crate::activity::{ActivityEvent as ProgressEvent, BattleOutcome, GameMode};
use crate::adb;
use crate::afk_journey::error::ActionError;
use crate::afk_journey::{BASE_RESOLUTION, BATTLE_TIMEOUT_SECONDS};
use crate::localization::tr;
use image::GrayImage;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Next,
    VictoryRewards,
    Retry,
    PowerUp,
    AmbiguousResult,
    NetworkError,
    FinalStage,
    DuraNoNext,
    DuraVictory,
    UnionFloorReturned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StageResult {
    Won,
    FinalStage,
    FormationFailed,
}

#[derive(Clone, Copy, Debug)]
pub struct BattleSettings {
    pub mode: GameMode,
    pub use_suggested_formations: bool,
    pub current_team_first: bool,
    pub skip_manual_formations: bool,
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
    mut execute: impl FnMut(GameMode) -> Result<ModeSummary, ActionError>,
) -> Result<Vec<ModeSummary>, ActionError> {
    let modes = selection.modes();
    if modes.is_empty() {
        return Err(tr!("afk-stages-select-at-least-one-stage-type").into());
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
) -> Result<u32, ActionError> {
    let summaries = run_sequence(selection, cancelled, |mode| {
        progress(ProgressEvent::ModeOpening(mode));
        log(tr!("afk-stages-opening-label", label = mode.label()));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FormationPass {
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TeamSlot {
    Current,
    Ranked(FormationPass, u32),
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TeamCursor {
    slot: TeamSlot,
    used: u32,
}

impl TeamCursor {
    fn can_resume(self, settings: BattleSettings) -> bool {
        if self.used < settings.attempts_per_formation {
            return true;
        }
        if !settings.use_suggested_formations || settings.max_formations == 0 {
            return false;
        }
        match self.slot {
            TeamSlot::Current => true,
            TeamSlot::Ranked(pass, number) => {
                number < settings.max_formations
                    || (pass == FormationPass::Automatic && !settings.skip_manual_formations)
            }
            TeamSlot::Fallback => false,
        }
    }

    fn first_rank(self, pass: FormationPass, attempts: u32) -> u32 {
        match self.slot {
            TeamSlot::Ranked(saved_pass, number) if saved_pass == pass => {
                number + u32::from(self.used >= attempts)
            }
            _ => 1,
        }
    }
}

fn formation_passes(skip_manual: bool) -> &'static [FormationPass] {
    if skip_manual {
        &[FormationPass::Automatic]
    } else {
        &[FormationPass::Automatic, FormationPass::Manual]
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FormationChoice {
    Copied,
    CurrentTeam,
    Exhausted,
}

trait FormationBrowser {
    // Opening Records from battle resets the displayed index to one.
    fn open(&mut self) -> Result<bool, ActionError>;
    fn next(&mut self) -> Result<bool, ActionError>;
    fn is_manual(&mut self) -> Result<bool, ActionError>;
    fn copy(&mut self) -> Result<bool, ActionError>;
}

fn copy_next_formation(
    browser: &mut impl FormationBrowser,
    target: &mut u32,
    limit: u32,
    pass: FormationPass,
) -> Result<FormationChoice, ActionError> {
    if !browser.open()? {
        return Ok(FormationChoice::CurrentTeam);
    }
    let mut displayed = 1;
    while *target <= limit {
        // Keep the displayed index while skipping. Do not count from one again
        // unless Records was actually closed and reopened (as in the original).
        while displayed < *target {
            if !browser.next()? {
                return Ok(FormationChoice::Exhausted);
            }
            displayed += 1;
        }
        let manual = browser.is_manual()?;
        let eligible = manual == (pass == FormationPass::Manual);
        if eligible && browser.copy()? {
            return Ok(FormationChoice::Copied);
        }
        *target += 1;
    }
    Ok(FormationChoice::Exhausted)
}

fn ensure_battle_screen(
    serial: &str,
    mode: GameMode,
    cancelled: &AtomicBool,
) -> Result<(), ActionError> {
    for _ in 0..4 {
        ensure_not_cancelled(cancelled)?;
        let (screen, size) = screenshot(serial)?;
        if mode == GameMode::UnionCampaign
            && super::union_campaign::floor_battle_button(&screen)?.is_some()
        {
            return super::union_campaign::prepare_floor(serial, cancelled);
        }
        if let Some(retry) = find_template(&screen, "retry.png", lower_screen(), 0.7)? {
            tap_match(serial, retry, size)?;
        } else if find_template(&screen, "battle/copy.png", lower_screen(), 0.75)?.is_some() {
            adb::press_back(serial).map_err(|error| error.to_string())?;
        } else if get_match(
            &screen,
            &["battle/records.png", "battle/battle.png"],
            lower_screen(),
            0.8,
        )?
        .is_some()
        {
            return Ok(());
        } else {
            // The battle setup may still be animating after Retry/Back.
            if poll_for_match(
                serial,
                &["battle/records.png", "battle/battle.png"],
                lower_screen(),
                crate::timing::secs(5),
                cancelled,
            )?
            .is_some()
            {
                return Ok(());
            }
            adb::press_back(serial).map_err(|error| error.to_string())?;
        }
        settle_cancellable(cancelled)?;
    }
    wait_for_match(
        serial,
        &["battle/records.png", "battle/battle.png"],
        lower_screen(),
        crate::timing::secs(20),
        cancelled,
    )?;
    Ok(())
}

fn manual_formation_visible(screen: &GrayImage) -> Result<bool, ActionError> {
    Ok(get_match(
        screen,
        &["battle/manual_battle.png", "battle/manual_battle_old1.png"],
        Rect {
            x: 0,
            y: 960,
            width: 540,
            height: 960,
        },
        0.8,
    )?
    .is_some())
}

struct AdbFormationBrowser<'a, L> {
    serial: &'a str,
    mode: GameMode,
    cancelled: &'a AtomicBool,
    log: &'a mut L,
}

impl<L: FnMut(String)> FormationBrowser for AdbFormationBrowser<'_, L> {
    fn open(&mut self) -> Result<bool, ActionError> {
        ensure_battle_screen(self.serial, self.mode, self.cancelled)?;
        // The original retries Records while its button remains visible.
        for _ in 0..3 {
            let (screen, size) = screenshot(self.serial)?;
            if find_template(&screen, "battle/copy.png", lower_screen(), 0.75)?.is_some() {
                settle_cancellable(self.cancelled)?;
                return Ok(true);
            }
            if let Some(records) =
                find_template(&screen, "battle/records.png", lower_screen(), 0.8)?
            {
                tap_match(self.serial, records, size)?;
                if poll_for_match(
                    self.serial,
                    &["battle/copy.png"],
                    lower_screen(),
                    crate::timing::secs(5),
                    self.cancelled,
                )?
                .is_some()
                {
                    settle_cancellable(self.cancelled)?;
                    return Ok(true);
                }
            } else {
                (self.log)(tr!("afk-stages-no-leaderboard-teams-are-available-using").into());
                return Ok(false);
            }
        }
        (self.log)(tr!("afk-stages-no-more-leaderboard-teams-are-available").into());
        Ok(false)
    }

    fn next(&mut self) -> Result<bool, ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        let (before, size) = screenshot(self.serial)?;
        let Some(next) = find_template(
            &before,
            "battle/formation_next.png",
            Rect {
                x: BASE_RESOLUTION.0 * 4 / 5,
                y: BASE_RESOLUTION.1 / 2,
                width: BASE_RESOLUTION.0 / 5,
                height: BASE_RESOLUTION.1 / 10,
            },
            0.78,
        )?
        else {
            return Ok(false);
        };
        tap_match(self.serial, next, size)?;
        settle_cancellable(self.cancelled)?;
        // Do not advance the index until the central formation actually changes.
        let roi = Rect {
            x: 216,
            y: 288,
            width: 648,
            height: 1248,
        };
        let previous =
            image::imageops::crop_imm(&before, roi.x, roi.y, roi.width, roi.height).to_image();
        let samples = template_samples(&previous, 6);
        let started = Instant::now();
        while started.elapsed() < crate::timing::secs(5) {
            ensure_not_cancelled(self.cancelled)?;
            let (screen, _) = screenshot(self.serial)?;
            if normalized_correlation(&screen, (roi.x, roi.y), &samples) < 0.95 {
                settle_cancellable(self.cancelled)?;
                return Ok(true);
            }
            thread::sleep(crate::timing::millis(300));
        }
        Ok(false)
    }

    fn is_manual(&mut self) -> Result<bool, ActionError> {
        // Original: poll the lower-left manual label for 1.5 seconds at 80%.
        let started = Instant::now();
        while started.elapsed() < crate::timing::millis(1500) {
            ensure_not_cancelled(self.cancelled)?;
            if manual_formation_visible(&screenshot(self.serial)?.0)? {
                return Ok(true);
            }
            thread::sleep(crate::timing::millis(300));
        }
        ensure_not_cancelled(self.cancelled)?;
        Ok(false)
    }

    fn copy(&mut self) -> Result<bool, ActionError> {
        let popup_roi = Rect {
            x: 108,
            y: 1152,
            width: 432,
            height: 576,
        };
        let deadline = Instant::now() + crate::timing::secs(12);
        loop {
            ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = screenshot(self.serial)?;
            if let Some(cancel) = find_template(&screen, "cancel.png", popup_roi, 0.72)? {
                (self.log)(tr!("afk-stages-skipping-a-team-with-locked-heroes").into());
                tap_match(self.serial, cancel, size)?;
                settle_cancellable(self.cancelled)?;
                return Ok(false);
            }
            if let Some(copy) = find_template(&screen, "battle/copy.png", lower_screen(), 0.75)? {
                if Instant::now() >= deadline {
                    return Err(tr!("afk-stages-could-not-copy-the-recommended-team").into());
                }
                tap_match(self.serial, copy, size)?;
                settle_cancellable(self.cancelled)?;
                continue;
            }
            if let Some((_, confirm)) = get_match(
                &screen,
                &["navigation/confirm.png", "confirm_text.png"],
                full_screen(),
                0.76,
            )? {
                tap_match(self.serial, confirm, size)?;
                settle_cancellable(self.cancelled)?;
            }
            return Ok(true);
        }
    }
}

fn start_battle(
    serial: &str,
    mode: GameMode,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
) -> Result<(), ActionError> {
    ensure_not_cancelled(cancelled)?;
    if mode == GameMode::UnionCampaign {
        super::union_campaign::prepare_floor(serial, cancelled)?;
    } else {
        let (path, _, _) = wait_for_match(
            serial,
            &[
                "battle/records.png",
                "battle/formations_icon.png",
                "battle/battle.png",
            ],
            lower_screen(),
            crate::timing::secs(20),
            cancelled,
        )?;
        log(format!(
            "Battle setup detected ({path}); starting with the current formation."
        ));
    }
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
        return Err(tr!("afk-stages-the-game-requested-payment-adbhelper-does").into());
    }

    if let Some((_, confirm)) = get_match(
        &screen,
        &["navigation/confirm.png", "confirm_text.png"],
        full_screen(),
        0.78,
    )? {
        if mode == GameMode::DurasTrials {
            adb::press_back(serial).map_err(|e| e.to_string())?;
            return Err(tr!("afk-stages-could-not-verify-the-dura-confirmation").into());
        }
        tap_match(serial, confirm, size)?;
        settle_cancellable(cancelled)?;
    }
    Ok(())
}

fn detect_decisive_outcome(screen: &GrayImage) -> Result<Option<(Outcome, Match)>, ActionError> {
    // Failure indicators take priority over generic statistics or transition art.
    // A statistics icon alone appears on both victory and defeat screens.
    for (paths, outcome, threshold) in [
        (&["retry.png"][..], Outcome::Retry, 0.70),
        (
            &["battle/power_up.png"][..],
            Outcome::PowerUp,
            MATCH_THRESHOLD,
        ),
        (
            &["navigation/confirm.png"][..],
            Outcome::NetworkError,
            MATCH_THRESHOLD,
        ),
        (&["next.png", "battle/next.png"][..], Outcome::Next, 0.74),
        (
            &[
                "battle/victory_rewards.png",
                // Wide victory banner, shared with the Sunlit Showdown result screen.
                "event/sunlit_showdown/victory.png",
            ][..],
            Outcome::VictoryRewards,
            0.76,
        ),
        (
            &["afk_stages/tap_to_close.png"][..],
            Outcome::FinalStage,
            MATCH_THRESHOLD,
        ),
    ] {
        if let Some((_, found)) = get_match(screen, paths, lower_screen(), threshold)? {
            return Ok(Some((outcome, found)));
        }
    }
    Ok(None)
}

fn detect_outcome(screen: &GrayImage) -> Result<Option<(Outcome, Match)>, ActionError> {
    if let Some(result) = detect_decisive_outcome(screen)? {
        return Ok(Some(result));
    }
    Ok(
        find_template(screen, "battle/result.png", lower_screen(), MATCH_THRESHOLD)?
            .map(|found| (Outcome::AmbiguousResult, found)),
    )
}

fn detect_outcome_for_mode(
    screen: &GrayImage,
    mode: GameMode,
) -> Result<Option<(Outcome, Match)>, ActionError> {
    if mode == GameMode::DurasTrials {
        let decisive = detect_decisive_outcome(screen)?;
        if decisive
            .as_ref()
            .is_some_and(|(o, _)| matches!(o, Outcome::PowerUp | Outcome::NetworkError))
        {
            return Ok(decisive);
        }
        if let Some(found) = find_template(
            screen,
            "duras_trials/no_next.png",
            Rect {
                x: 648,
                y: 1728,
                width: 432,
                height: 192,
            },
            0.85,
        )? {
            return Ok(Some((Outcome::DuraNoNext, found)));
        }
        if let Some((_, found)) = get_match(
            screen,
            &[
                "duras_trials/first_clear_bottom_half.png",
                "duras_trials/end_sunrise.png",
            ],
            lower_screen(),
            0.85,
        )? {
            return Ok(Some((Outcome::DuraVictory, found)));
        }
        if decisive
            .as_ref()
            .is_some_and(|(o, _)| *o != Outcome::FinalStage)
        {
            return Ok(decisive);
        }
        return Ok(
            find_template(screen, "battle/result.png", lower_screen(), MATCH_THRESHOLD)?
                .map(|found| (Outcome::AmbiguousResult, found)),
        );
    }
    if mode == GameMode::UnionCampaign {
        if let Some(button) = super::union_campaign::floor_battle_button(screen)? {
            return Ok(Some((Outcome::UnionFloorReturned, button)));
        }
        // Keep defeat and network indicators ahead of the Union victory button.
        if let Some((outcome, found)) = detect_decisive_outcome(screen)? {
            if outcome != Outcome::FinalStage {
                return Ok(Some((outcome, found)));
            }
        }
        if let Some(continue_button) = find_template(
            screen,
            "arena/continue.png",
            Rect {
                x: 0,
                y: 1632,
                width: 1080,
                height: 288,
            },
            0.78,
        )? {
            return Ok(Some((Outcome::Next, continue_button)));
        }
        return Ok(
            find_template(screen, "battle/result.png", lower_screen(), MATCH_THRESHOLD)?
                .map(|found| (Outcome::AmbiguousResult, found)),
        );
    }
    if !mode.is_legend_trial() {
        return detect_outcome(screen);
    }
    if let Some(available) = find_template(
        screen,
        "legend_trials/available_after.png",
        lower_screen(),
        MATCH_THRESHOLD,
    )? {
        return Ok(Some((Outcome::FinalStage, available)));
    }
    // The shared AFK close overlay is a milestone in Legend Trial.
    if let Some((outcome, found)) = detect_decisive_outcome(screen)? {
        if outcome != Outcome::FinalStage {
            return Ok(Some((outcome, found)));
        }
    }
    Ok(
        find_template(screen, "battle/result.png", lower_screen(), MATCH_THRESHOLD)?
            .map(|found| (Outcome::AmbiguousResult, found)),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultCheck {
    Retry,
    PowerUp,
    UnionFloorReturned,
    NoDefeat,
}

trait ResultDriver {
    fn wait_for_controls(&mut self) -> Result<(), ActionError>;
    fn check_defeat(&mut self) -> Result<ResultCheck, ActionError>;
    fn retry(&mut self) -> Result<(), ActionError>;
    fn back(&mut self) -> Result<(), ActionError>;
    fn continue_result(&mut self) -> Result<(), ActionError>;
    fn advance_optional_next(&mut self) -> Result<(), ActionError>;
}

/// Mirrors AFKJourneyBase._handle_ambiguous_result_screen:
/// settle 2s, reject Retry/Power Up, continue, then optionally tap Next.
/// The original returns success even when the optional Next wait times out.
fn handle_ambiguous_result(driver: &mut impl ResultDriver) -> Result<BattleOutcome, ActionError> {
    driver.wait_for_controls()?;
    match driver.check_defeat()? {
        ResultCheck::Retry => {
            driver.retry()?;
            return Ok(BattleOutcome::Lost);
        }
        ResultCheck::PowerUp => {
            driver.back()?;
            return Ok(BattleOutcome::Lost);
        }
        ResultCheck::UnionFloorReturned => return Ok(BattleOutcome::Unclear),
        ResultCheck::NoDefeat => {}
    }
    driver.continue_result()?;
    driver.advance_optional_next()?;
    Ok(BattleOutcome::Won)
}

struct AdbResultDriver<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    size: (u32, u32),
    retry: Option<Match>,
    mode: GameMode,
}

impl ResultDriver for AdbResultDriver<'_> {
    fn wait_for_controls(&mut self) -> Result<(), ActionError> {
        for _ in 0..20 {
            ensure_not_cancelled(self.cancelled)?;
            thread::sleep(crate::timing::millis(100));
        }
        ensure_not_cancelled(self.cancelled)
    }
    fn check_defeat(&mut self) -> Result<ResultCheck, ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        let (screen, size) = screenshot(self.serial)?;
        self.size = size;
        if self.mode == GameMode::UnionCampaign
            && super::union_campaign::floor_battle_button(&screen)?.is_some()
        {
            return Ok(ResultCheck::UnionFloorReturned);
        }
        self.retry = find_template(&screen, "retry.png", lower_screen(), 0.70)?;
        if self.retry.is_some() {
            return Ok(ResultCheck::Retry);
        }
        if find_template(
            &screen,
            "battle/power_up.png",
            lower_screen(),
            MATCH_THRESHOLD,
        )?
        .is_some()
        {
            return Ok(ResultCheck::PowerUp);
        }
        Ok(ResultCheck::NoDefeat)
    }
    fn retry(&mut self) -> Result<(), ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        if self.mode == GameMode::DurasTrials {
            adb::press_back(self.serial).map_err(|e| e.to_string())?;
            return settle_cancellable(self.cancelled);
        }
        tap_match(
            self.serial,
            self.retry
                .ok_or(tr!("afk-stages-retry-button-not-detected"))?,
            self.size,
        )?;
        settle_cancellable(self.cancelled)
    }
    fn back(&mut self) -> Result<(), ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        adb::press_back(self.serial).map_err(|e| e.to_string())?;
        settle_cancellable(self.cancelled)
    }
    fn continue_result(&mut self) -> Result<(), ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        if self.mode == GameMode::DurasTrials {
            // Dura result screens may offer a paid Repeat. Only Back is safe
            // here; Sweep is only handled from the Dura stage screen.
            return adb::press_back(self.serial)
                .map_err(|e| e.to_string())
                .map_err(ActionError::from);
        }
        adb::tap_game_point(self.serial, 950, 1800, self.size)
            .map_err(|e| e.to_string())
            .map_err(ActionError::from)
    }
    fn advance_optional_next(&mut self) -> Result<(), ActionError> {
        if self.mode.has_external_progression() {
            return settle_cancellable(self.cancelled);
        }
        let roi = Rect {
            x: 0,
            y: BASE_RESOLUTION.1 * 3 / 5,
            width: BASE_RESOLUTION.0,
            height: BASE_RESOLUTION.1 * 2 / 5,
        };
        if let Some((_, next, size)) = poll_for_match(
            self.serial,
            &["next.png", "battle/next.png"],
            roi,
            crate::timing::secs(5),
            self.cancelled,
        )? {
            tap_match(self.serial, next, size)?;
            settle_cancellable(self.cancelled)?;
        }
        Ok(())
    }
}

fn live_battle_skip(screen: &GrayImage, mode: GameMode) -> Result<Option<Match>, ActionError> {
    if mode != GameMode::UnionCampaign {
        return Ok(None);
    }
    Ok(get_match(
        screen,
        &["battle/skip.png", "battle/skip_orange.png"],
        full_screen(),
        0.78,
    )?
    .map(|(_, found)| found))
}

fn wait_for_outcome(
    serial: &str,
    mode: GameMode,
    log: &mut impl FnMut(String),
    cancelled: &AtomicBool,
) -> Result<(Outcome, Match, (u32, u32)), ActionError> {
    let started = Instant::now();
    let mut previous_floor_return = false;
    while started.elapsed() < crate::timing::secs(BATTLE_TIMEOUT_SECONDS) {
        ensure_not_cancelled(cancelled)?;
        let (screen, size) = screenshot(serial)?;
        match detect_outcome_for_mode(&screen, mode)? {
            Some((Outcome::UnionFloorReturned, _)) if !previous_floor_return => {
                previous_floor_return = true;
                thread::sleep(crate::timing::millis(500));
                continue;
            }
            Some((outcome, found)) => {
                log(format!("Battle result detected: {outcome:?}."));
                return Ok((outcome, found, size));
            }
            None => previous_floor_return = false,
        }
        if let Some(skip) = live_battle_skip(&screen, mode)? {
            ensure_not_cancelled(cancelled)?;
            tap_match(serial, skip, size)?;
            log(tr!("afk-stages-skipping-the-union-campaign-battle").into());
            settle_cancellable(cancelled)?;
            continue;
        }
        thread::sleep(crate::timing::millis(500));
    }
    Err(ActionError::BattleResultTimeout)
}

fn battle_with_current_team(
    serial: &str,
    settings: BattleSettings,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
    cursor: TeamCursor,
    progress: &mut impl FnMut(ProgressEvent),
) -> Result<StageResult, ActionError> {
    let mode = settings.mode;
    let attempts = settings.attempts_per_formation;
    for attempt in (cursor.used + 1)..=attempts {
        ensure_not_cancelled(cancelled)?;
        log(format!("Starting battle attempt {attempt}/{attempts}."));
        if mode == GameMode::DurasTrials {
            super::duras_trials::ensure_formation(serial, cancelled)?;
        }
        start_battle(serial, mode, log, size, cancelled)?;
        progress(ProgressEvent::BattleStarted(mode));
        let (outcome, found, size) = match wait_for_outcome(serial, mode, log, cancelled) {
            Ok(result) => result,
            Err(error) => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                let cursor = TeamCursor {
                    used: attempt,
                    ..cursor
                };
                // Transport loss and game crashes also consume this attempt.
                // Preserve its budget before the shared supervisor reopens the mode.
                super::recovery::save_team_cursor(mode, cursor);
                if error == ActionError::BattleResultTimeout && !cursor.can_resume(settings) {
                    log(tr!("afk-stages-no-battle-result-was-detected-after").into());
                    return Ok(StageResult::FormationFailed);
                }
                return Err(error);
            }
        };
        match outcome {
            Outcome::Next => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                if !mode.has_external_progression() {
                    tap_match(serial, found, size)?;
                    settle_cancellable(cancelled)?;
                }
                return Ok(StageResult::Won);
            }
            Outcome::VictoryRewards => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                if mode == GameMode::DurasTrials {
                    adb::press_back(serial).map_err(|error| error.to_string())?;
                } else {
                    adb::tap_game_point(serial, 550, 1800, size)
                        .map_err(|error| error.to_string())?;
                }
                settle_cancellable(cancelled)?;
                return Ok(StageResult::Won);
            }
            Outcome::Retry => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Lost));
                if mode == GameMode::DurasTrials {
                    adb::press_back(serial).map_err(|e| e.to_string())?;
                } else {
                    tap_match(serial, found, size)?;
                }
                settle_cancellable(cancelled)?;
            }
            Outcome::PowerUp => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Lost));
                if mode == GameMode::DurasTrials {
                    adb::press_back(serial).map_err(|e| e.to_string())?;
                    settle_cancellable(cancelled)?;
                    super::duras_trials::ensure_formation(serial, cancelled)?;
                    continue;
                }
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
                let mut driver = AdbResultDriver {
                    serial,
                    cancelled,
                    size,
                    retry: None,
                    mode,
                };
                let outcome = handle_ambiguous_result(&mut driver).inspect_err(|_| {
                    progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                })?;
                progress(ProgressEvent::BattleFinished(mode, outcome));
                if matches!(outcome, BattleOutcome::Won) {
                    progress(ProgressEvent::StageCleared(mode));
                    return Ok(StageResult::Won);
                }
            }
            Outcome::UnionFloorReturned => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                log(tr!("afk-stages-union-campaign-returned-to-the-floor").into());
            }
            Outcome::DuraNoNext | Outcome::DuraVictory => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Won));
                progress(ProgressEvent::StageCleared(mode));
                if outcome == Outcome::DuraNoNext {
                    adb::press_back(serial).map_err(|e| e.to_string())?;
                    settle_cancellable(cancelled)?;
                }
                return Ok(StageResult::Won);
            }
            Outcome::NetworkError => {
                progress(ProgressEvent::BattleFinished(mode, BattleOutcome::Unclear));
                if mode != GameMode::DurasTrials {
                    tap_match(serial, found, size)?;
                }
                return Err(tr!("afk-stages-a-confirmation-popup-appeared-after-battle").into());
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

pub(super) fn play_one_stage(
    serial: &str,
    log: &mut impl FnMut(String),
    size: (u32, u32),
    cancelled: &AtomicBool,
    settings: BattleSettings,
    progress: &mut impl FnMut(ProgressEvent),
) -> Result<StageResult, ActionError> {
    let (screen, _) = screenshot(serial)?;
    let copy_is_open = find_template(&screen, "battle/copy.png", lower_screen(), 0.78)?.is_some();
    let resume = super::recovery::take_team_cursor(settings.mode);

    let current = TeamCursor {
        slot: TeamSlot::Current,
        used: 0,
    };
    let first = resume.unwrap_or(current);
    if first.slot == TeamSlot::Fallback {
        return battle_with_current_team(serial, settings, log, size, cancelled, first, progress);
    }
    if first.slot == TeamSlot::Current
        && (settings.current_team_first || !settings.use_suggested_formations)
        && first.used < settings.attempts_per_formation
        && (!copy_is_open || resume.is_some())
    {
        if copy_is_open {
            ensure_battle_screen(serial, settings.mode, cancelled)?;
        }
        log("Trying the currently selected team first.".into());
        match battle_with_current_team(serial, settings, log, size, cancelled, first, progress)? {
            StageResult::Won => return Ok(StageResult::Won),
            StageResult::FinalStage => return Ok(StageResult::FinalStage),
            StageResult::FormationFailed => {}
        }
    }

    if !settings.use_suggested_formations {
        return Ok(StageResult::FormationFailed);
    }

    for &pass in formation_passes(settings.skip_manual_formations) {
        if matches!(first.slot, TeamSlot::Ranked(saved_pass, _) if pass < saved_pass) {
            continue;
        }
        if pass == FormationPass::Manual {
            log(tr!("afk-stages-automatic-teams-exhausted-trying-manual-teams").into());
        }
        let mut formation_number = first.first_rank(pass, settings.attempts_per_formation);
        while formation_number <= settings.max_formations {
            ensure_not_cancelled(cancelled)?;
            if settings.mode == GameMode::DurasTrials {
                super::duras_trials::ensure_formation(serial, cancelled)?;
            }
            let choice = copy_next_formation(
                &mut AdbFormationBrowser {
                    serial,
                    mode: settings.mode,
                    cancelled,
                    log,
                },
                &mut formation_number,
                settings.max_formations,
                pass,
            )?;
            let slot = match choice {
                FormationChoice::Copied => TeamSlot::Ranked(pass, formation_number),
                FormationChoice::CurrentTeam if resume.is_none() => TeamSlot::Fallback,
                FormationChoice::CurrentTeam => return Ok(StageResult::FormationFailed),
                FormationChoice::Exhausted => break,
            };
            let used = resume
                .filter(|cursor| cursor.slot == slot)
                .map_or(0, |cursor| cursor.used);
            log(tr!(
                "afk-stages-trying-team-formation-number-pass",
                formation_number = formation_number,
                pass = format!("{:?}", pass)
            ));
            match battle_with_current_team(
                serial,
                settings,
                log,
                size,
                cancelled,
                TeamCursor { slot, used },
                progress,
            )? {
                StageResult::Won => return Ok(StageResult::Won),
                StageResult::FinalStage => return Ok(StageResult::FinalStage),
                StageResult::FormationFailed => {}
            }
            if choice == FormationChoice::CurrentTeam {
                return Ok(StageResult::FormationFailed);
            }
            formation_number += 1;
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

fn afk_stages_visible(screen: &GrayImage) -> Result<bool, ActionError> {
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

fn stage_entry_point(
    screen: &GrayImage,
    mode: GameMode,
) -> Result<Option<(u32, u32)>, ActionError> {
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
            GameMode::SeasonLegendTrial(_)
            | GameMode::UnionCampaign
            | GameMode::DurasTrials
            | GameMode::Arena
            | GameMode::SupremeArena
            | GameMode::AfkRewards
            | GameMode::FreeHourglasses
            | GameMode::DiamondHourglasses
            | GameMode::StageRewards
            | GameMode::DreamRealm
            | GameMode::SeasonRankingRewards(_)
            | GameMode::FriendRewards
            | GameMode::MailRewards
            | GameMode::BattlePassRewards
            | GameMode::DailyQuestRewards
            | GameMode::GuildQuestRewards
            | GameMode::CollectAffinity
            | GameMode::LevelUpHeroes
            | GameMode::EquipNewEquipment
            | GameMode::Emporium
            | GameMode::SinglePull
            | GameMode::EssenceSwap
            | GameMode::HeroScan
            | GameMode::GuildScan
            | GameMode::Fishing => return Err(tr!("afk-stages-this-is-not-an-afk-stages").into()),
        }));
    }
    if mode == GameMode::AfkStages {
        if let Some(button) = large {
            return Ok(Some(button.center()));
        }
    }
    Err(tr!(
        "afk-stages-could-not-detect-the-entry-point",
        label = mode.label()
    )
    .into())
}

#[derive(Debug)]
enum OverviewAction {
    Reached,
    Back,
    CancelExit,
    Tap(Match),
    EnterGame,
    SelectCrest(Match),
}

fn overview_action(screen: &GrayImage) -> Result<OverviewAction, ActionError> {
    // Handle overlays before looking for overview controls behind them.
    if get_match(
        screen,
        &[
            "battle/copy.png",
            "navigation/dotdotdot.png",
            "popup/quick_purchase.png",
            "battle/spend.png",
            "battle/gold.png",
        ],
        full_screen(),
        0.8,
    )?
    .is_some()
    {
        return Ok(OverviewAction::Back);
    }
    if let Some((control, close)) = get_match(
        screen,
        &[
            "cancel.png",
            "guide/close.png",
            "guide/next.png",
            "navigation/x.png",
            "login/claim.png",
            "arcane_labyrinth/back_arrow.png",
            "navigation/resonating_hall_back.png",
            "battle/exit_door.png",
            "battle/skip.png",
            "battle/skip_orange.png",
            "navigation/confirm.png",
            "navigation/confirm_text.png",
            "confirm_text.png",
        ],
        full_screen(),
        0.8,
    )? {
        if control.contains("confirm")
            && super::ocr::recognize(screen).is_ok_and(|lines| exit_prompt(&lines))
        {
            return Ok(OverviewAction::CancelExit);
        }
        return Ok(OverviewAction::Tap(close));
    }
    if let Some(crest) = find_template(
        screen,
        "arcane_labyrinth/select_a_crest.png",
        full_screen(),
        0.8,
    )? {
        return Ok(OverviewAction::SelectCrest(crest));
    }
    if find_template(screen, "navigation/notice.png", full_screen(), 0.8)?.is_some() {
        return Ok(OverviewAction::EnterGame);
    }
    if get_match(
        screen,
        &[
            "navigation/homestead/homestead_enter.png",
            "navigation/homestead/homestead_invaded.png",
            "navigation/homestead/world.png",
            "navigation/time_of_day.png",
            "navigation/resonating_hall_shortcut.png",
        ],
        full_screen(),
        0.8,
    )?
    .is_some()
    {
        return Ok(OverviewAction::Reached);
    }
    if let Ok(lines) = super::ocr::recognize(screen) {
        if exit_prompt(&lines) {
            return Ok(OverviewAction::CancelExit);
        }
        // The footer is also present with seasonal world layouts that have no
        // matching shortcut or time-of-day icon.
        let footer = |name: &str, range: std::ops::Range<u32>| {
            lines.iter().any(|line| {
                let (x, y) = line.center();
                y >= 1850 && range.contains(&x) && line.text.trim().eq_ignore_ascii_case(name)
            })
        };
        if footer("Battle Modes", 350..570)
            && footer("Guild", 710..830)
            && (footer("Hall", 540..680) || footer("House", 240..365))
        {
            return Ok(OverviewAction::Reached);
        }
    }
    Ok(OverviewAction::Back)
}

fn exit_prompt(lines: &[super::ocr::TextLine]) -> bool {
    lines.iter().any(|line| {
        (840..1030).contains(&line.center().1)
            && line
                .text
                .to_ascii_lowercase()
                .contains("you want to exit the")
    })
}

pub(super) fn navigate_to_current_overview(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(u32, u32), ActionError> {
    let started = Instant::now();
    for attempt in 0..40 {
        ensure_not_cancelled(cancelled)?;
        if started.elapsed() > crate::timing::secs(240) {
            break;
        }
        if attempt == 20 {
            log(tr!("afk-stages-retrying-navigation-to-the-main-screen").into());
            adb::stop_afk_journey(serial).map_err(|error| error.to_string())?;
            adb::launch_afk_journey(serial).map_err(|error| error.to_string())?;
            for _ in 0..5 {
                settle_cancellable(cancelled)?;
            }
        }
        let (screen, size) = screenshot(serial)?;
        match overview_action(&screen)? {
            OverviewAction::Reached => return Ok(size),
            OverviewAction::Back => adb::press_back(serial).map_err(|error| error.to_string())?,
            OverviewAction::CancelExit => {
                adb::tap_game_point(serial, 635, 1270, size).map_err(|error| error.to_string())?;
            }
            OverviewAction::Tap(found) => tap_match(serial, found, size)?,
            OverviewAction::EnterGame => {
                adb::tap_game_point(serial, 540, 960, size).map_err(|error| error.to_string())?;
                for _ in 0..3 {
                    settle_cancellable(cancelled)?;
                }
            }
            OverviewAction::SelectCrest(crest) => {
                adb::tap_game_point(serial, 550, 1460, size).map_err(|error| error.to_string())?;
                settle_cancellable(cancelled)?;
                tap_match(serial, crest, size)?;
            }
        }
        settle_cancellable(cancelled)?;
    }
    Err(tr!("afk-stages-could-not-reach-the-main-screen").into())
}

type GameScreen = (GrayImage, (u32, u32));

trait AfkNavigator {
    fn overview(&mut self) -> Result<(), ActionError>;
    fn afk_shortcut(&mut self) -> Result<Option<GameScreen>, ActionError>;
    fn battle_modes(&mut self) -> Result<bool, ActionError>;
    fn afk_page(&mut self) -> Result<Option<GameScreen>, ActionError>;
}

fn enter_afk_page(navigator: &mut impl AfkNavigator) -> Result<GameScreen, ActionError> {
    // Every mode, including Season -> normal, starts from a verified overview.
    // A battle/Records/result screen must never be accepted as the AFK selector.
    for _ in 0..3 {
        navigator.overview()?;
        if let Some(screen) = navigator.afk_shortcut()? {
            return Ok(screen);
        }
        if navigator.battle_modes()? {
            if let Some(screen) = navigator.afk_page()? {
                return Ok(screen);
            }
        }
    }
    Err(tr!("afk-stages-could-not-open-the-afk-page").into())
}

fn apex_stage_shortcut(screen: &GrayImage) -> Result<Option<Match>, ActionError> {
    find_template(
        screen,
        "navigation/apex_stage_shortcut.png",
        Rect {
            x: 0,
            y: 1815,
            width: 200,
            height: 90,
        },
        0.9,
    )
}

struct AdbAfkNavigator<'a, L> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    log: &'a mut L,
    size: (u32, u32),
}

impl<L: FnMut(String)> AdbAfkNavigator<'_, L> {
    fn wait_for_afk_page(&self, timeout: Duration) -> Result<Option<GameScreen>, ActionError> {
        let started = Instant::now();
        while started.elapsed() < timeout {
            ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = screenshot(self.serial)?;
            if super::daily::claim_control(&screen)?.is_some() {
                // The idle reward popup closes on a tap outside its controls.
                adb::tap_game_point(self.serial, 150, 1070, size)
                    .map_err(|error| error.to_string())?;
            } else if afk_stages_visible(&screen)? && super::daily::page(&screen)? {
                return Ok(Some((screen, size)));
            }
            settle_cancellable(self.cancelled)?;
        }
        Ok(None)
    }
}

impl<L: FnMut(String)> AfkNavigator for AdbAfkNavigator<'_, L> {
    fn overview(&mut self) -> Result<(), ActionError> {
        self.size = navigate_to_current_overview(self.serial, self.cancelled, self.log)?;
        settle_cancellable(self.cancelled)
    }

    fn afk_shortcut(&mut self) -> Result<Option<GameScreen>, ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        let (screen, size) = screenshot(self.serial)?;
        let Some(shortcut) = apex_stage_shortcut(&screen)? else {
            return Ok(None);
        };
        tap_match(self.serial, shortcut, size)?;
        self.wait_for_afk_page(crate::timing::secs(10))
    }

    fn battle_modes(&mut self) -> Result<bool, ActionError> {
        ensure_not_cancelled(self.cancelled)?;
        adb::tap_game_point(self.serial, 460, 1830, self.size)
            .map_err(|error| error.to_string())?;
        for _ in 0..2 {
            let Some((path, _, _)) = poll_for_match(
                self.serial,
                &["battle_modes/afk_stage.png", "popup/quick_purchase.png"],
                full_screen(),
                crate::timing::secs(20),
                self.cancelled,
            )?
            else {
                return Ok(false);
            };
            if path == "battle_modes/afk_stage.png" {
                return Ok(true);
            }
            adb::press_back(self.serial).map_err(|error| error.to_string())?;
            settle_cancellable(self.cancelled)?;
        }
        Ok(false)
    }

    fn afk_page(&mut self) -> Result<Option<GameScreen>, ActionError> {
        // Retry while the tile is still visible, then verify the selector.
        for _ in 0..3 {
            ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = screenshot(self.serial)?;
            if let Some(entry) =
                find_template(&screen, "battle_modes/afk_stage.png", full_screen(), 0.75)?
            {
                tap_match(self.serial, entry, size)?;
                settle_cancellable(self.cancelled)?;
            } else {
                break;
            }
        }
        self.wait_for_afk_page(crate::timing::secs(20))
    }
}

pub(super) fn navigate_to_battle_modes(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(u32, u32), ActionError> {
    let mut navigator = AdbAfkNavigator {
        serial,
        cancelled,
        log,
        size: BASE_RESOLUTION,
    };
    for _ in 0..3 {
        navigator.overview()?;
        if navigator.battle_modes()? {
            return Ok(navigator.size);
        }
    }
    Err(tr!("afk-stages-could-not-open-battle-modes").into())
}

pub(super) fn navigate_to_afk_stages(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<GameScreen, ActionError> {
    ensure_not_cancelled(cancelled)?;
    adb::launch_afk_journey(serial).map_err(|error| error.to_string())?;
    for _ in 0..3 {
        settle_cancellable(cancelled)?;
    }
    enter_afk_page(&mut AdbAfkNavigator {
        serial,
        cancelled,
        log,
        size: BASE_RESOLUTION,
    })
}

pub fn run<F, P>(
    serial: &str,
    battle_settings: BattleSettings,
    cancelled: &AtomicBool,
    mut log: F,
    mut progress: P,
) -> Result<ModeSummary, ActionError>
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
        log(tr!(
            "afk-stages-label-is-unavailable-or-already-completed",
            label = mode.label()
        ));
        return Ok(ModeSummary {
            cleared: 0,
            end: ModeEnd::Unavailable,
        });
    };
    adb::tap_game_point(serial, x, y, size).map_err(|error| error.to_string())?;
    log(tr!("afk-stages-selected-label", label = mode.label()));
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
        log(tr!(
            "afk-stages-label-preparing-stage-cleared",
            label = mode.label(),
            cleared = cleared + 1
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
                log(tr!(
                    "afk-stages-label-cleared-stages-cleared",
                    label = mode.label(),
                    cleared = cleared
                ));
            }
            Ok(StageResult::FinalStage) => {
                log(tr!(
                    "afk-stages-label-final-stage-reached",
                    label = mode.label()
                ));
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
    use std::sync::atomic::Ordering;

    #[test]
    fn timeout_cursor_keeps_the_remaining_attempts_and_rank_order() {
        let settings = BattleSettings {
            mode: GameMode::SeasonAfkStages,
            use_suggested_formations: true,
            current_team_first: true,
            skip_manual_formations: false,
            max_formations: 3,
            attempts_per_formation: 2,
        };
        let pass = FormationPass::Automatic;
        let mut cursor = TeamCursor {
            slot: TeamSlot::Ranked(pass, 3),
            used: 1,
        };
        assert!(cursor.can_resume(settings));
        assert_eq!(cursor.first_rank(pass, settings.attempts_per_formation), 3);

        cursor.used = 2;
        assert!(cursor.can_resume(settings));
        assert_eq!(cursor.first_rank(pass, settings.attempts_per_formation), 4);
        assert_eq!(
            cursor.first_rank(FormationPass::Manual, settings.attempts_per_formation),
            1
        );
        assert!(!cursor.can_resume(BattleSettings {
            skip_manual_formations: true,
            ..settings
        }));

        cursor.slot = TeamSlot::Ranked(FormationPass::Manual, 3);
        assert!(!cursor.can_resume(settings));
    }

    struct FakeFormations {
        // (manual, can_copy)
        teams: Vec<(bool, bool)>,
        displayed: usize,
        copied: Vec<usize>,
        visited: Vec<usize>,
        next_error: bool,
    }

    impl FormationBrowser for FakeFormations {
        fn open(&mut self) -> Result<bool, ActionError> {
            self.displayed = 1;
            self.visited.push(1);
            Ok(true)
        }
        fn next(&mut self) -> Result<bool, ActionError> {
            if self.next_error {
                return Err("ADB getrennt".into());
            }
            if self.displayed == self.teams.len() {
                return Ok(false);
            }
            self.displayed += 1;
            self.visited.push(self.displayed);
            Ok(true)
        }
        fn is_manual(&mut self) -> Result<bool, ActionError> {
            Ok(self.teams[self.displayed - 1].0)
        }
        fn copy(&mut self) -> Result<bool, ActionError> {
            if !self.teams[self.displayed - 1].1 {
                return Ok(false);
            }
            self.copied.push(self.displayed);
            Ok(true)
        }
    }

    fn formations(teams: &[(bool, bool)]) -> FakeFormations {
        FakeFormations {
            teams: teams.to_vec(),
            displayed: 1,
            copied: vec![],
            visited: vec![],
            next_error: false,
        }
    }

    fn exhaust_teams(browser: &mut FakeFormations, skip_manual: bool) -> Result<(), ActionError> {
        for &pass in formation_passes(skip_manual) {
            let mut target = 1;
            while target <= browser.teams.len() as u32 {
                let limit = browser.teams.len() as u32;
                if copy_next_formation(browser, &mut target, limit, pass)?
                    == FormationChoice::Exhausted
                {
                    break;
                }
                target += 1;
            }
        }
        Ok(())
    }

    #[test]
    fn manual_and_locked_skips_do_not_jump_over_the_next_team() {
        let mut browser = formations(&[(true, true), (false, false), (false, true)]);
        let mut target = 1;
        assert_eq!(
            copy_next_formation(&mut browser, &mut target, 3, FormationPass::Automatic).unwrap(),
            FormationChoice::Copied
        );
        assert_eq!(target, 3);
        assert_eq!(browser.visited, vec![1, 2, 3]);
        assert_eq!(browser.copied, vec![3]);
    }

    #[test]
    fn all_automatic_teams_run_before_manual_teams_with_a_fresh_list_each_pass() {
        let mut browser = formations(&[(true, true), (false, true), (true, true), (false, true)]);
        exhaust_teams(&mut browser, false).unwrap();
        assert_eq!(browser.copied, vec![2, 4, 1, 3]);
    }

    #[test]
    fn skip_manual_option_excludes_every_manual_team() {
        let mut browser = formations(&[(true, true), (false, true), (true, true)]);
        exhaust_teams(&mut browser, true).unwrap();
        assert_eq!(browser.copied, vec![2]);
        let mut only_manual = formations(&[(true, true), (true, true)]);
        exhaust_teams(&mut only_manual, true).unwrap();
        assert!(only_manual.copied.is_empty());
        exhaust_teams(&mut only_manual, false).unwrap();
        assert_eq!(only_manual.copied, vec![1, 2]);
    }

    #[test]
    fn no_next_arrow_ends_the_pass_and_adb_errors_are_not_swallowed() {
        let mut browser = formations(&[(true, true)]);
        let mut target = 1;
        assert_eq!(
            copy_next_formation(&mut browser, &mut target, 10, FormationPass::Automatic).unwrap(),
            FormationChoice::Exhausted
        );
        browser.next_error = true;
        assert_eq!(
            copy_next_formation(&mut browser, &mut target, 10, FormationPass::Automatic)
                .unwrap_err(),
            "ADB getrennt"
        );
    }

    #[test]
    fn manual_labels_are_recognized_only_in_the_original_lower_left_region() {
        for path in ["battle/manual_battle.png", "battle/manual_battle_old1.png"] {
            let template = load_template(path).unwrap();
            let mut screen = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &template, 30, 1100);
            assert!(manual_formation_visible(&screen).unwrap(), "{path}");
            let mut wrong_region = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut wrong_region, &template, 600, 300);
            assert!(!manual_formation_visible(&wrong_region).unwrap(), "{path}");
        }
    }

    struct FakeNavigation {
        trace: Vec<&'static str>,
        failures: u32,
        fail_overview: bool,
        shortcut: bool,
    }

    impl AfkNavigator for FakeNavigation {
        fn overview(&mut self) -> Result<(), ActionError> {
            self.trace.push("overview");
            if self.fail_overview {
                return Err("Stop".into());
            }
            Ok(())
        }
        fn afk_shortcut(&mut self) -> Result<Option<GameScreen>, ActionError> {
            self.trace.push("afk_shortcut");
            Ok(self
                .shortcut
                .then(|| (GrayImage::new(1, 1), BASE_RESOLUTION)))
        }
        fn battle_modes(&mut self) -> Result<bool, ActionError> {
            self.trace.push("battle_modes");
            if self.failures > 0 {
                self.failures -= 1;
                return Ok(false);
            }
            Ok(true)
        }
        fn afk_page(&mut self) -> Result<Option<GameScreen>, ActionError> {
            self.trace.push("afk_page");
            Ok(Some((GrayImage::new(1, 1), BASE_RESOLUTION)))
        }
    }

    #[test]
    fn season_to_normal_returns_to_overview_before_opening_battle_modes_again() {
        let mut navigator = FakeNavigation {
            trace: vec![],
            failures: 0,
            fail_overview: false,
            shortcut: false,
        };
        run_sequence(StageSelection::default(), &AtomicBool::new(false), |_| {
            enter_afk_page(&mut navigator)?;
            Ok(ModeSummary {
                cleared: 0,
                end: ModeEnd::AttemptsExhausted,
            })
        })
        .unwrap();
        assert_eq!(
            navigator.trace,
            vec![
                "overview",
                "afk_shortcut",
                "battle_modes",
                "afk_page",
                "overview",
                "afk_shortcut",
                "battle_modes",
                "afk_page"
            ]
        );
    }

    #[test]
    fn failed_battle_mode_navigation_retries_from_overview_and_stop_propagates() {
        let mut navigator = FakeNavigation {
            trace: vec![],
            failures: 1,
            fail_overview: false,
            shortcut: false,
        };
        enter_afk_page(&mut navigator).unwrap();
        assert_eq!(
            navigator.trace,
            vec![
                "overview",
                "afk_shortcut",
                "battle_modes",
                "overview",
                "afk_shortcut",
                "battle_modes",
                "afk_page"
            ]
        );
        navigator.trace.clear();
        navigator.fail_overview = true;
        assert_eq!(enter_afk_page(&mut navigator).unwrap_err(), "Stop");
        assert_eq!(navigator.trace, vec!["overview"]);
    }

    #[test]
    fn world_shortcut_reaches_afk_without_opening_battle_modes() {
        let mut navigator = FakeNavigation {
            trace: vec![],
            failures: 0,
            fail_overview: false,
            shortcut: true,
        };
        enter_afk_page(&mut navigator).unwrap();
        assert_eq!(navigator.trace, vec!["overview", "afk_shortcut"]);
    }

    #[test]
    #[ignore = "checks captured world, reward popup, selector, and formation screens"]
    fn captured_afk_world_navigation() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_PROGRESSION_CAPTURES").unwrap(),
        );
        let load = |name: &str| {
            normalize_screenshot(&std::fs::read(root.join(format!("{name}.png"))).unwrap())
                .unwrap()
                .0
        };
        let world = load("afk_world_report_current");
        assert!(matches!(
            overview_action(&world).unwrap(),
            OverviewAction::Reached
        ));
        assert!(apex_stage_shortcut(&world).unwrap().is_some());
        let reward = load("afk_world_via_shortcut");
        assert!(super::super::daily::claim_control(&reward)
            .unwrap()
            .is_some());
        for name in ["afk_world_selector_settled", "afk_world_dismiss_rewards"] {
            let selector = load(name);
            assert!(afk_stages_visible(&selector).unwrap());
            assert!(super::super::daily::page(&selector).unwrap());
            assert!(apex_stage_shortcut(&selector).unwrap().is_none());
            assert!(stage_entry_point(&selector, GameMode::AfkStages)
                .unwrap()
                .is_some());
            assert!(stage_entry_point(&selector, GameMode::SeasonAfkStages)
                .unwrap()
                .is_some());
        }
    }

    #[test]
    #[ignore = "opens both AFK team formations through production navigation without starting battles"]
    fn connected_emulator_afk_stage_navigation() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = AtomicBool::new(false);
        let mut log = |message| println!("{message}");
        for mode in [GameMode::SeasonAfkStages, GameMode::AfkStages] {
            let (screen, size) = navigate_to_afk_stages(&serial, &cancelled, &mut log).unwrap();
            let (x, y) = stage_entry_point(&screen, mode)
                .unwrap()
                .expect("stage entry");
            adb::tap_game_point(&serial, x, y, size).unwrap();
            let formation = poll_for_match(
                &serial,
                &["battle/records.png", "battle/battle.png"],
                lower_screen(),
                Duration::from_secs(25),
                &cancelled,
            )
            .unwrap();
            assert!(formation.is_some(), "{}", mode.label());
            println!("{} formation reached.", mode.label());
        }
    }

    #[test]
    fn formation_and_stage_pages_are_not_accepted_as_overview() {
        for (path, x, y) in [
            ("battle/copy.png", 400, 1500),
            ("afk_stages/season_battle.png", 700, 1700),
        ] {
            let mut screen = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &load_template(path).unwrap(), x, y);
            assert!(
                matches!(overview_action(&screen).unwrap(), OverviewAction::Back),
                "{path}"
            );
        }
    }

    #[test]
    fn overview_recognizes_world_and_homestead_and_dismisses_overlays_first() {
        for path in [
            "navigation/homestead/homestead_enter.png",
            "navigation/homestead/world.png",
        ] {
            let mut screen = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &load_template(path).unwrap(), 800, 1100);
            assert!(
                matches!(overview_action(&screen).unwrap(), OverviewAction::Reached),
                "{path}"
            );
            image::imageops::replace(
                &mut screen,
                &load_template("navigation/confirm.png").unwrap(),
                350,
                1400,
            );
            assert!(matches!(
                overview_action(&screen).unwrap(),
                OverviewAction::Tap(_)
            ));
        }
    }

    #[test]
    fn legend_limits_and_milestones_are_not_confused_with_afk_final_stages() {
        let mode = GameMode::SeasonLegendTrial(crate::activity::LegendTower::Wilder);
        let screen = result_screen(&["afk_stages/tap_to_close.png"]);
        assert_eq!(
            detect_outcome(&screen).unwrap().unwrap().0,
            Outcome::FinalStage
        );
        assert!(detect_outcome_for_mode(&screen, mode).unwrap().is_none());
        let screen = result_screen(&["legend_trials/available_after.png"]);
        assert_eq!(
            detect_outcome_for_mode(&screen, mode).unwrap().unwrap().0,
            Outcome::FinalStage
        );
    }

    #[test]
    fn union_victory_continue_confirms_a_win_but_defeat_indicators_take_priority() {
        let mut screen = GrayImage::new(1080, 1920);
        image::imageops::replace(
            &mut screen,
            &load_template("arena/continue.png").unwrap(),
            430,
            1740,
        );
        assert_eq!(
            detect_outcome_for_mode(&screen, GameMode::UnionCampaign)
                .unwrap()
                .unwrap()
                .0,
            Outcome::Next
        );
        image::imageops::replace(&mut screen, &load_template("retry.png").unwrap(), 120, 1300);
        assert_eq!(
            detect_outcome_for_mode(&screen, GameMode::UnionCampaign)
                .unwrap()
                .unwrap()
                .0,
            Outcome::Retry
        );
    }

    #[test]
    fn live_skip_is_exclusive_to_union_campaign() {
        for path in ["battle/skip.png", "battle/skip_orange.png"] {
            let mut screen = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &load_template(path).unwrap(), 500, 800);
            assert!(live_battle_skip(&screen, GameMode::UnionCampaign)
                .unwrap()
                .is_some());
            for mode in [
                GameMode::AfkStages,
                GameMode::SeasonAfkStages,
                GameMode::SeasonLegendTrial(crate::activity::LegendTower::Wilder),
            ] {
                assert!(live_battle_skip(&screen, mode).unwrap().is_none());
            }
        }
    }

    #[test]
    fn records_text_is_detected_at_every_search_grid_phase() {
        let template = load_template("battle/records.png").unwrap();
        for offset in 0..4 {
            let mut screen = GrayImage::new(1080, 1920);
            image::imageops::replace(&mut screen, &template, 201 + offset, 1741 + offset);
            let found = find_template(&screen, "battle/records.png", lower_screen(), 0.8)
                .unwrap()
                .expect("Records at off-grid position");
            assert_eq!(
                (found.x, found.y),
                ((201 + offset) as u32, (1741 + offset) as u32)
            );
        }
    }

    #[test]
    fn dura_paid_replay_does_not_override_explicit_victory_markers() {
        let mode = GameMode::DurasTrials;
        for (path, expected) in [
            (
                "duras_trials/first_clear_bottom_half.png",
                Outcome::DuraVictory,
            ),
            ("duras_trials/no_next.png", Outcome::DuraNoNext),
        ] {
            let mut screen = result_screen(&[path, "retry.png"]);
            if expected == Outcome::DuraNoNext {
                screen = result_screen(&["retry.png"]);
                image::imageops::replace(&mut screen, &load_template(path).unwrap(), 650, 1750);
            }
            assert_eq!(
                detect_outcome_for_mode(&screen, mode).unwrap().unwrap().0,
                expected
            );
        }
        assert_eq!(
            detect_outcome_for_mode(&result_screen(&["retry.png"]), mode)
                .unwrap()
                .unwrap()
                .0,
            Outcome::Retry
        );
    }

    #[test]
    fn dura_no_token_caption_above_result_footer_does_not_end_a_trial() {
        let screen = result_screen(&["duras_trials/no_next.png", "retry.png"]);
        assert_eq!(
            detect_outcome_for_mode(&screen, GameMode::DurasTrials)
                .unwrap()
                .unwrap()
                .0,
            Outcome::Retry
        );
    }

    fn result_screen(paths: &[&str]) -> GrayImage {
        let mut screen = GrayImage::new(BASE_RESOLUTION.0, BASE_RESOLUTION.1);
        for (index, path) in paths.iter().enumerate() {
            let template = load_template(path).unwrap();
            image::imageops::replace(
                &mut screen,
                &template,
                120 + index as i64 * 400,
                1300 + index as i64 * 220,
            );
        }
        screen
    }

    #[test]
    fn both_next_button_styles_confirm_a_win_without_an_extra_click() {
        for path in ["next.png", "battle/next.png"] {
            let screen = result_screen(&[path]);
            assert_eq!(
                detect_decisive_outcome(&screen).unwrap().unwrap().0,
                Outcome::Next,
                "{path}"
            );
        }
    }

    #[test]
    fn victory_rewards_confirm_a_win_even_without_a_next_button() {
        for path in [
            "battle/victory_rewards.png",
            "event/sunlit_showdown/victory.png",
        ] {
            let screen = result_screen(&[path]);
            assert_eq!(
                detect_decisive_outcome(&screen).unwrap().unwrap().0,
                Outcome::VictoryRewards,
                "{path}"
            );
        }
    }

    #[test]
    fn statistics_icon_dispatches_to_original_result_handler() {
        let screen = result_screen(&["battle/result.png"]);
        assert!(detect_decisive_outcome(&screen).unwrap().is_none());
        assert_eq!(
            detect_outcome(&screen).unwrap().unwrap().0,
            Outcome::AmbiguousResult
        );
    }

    struct FakeResultDriver {
        check: ResultCheck,
        next_present: bool,
        cancelled: bool,
        next_error: bool,
        steps: Vec<&'static str>,
    }

    impl FakeResultDriver {
        fn new(check: ResultCheck, next_present: bool) -> Self {
            Self {
                check,
                next_present,
                cancelled: false,
                next_error: false,
                steps: Vec::new(),
            }
        }
    }

    impl ResultDriver for FakeResultDriver {
        fn wait_for_controls(&mut self) -> Result<(), ActionError> {
            self.steps.push("wait-2-seconds");
            if self.cancelled {
                Err(ActionError::Cancelled)
            } else {
                Ok(())
            }
        }
        fn check_defeat(&mut self) -> Result<ResultCheck, ActionError> {
            self.steps.push("check-retry-and-power-up");
            Ok(self.check)
        }
        fn retry(&mut self) -> Result<(), ActionError> {
            self.steps.push("tap-retry");
            Ok(())
        }
        fn back(&mut self) -> Result<(), ActionError> {
            self.steps.push("back");
            Ok(())
        }
        fn continue_result(&mut self) -> Result<(), ActionError> {
            self.steps.push("tap-950-1800");
            Ok(())
        }
        fn advance_optional_next(&mut self) -> Result<(), ActionError> {
            self.steps.push(if self.next_present {
                "tap-next"
            } else {
                "next-timeout"
            });
            if self.next_error {
                Err("ADB unavailable".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn original_fallback_accepts_result_when_optional_next_is_missing() {
        let mut driver = FakeResultDriver::new(ResultCheck::NoDefeat, false);
        assert!(matches!(
            handle_ambiguous_result(&mut driver).unwrap(),
            BattleOutcome::Won
        ));
        assert_eq!(
            driver.steps,
            vec![
                "wait-2-seconds",
                "check-retry-and-power-up",
                "tap-950-1800",
                "next-timeout"
            ]
        );
    }

    #[test]
    fn original_fallback_taps_next_when_it_is_present() {
        let mut driver = FakeResultDriver::new(ResultCheck::NoDefeat, true);
        assert!(matches!(
            handle_ambiguous_result(&mut driver).unwrap(),
            BattleOutcome::Won
        ));
        assert_eq!(
            driver.steps,
            vec![
                "wait-2-seconds",
                "check-retry-and-power-up",
                "tap-950-1800",
                "tap-next"
            ]
        );
    }

    #[test]
    fn delayed_retry_and_power_up_do_not_use_the_success_fallback() {
        for (check, action) in [
            (ResultCheck::Retry, "tap-retry"),
            (ResultCheck::PowerUp, "back"),
        ] {
            let mut driver = FakeResultDriver::new(check, false);
            assert!(matches!(
                handle_ambiguous_result(&mut driver).unwrap(),
                BattleOutcome::Lost
            ));
            assert_eq!(
                driver.steps,
                vec!["wait-2-seconds", "check-retry-and-power-up", action]
            );
        }
    }

    #[test]
    fn union_floor_during_result_transition_preserves_unknown_outcome() {
        let mut driver = FakeResultDriver::new(ResultCheck::UnionFloorReturned, false);
        assert!(matches!(
            handle_ambiguous_result(&mut driver).unwrap(),
            BattleOutcome::Unclear
        ));
        assert_eq!(driver.steps, ["wait-2-seconds", "check-retry-and-power-up"]);
    }

    #[test]
    #[ignore = "requires a cropped Union Campaign floor screenshot"]
    fn captured_union_direct_floor_return() {
        let path = std::env::var_os("ADBHELPER_TEST_UNION_FLOOR").expect("set screenshot path");
        let (screen, _) = normalize_screenshot(&std::fs::read(path).unwrap()).unwrap();
        assert!(matches!(
            detect_outcome_for_mode(&screen, GameMode::UnionCampaign).unwrap(),
            Some((Outcome::UnionFloorReturned, _))
        ));
    }

    #[test]
    fn original_fallback_does_not_swallow_cancel_or_adb_failure() {
        let mut cancelled = FakeResultDriver::new(ResultCheck::NoDefeat, false);
        cancelled.cancelled = true;
        assert!(handle_ambiguous_result(&mut cancelled).is_err());
        assert_eq!(cancelled.steps, vec!["wait-2-seconds"]);
        let mut disconnected = FakeResultDriver::new(ResultCheck::NoDefeat, false);
        disconnected.next_error = true;
        assert!(handle_ambiguous_result(&mut disconnected).is_err());
    }

    #[test]
    fn optional_match_timeout_is_not_a_cancel_error() {
        assert!(poll_for_match(
            "not-a-device",
            &["next.png"],
            lower_screen(),
            Duration::ZERO,
            &AtomicBool::new(false)
        )
        .unwrap()
        .is_none());
        assert!(poll_for_match(
            "not-a-device",
            &["next.png"],
            lower_screen(),
            Duration::ZERO,
            &AtomicBool::new(true)
        )
        .is_err());
    }

    #[test]
    fn defeat_indicators_take_priority_over_the_shared_result_icon() {
        for (path, expected) in [
            ("retry.png", Outcome::Retry),
            ("battle/power_up.png", Outcome::PowerUp),
        ] {
            let screen = result_screen(&["battle/result.png", path]);
            assert_eq!(
                detect_decisive_outcome(&screen).unwrap().unwrap().0,
                expected
            );
        }
    }

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
                    skip_manual_formations: false,
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
