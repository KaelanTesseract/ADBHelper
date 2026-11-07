//! Per-worker recovery for game crashes and missing battle results.
use super::{
    afk_stages::{self as common, StageSelection, TeamCursor},
    arena::Cursor,
    legend_trial::TowerSelection,
};
use crate::localization::{format_text, text};
use crate::{
    activity::{ActivityEvent, BattleOutcome, GameMode, LegendTower},
    adb,
};
use std::{
    cell::RefCell,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const MAX_RESTARTS: u32 = 3;
#[derive(Clone, Copy, Debug, Default)]
pub struct Checkpoint {
    pub mode: Option<GameMode>,
    pub arena: Cursor,
}
impl Checkpoint {
    pub fn stages(self, selected: StageSelection) -> StageSelection {
        if self.mode == Some(GameMode::AfkStages) {
            StageSelection {
                season: false,
                ..selected
            }
        } else {
            selected
        }
    }
    pub fn towers(self, mut selected: TowerSelection) -> TowerSelection {
        if let Some(GameMode::SeasonLegendTrial(current)) = self.mode {
            for (index, tower) in LegendTower::ALL.iter().enumerate() {
                if *tower == current {
                    break;
                }
                selected.enabled[index] = false;
            }
        }
        selected
    }
}
struct Session {
    serial: String,
    expected_running: bool,
    suspended: bool,
    crashed: bool,
    last_probe: Option<Instant>,
    checkpoint: Checkpoint,
    team: Option<(GameMode, TeamCursor)>,
}
thread_local! { static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) }; }
struct Guard(Option<Session>);
impl Guard {
    fn install(serial: &str) -> Self {
        Self(SESSION.with(|slot| {
            slot.replace(Some(Session {
                serial: serial.into(),
                expected_running: false,
                suspended: false,
                crashed: false,
                last_probe: None,
                checkpoint: Checkpoint::default(),
                team: None,
            }))
        }))
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        SESSION.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

pub(super) fn save_team_cursor(mode: GameMode, cursor: TeamCursor) {
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().as_mut() {
            session.team = Some((mode, cursor));
        }
    });
}

pub(super) fn take_team_cursor(mode: GameMode) -> Option<TeamCursor> {
    SESSION.with(|slot| {
        slot.borrow_mut()
            .as_mut()?
            .team
            .take()
            .filter(|(saved_mode, _)| *saved_mode == mode)
            .map(|(_, cursor)| cursor)
    })
}

pub fn save_arena_cursor(serial: &str, cursor: Cursor) {
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut().filter(|s| s.serial == serial) {
            s.checkpoint.arena = cursor;
        }
    });
}
pub fn arena_cursor(serial: &str) -> Cursor {
    SESSION.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|s| s.serial == serial)
            .map(|s| s.checkpoint.arena)
            .unwrap_or_default()
    })
}
pub fn expect_running(serial: &str) {
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut().filter(|s| s.serial == serial) {
            s.expected_running = true;
            s.suspended = false;
            s.last_probe = None;
        }
    });
}
pub fn intentional_stop(serial: &str) {
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut().filter(|s| s.serial == serial) {
            s.suspended = true;
        }
    });
}
pub fn check_process(serial: &str) -> Result<(), String> {
    let probe = SESSION.with(|slot| -> Result<bool, String> {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut().filter(|s| s.serial == serial) else {
            return Ok(false);
        };
        if s.crashed {
            return Err(game_crashed().into());
        }
        if s.suspended
            || !s.expected_running
            || s.last_probe
                .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return Ok(false);
        }
        s.last_probe = Some(Instant::now());
        Ok(true)
    })?;
    if probe && !adb::afk_journey_running(serial).map_err(|e| e.to_string())? {
        SESSION.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut() {
                s.crashed = true;
            }
        });
        return Err(game_crashed().into());
    }
    Ok(())
}

fn game_crashed() -> &'static str {
    text(
        "AFK Journey exited unexpectedly.",
        "AFK Journey wurde unerwartet beendet.",
    )
}

fn state() -> (bool, Checkpoint) {
    SESSION.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|s| (s.crashed, s.checkpoint))
            .unwrap_or_default()
    })
}
fn restart(serial: &str, cancelled: &AtomicBool) -> Result<(), String> {
    common::ensure_not_cancelled(cancelled)?;
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.crashed = false;
            s.suspended = true;
            s.last_probe = None;
        }
    });
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    // Give the newly launched game time to finish its loading/login animation.
    // No coordinates are sent during this wait. Stop remains responsive.
    for _ in 0..15 {
        common::settle_cancellable(cancelled)?;
    }
    if !adb::afk_journey_running(serial).map_err(|e| e.to_string())? {
        return Err(text(
            "AFK Journey could not restart after the crash.",
            "AFK Journey konnte nach dem Absturz nicht gestartet werden.",
        )
        .into());
    }
    expect_running(serial);
    Ok(())
}

fn supervised(
    serial: &str,
    cancelled: &AtomicBool,
    mut execute: impl FnMut(Checkpoint, &mut dyn FnMut(ActivityEvent)) -> Result<u32, String>,
    mut restart: impl FnMut() -> Result<(), String>,
    mut log: impl FnMut(String),
    mut report: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    let _guard = Guard::install(serial);
    let mut pending = None;
    let mut pending_clear = None;
    let mut total = 0u32;
    let mut restart_count = 0;
    loop {
        common::ensure_not_cancelled(cancelled)?;
        let (_, checkpoint) = state();
        let result = execute(checkpoint, &mut |event| {
            match event {
                ActivityEvent::Collected(_, count) => {
                    total = total.saturating_add(count);
                }
                ActivityEvent::ModeOpening(mode) | ActivityEvent::ModeEntered(mode) => {
                    SESSION.with(|slot| {
                        if let Some(s) = slot.borrow_mut().as_mut() {
                            s.checkpoint.mode = Some(mode);
                        }
                    });
                }
                ActivityEvent::BattleStarted(mode) => {
                    pending = Some(mode);
                    pending_clear = None;
                }
                ActivityEvent::BattleFinished(mode, outcome) if pending == Some(mode) => {
                    pending = None;
                    if mode.is_pvp() && matches!(outcome, BattleOutcome::Won) {
                        total = total.saturating_add(1);
                    } else if matches!(outcome, BattleOutcome::Won) {
                        pending_clear = Some(mode);
                    }
                }
                ActivityEvent::StageCleared(mode) if pending_clear == Some(mode) => {
                    total = total.saturating_add(1);
                    pending_clear = None;
                }
                _ => {}
            }
            report(event);
        });
        common::ensure_not_cancelled(cancelled)?;
        if result.is_err() {
            SESSION.with(|slot| {
                if let Some(s) = slot.borrow_mut().as_mut() {
                    s.last_probe = None;
                }
            });
            let _ = check_process(serial);
        }
        let (crashed, checkpoint) = state();
        if !crashed {
            if matches!(&result, Err(error) if error == super::BATTLE_RESULT_TIMEOUT) {
                if let Some(mode) = pending.take() {
                    report(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
                }
                let mode = checkpoint
                    .mode
                    .map(GameMode::label)
                    .unwrap_or(text("the selected action", "der ausgewählten Aktion"));
                log(format_text!(
                    "No battle result after 100 seconds. Reopening {mode}.",
                    "Kein Kampfergebnis nach 100 Sekunden. {mode} wird erneut geöffnet."
                ));
                continue;
            }
            return result.map(|_| total);
        }
        if let Some(mode) = pending.take() {
            report(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
        }
        if restart_count == MAX_RESTARTS {
            return Err(text("AFK Journey kept crashing. The run stopped after three restarts.", "AFK Journey ist wiederholt abgestürzt. Nach drei Neustarts wurde der Lauf beendet.").into());
        }
        restart_count += 1;
        log(format_text!(
            "Game crash detected. Restart {}/{}. Resuming {}.",
            "Spielabsturz erkannt. Neustart {}/{}; fortsetzen bei {}.",
            restart_count,
            MAX_RESTARTS,
            checkpoint
                .mode
                .map(GameMode::label)
                .unwrap_or(text("the selected action", "der ausgewählten Aktion"))
        ));
        restart()?;
    }
}

pub fn run(
    serial: &str,
    cancelled: &AtomicBool,
    execute: impl FnMut(Checkpoint, &mut dyn FnMut(ActivityEvent)) -> Result<u32, String>,
    log: impl FnMut(String),
    report: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    supervised(
        serial,
        cancelled,
        execute,
        || restart(serial, cancelled),
        log,
        report,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn crash() {
        SESSION.with(|slot| slot.borrow_mut().as_mut().unwrap().crashed = true);
    }

    #[test]
    fn result_timeout_reopens_the_current_mode_and_stop_prevents_another_retry() {
        use std::sync::atomic::Ordering;

        let cancelled = AtomicBool::new(false);
        let mut calls = 0;
        let mode = GameMode::AfkStages;
        let mut activity = crate::activity::RunActivity::default();
        activity.start(GameMode::SeasonAfkStages, Instant::now());
        let result = supervised(
            "test",
            &cancelled,
            |checkpoint, report| {
                calls += 1;
                if calls == 2 {
                    assert_eq!(checkpoint.mode, Some(mode));
                    assert!(!checkpoint.stages(StageSelection::default()).season);
                    cancelled.store(true, Ordering::Relaxed);
                }
                report(ActivityEvent::ModeEntered(mode));
                report(ActivityEvent::BattleStarted(mode));
                report(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
                Err(super::super::BATTLE_RESULT_TIMEOUT.into())
            },
            || panic!("an in-game screen change must not relaunch the game"),
            |_| {},
            |event| activity.apply(event),
        );
        assert!(result.is_err());
        assert_eq!(calls, 2);
        assert_eq!(activity.modes[&mode].attempts, 2);
        assert_eq!(activity.modes[&mode].cleared, 0);
    }

    #[test]
    fn resumes_the_current_mode_preserves_counters_and_closes_an_interrupted_attempt() {
        let mut calls = 0;
        let mut restarts = 0;
        let mut activity = crate::activity::RunActivity::default();
        activity.start(GameMode::SeasonAfkStages, Instant::now());
        let result = supervised(
            "test",
            &AtomicBool::new(false),
            |checkpoint, report| {
                calls += 1;
                if calls == 1 {
                    report(ActivityEvent::ModeEntered(GameMode::SeasonAfkStages));
                    report(ActivityEvent::BattleStarted(GameMode::SeasonAfkStages));
                    report(ActivityEvent::BattleFinished(
                        GameMode::SeasonAfkStages,
                        BattleOutcome::Won,
                    ));
                    report(ActivityEvent::StageCleared(GameMode::SeasonAfkStages));
                    report(ActivityEvent::ModeEntered(GameMode::AfkStages));
                    report(ActivityEvent::BattleStarted(GameMode::AfkStages));
                    crash();
                    Err(game_crashed().into())
                } else {
                    assert_eq!(checkpoint.mode, Some(GameMode::AfkStages));
                    assert!(!checkpoint.stages(StageSelection::default()).season);
                    report(ActivityEvent::ModeEntered(GameMode::AfkStages));
                    report(ActivityEvent::BattleStarted(GameMode::AfkStages));
                    report(ActivityEvent::BattleFinished(
                        GameMode::AfkStages,
                        BattleOutcome::Won,
                    ));
                    report(ActivityEvent::StageCleared(GameMode::AfkStages));
                    Ok(1)
                }
            },
            || {
                restarts += 1;
                SESSION.with(|slot| slot.borrow_mut().as_mut().unwrap().crashed = false);
                Ok(())
            },
            |_| {},
            |event| activity.apply(event),
        );
        assert_eq!(result.unwrap(), 2);
        assert_eq!((calls, restarts), (2, 1));
        assert_eq!(activity.modes[&GameMode::SeasonAfkStages].cleared, 1);
        assert_eq!(activity.modes[&GameMode::AfkStages].attempts, 2);
        assert_eq!(activity.modes[&GameMode::AfkStages].cleared, 1);
    }
    #[test]
    fn adb_errors_do_not_restart_and_repeated_crashes_are_bounded() {
        let mut restarts = 0;
        assert!(supervised(
            "test",
            &AtomicBool::new(false),
            |_, _| Err("device offline".into()),
            || {
                restarts += 1;
                Ok(())
            },
            |_| {},
            |_| {}
        )
        .is_err());
        assert_eq!(restarts, 0);
        assert!(supervised(
            "test",
            &AtomicBool::new(false),
            |_, _| {
                crash();
                Err(game_crashed().into())
            },
            || {
                restarts += 1;
                SESSION.with(|slot| slot.borrow_mut().as_mut().unwrap().crashed = false);
                Ok(())
            },
            |_| {},
            |_| {}
        )
        .is_err());
        assert_eq!(restarts, 3);
    }
    #[test]
    fn stop_before_recovery_prevents_relaunch_and_cursor_survives_a_crash() {
        let cancelled = AtomicBool::new(false);
        let mut calls = 0;
        assert!(supervised(
            "test",
            &cancelled,
            |_, _| {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                crash();
                Err(game_crashed().into())
            },
            || {
                calls += 1;
                Ok(())
            },
            |_| {},
            |_| {}
        )
        .is_err());
        assert_eq!(calls, 0);
        let cursor = Cursor {
            initial: 3,
            claims: 1,
            pending_extra: true,
            ..Cursor::default()
        };
        let mut calls = 0;
        supervised(
            "test",
            &AtomicBool::new(false),
            |checkpoint, _| {
                calls += 1;
                if calls == 1 {
                    save_arena_cursor("test", cursor);
                    crash();
                    Err(game_crashed().into())
                } else {
                    assert_eq!(checkpoint.arena, cursor);
                    Ok(0)
                }
            },
            || {
                SESSION.with(|slot| slot.borrow_mut().as_mut().unwrap().crashed = false);
                Ok(())
            },
            |_| {},
            |_| {},
        )
        .unwrap();
    }
    #[test]
    fn legend_recovery_skips_previously_finished_towers() {
        let selected = Checkpoint {
            mode: Some(GameMode::SeasonLegendTrial(LegendTower::Graveborn)),
            ..Checkpoint::default()
        }
        .towers(TowerSelection::default());
        assert_eq!(selected.enabled, [false, false, true, true]);
    }
    #[test]
    fn a_detected_crash_blocks_further_input_until_recovery() {
        let _guard = Guard::install("test");
        crash();
        assert!(check_process("test").is_err());
        assert!(check_process("unrelated").is_ok());
    }
}
