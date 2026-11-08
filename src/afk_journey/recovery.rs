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
const MAX_CONNECTION_PROBES: u32 = 15;
const MAX_TRANSPORT_RECOVERIES: u32 = 3;
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
    restart_reason: Option<String>,
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
                restart_reason: None,
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

/// Navigation may be stuck while the game's Android process remains healthy.
/// Keep the readable error separate from the explicit restart request so
/// validation errors and purchase refusals never implicitly retry an action.
pub(super) fn restart_required(serial: &str, reason: String) -> String {
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut().filter(|s| s.serial == serial) {
            s.restart_reason = Some(reason.clone());
        }
    });
    reason
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
        if let Some(reason) = &s.restart_reason {
            return Err(reason.clone());
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
    if probe && adb::game_state(serial).map_err(|e| e.to_string())? != adb::GameState::Running {
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
            s.restart_reason = None;
            s.suspended = true;
            s.last_probe = None;
        }
    });
    // Also clear a surviving background process when Android shows Home.
    adb::stop_afk_journey(serial).map_err(|e| e.to_string())?;
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    // Give the newly launched game time to finish its loading/login animation.
    // No coordinates are sent during this wait. Stop remains responsive.
    for _ in 0..15 {
        common::settle_cancellable(cancelled)?;
    }
    if adb::game_state(serial).map_err(|e| e.to_string())? != adb::GameState::Running {
        return Err(game_crashed().into());
    }
    expect_running(serial);
    Ok(())
}

trait RecoveryDriver {
    fn probe(&mut self) -> Result<adb::GameState, String>;
    fn reconnect(&mut self) -> Result<(), String>;
    fn settle(&mut self) -> Result<(), String>;
    fn restart(&mut self) -> Result<(), String>;
}

struct AdbRecovery<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
}
impl RecoveryDriver for AdbRecovery<'_> {
    fn probe(&mut self) -> Result<adb::GameState, String> {
        adb::game_state(self.serial).map_err(|e| e.to_string())
    }
    fn reconnect(&mut self) -> Result<(), String> {
        adb::reconnect(self.serial).map_err(|e| e.to_string())
    }
    fn settle(&mut self) -> Result<(), String> {
        common::settle_cancellable(self.cancelled)
    }
    fn restart(&mut self) -> Result<(), String> {
        restart(self.serial, self.cancelled)
    }
}

fn connected_state(
    driver: &mut impl RecoveryDriver,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<adb::GameState, String> {
    let mut last_error = String::new();
    for attempt in 0..MAX_CONNECTION_PROBES {
        common::ensure_not_cancelled(cancelled)?;
        match driver.probe() {
            Ok(state) => return Ok(state),
            Err(error) if adb::is_transport_error(&error) => {
                last_error = error;
                if attempt == 0 {
                    log(text("ADB connection interrupted. Reconnecting before checking the game.", "ADB-Verbindung unterbrochen. Verbindung wird vor der Spielprüfung wiederhergestellt.").into());
                }
                if attempt + 1 == MAX_CONNECTION_PROBES {
                    break;
                }
                if attempt % 3 == 0 {
                    if let Err(error) = driver.reconnect() {
                        if !adb::is_transport_error(&error) {
                            return Err(error);
                        }
                    }
                }
                driver.settle()?;
                driver.settle()?;
            }
            Err(error) => return Err(error),
        }
    }
    Err(format_text!(
        "ADB is still unavailable; recovery stopped: {last_error}",
        "ADB ist weiterhin nicht erreichbar; Wiederherstellung beendet: {last_error}"
    ))
}

fn ready_after_recovery(serial: &str) {
    SESSION.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.crashed = false;
            s.restart_reason = None;
            s.suspended = false;
        }
    });
    expect_running(serial);
}

fn supervised_with_driver(
    serial: &str,
    cancelled: &AtomicBool,
    mut execute: impl FnMut(Checkpoint, &mut dyn FnMut(ActivityEvent)) -> Result<u32, String>,
    driver: &mut impl RecoveryDriver,
    mut log: impl FnMut(String),
    mut report: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    let _guard = Guard::install(serial);
    let mut pending = None;
    let mut pending_clear = None;
    let mut total = 0u32;
    let mut restart_count = 0;
    let mut transport_recoveries = 0;
    loop {
        common::ensure_not_cancelled(cancelled)?;
        let (_, checkpoint) = state();
        let result = execute(checkpoint, &mut |event| {
            match event {
                ActivityEvent::Collected(_, count) => {
                    total = total.saturating_add(count);
                    transport_recoveries = 0;
                }
                ActivityEvent::Swept(_, _) => transport_recoveries = 0,
                ActivityEvent::ModeOpening(mode) | ActivityEvent::ModeEntered(mode) => {
                    SESSION.with(|slot| {
                        if let Some(s) = slot.borrow_mut().as_mut() {
                            s.checkpoint.mode = Some(mode);
                            if matches!(event, ActivityEvent::ModeEntered(_)) {
                                s.expected_running = true;
                                s.suspended = false;
                            }
                        }
                    });
                }
                ActivityEvent::BattleStarted(mode) => {
                    pending = Some(mode);
                    pending_clear = None;
                }
                ActivityEvent::BattleFinished(mode, outcome) if pending == Some(mode) => {
                    pending = None;
                    if !matches!(outcome, BattleOutcome::Unclear) {
                        transport_recoveries = 0;
                    }
                    if mode.is_pvp() && matches!(outcome, BattleOutcome::Won) {
                        total = total.saturating_add(1);
                    } else if matches!(outcome, BattleOutcome::Won) {
                        pending_clear = Some(mode);
                    }
                }
                ActivityEvent::StageCleared(mode) if pending_clear == Some(mode) => {
                    total = total.saturating_add(1);
                    pending_clear = None;
                    transport_recoveries = 0;
                }
                _ => {}
            }
            report(event);
        });
        common::ensure_not_cancelled(cancelled)?;
        let transport_failed = matches!(&result, Err(error) if adb::is_transport_error(error));
        let should_probe = transport_failed
            || SESSION.with(|slot| {
                slot.borrow().as_ref().is_some_and(|s| {
                    s.crashed || s.restart_reason.is_some() || (s.expected_running && !s.suspended)
                })
            });
        // Do not discard a failed process probe after an action error.
        let health = if should_probe {
            connected_state(driver, cancelled, &mut log)?
        } else {
            adb::GameState::Running
        };
        let (crashed, checkpoint) = state();
        let restart_reason = SESSION.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|s| s.restart_reason.clone())
        });
        let needs_restart =
            crashed || health != adb::GameState::Running || restart_reason.is_some();
        if !needs_restart && transport_failed {
            if let Some(mode) = pending.take() {
                report(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
            }
            if transport_recoveries == MAX_TRANSPORT_RECOVERIES {
                return Err(text("ADB kept interrupting the action. Recovery stopped after three retries without confirmed progress.", "ADB unterbricht die Aktion weiterhin. Nach drei Wiederholungen ohne bestätigten Fortschritt wurde die Wiederherstellung beendet.").into());
            }
            transport_recoveries += 1;
            ready_after_recovery(serial);
            log(text(
                "ADB connection restored. Reopening the current game mode.",
                "ADB-Verbindung wiederhergestellt. Der aktuelle Spielmodus wird erneut geöffnet.",
            )
            .into());
            continue;
        }
        if !needs_restart {
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
        loop {
            common::ensure_not_cancelled(cancelled)?;
            if restart_count == MAX_RESTARTS {
                return Err(text("Automatic recovery failed after three game restarts.", "Die automatische Wiederherstellung ist nach drei Spielneustarts fehlgeschlagen.").into());
            }
            restart_count += 1;
            if let Some(reason) = &restart_reason {
                log(reason.clone());
            }
            log(format_text!(
                "Restarting the game ({}/{}). Resuming {}.",
                "Spiel wird neu gestartet ({}/{}); fortsetzen bei {}.",
                restart_count,
                MAX_RESTARTS,
                checkpoint
                    .mode
                    .map(GameMode::label)
                    .unwrap_or(text("the selected action", "der ausgewählten Aktion"))
            ));
            match driver.restart() {
                Ok(()) => {
                    ready_after_recovery(serial);
                    break;
                }
                Err(error) if adb::is_transport_error(&error) => {
                    // A launch can succeed even if its response is lost.
                    if connected_state(driver, cancelled, &mut log)? == adb::GameState::Running {
                        ready_after_recovery(serial);
                        break;
                    }
                }
                Err(error) if error == game_crashed() => {}
                Err(error) => return Err(error),
            }
        }
    }
}

pub fn run(
    serial: &str,
    cancelled: &AtomicBool,
    execute: impl FnMut(Checkpoint, &mut dyn FnMut(ActivityEvent)) -> Result<u32, String>,
    log: impl FnMut(String),
    report: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    supervised_with_driver(
        serial,
        cancelled,
        execute,
        &mut AdbRecovery { serial, cancelled },
        log,
        report,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakeRecovery<F> {
        restart: F,
        states: VecDeque<Result<adb::GameState, String>>,
        fallback: adb::GameState,
        reconnects: u32,
        pauses: u32,
        probes: u32,
        stop_on_pause: Option<std::sync::Arc<AtomicBool>>,
    }
    impl<F: FnMut() -> Result<(), String>> RecoveryDriver for FakeRecovery<F> {
        fn probe(&mut self) -> Result<adb::GameState, String> {
            self.probes += 1;
            self.states.pop_front().unwrap_or(Ok(self.fallback))
        }
        fn reconnect(&mut self) -> Result<(), String> {
            self.reconnects += 1;
            Ok(())
        }
        fn settle(&mut self) -> Result<(), String> {
            self.pauses += 1;
            if let Some(stop) = &self.stop_on_pause {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                common::ensure_not_cancelled(stop)?;
            }
            Ok(())
        }
        fn restart(&mut self) -> Result<(), String> {
            (self.restart)()
        }
    }
    fn driver<F: FnMut() -> Result<(), String>>(restart: F) -> FakeRecovery<F> {
        FakeRecovery {
            restart,
            states: VecDeque::new(),
            fallback: adb::GameState::Running,
            reconnects: 0,
            pauses: 0,
            probes: 0,
            stop_on_pause: None,
        }
    }
    // Existing orchestration tests never invoke a real ADB process.
    fn supervised(
        serial: &str,
        cancelled: &AtomicBool,
        execute: impl FnMut(Checkpoint, &mut dyn FnMut(ActivityEvent)) -> Result<u32, String>,
        restart: impl FnMut() -> Result<(), String>,
        log: impl FnMut(String),
        report: impl FnMut(ActivityEvent),
    ) -> Result<u32, String> {
        supervised_with_driver(
            serial,
            cancelled,
            execute,
            &mut driver(restart),
            log,
            report,
        )
    }

    #[test]
    fn nested_closed_errors_reconnect_then_resume_the_same_legend_tower() {
        let mode = GameMode::SeasonLegendTrial(LegendTower::Graveborn);
        let mut restarts = 0;
        let mut calls = 0;
        let mut d = driver(|| {
            restarts += 1;
            Ok(())
        });
        d.states = [
            Err("ADB command failed: error: closed".into()),
            Err("error: device offline".into()),
            Ok(adb::GameState::Missing),
            Ok(adb::GameState::Running),
        ]
        .into();
        let result = supervised_with_driver(
            "test",
            &AtomicBool::new(false),
            |checkpoint, report| {
                calls += 1;
                if calls == 2 {
                    assert_eq!(checkpoint.mode, Some(mode));
                    assert_eq!(
                        checkpoint.towers(TowerSelection::default()).enabled,
                        [false, false, true, true]
                    );
                    return Ok(0);
                }
                report(ActivityEvent::ModeEntered(mode));
                Err("ADB command failed: ADB command failed: error: closed".into())
            },
            &mut d,
            |_| {},
            |_| {},
        );
        assert_eq!(result.unwrap(), 0);
        assert_eq!(calls, 2);
        assert_eq!(d.reconnects, 1);
        drop(d);
        assert_eq!(restarts, 1);
    }

    #[test]
    fn every_action_family_restarts_and_keeps_confirmed_progress() {
        use crate::activity::RankingScope;
        let modes = [
            GameMode::SeasonAfkStages,
            GameMode::AfkStages,
            GameMode::SeasonLegendTrial(LegendTower::Lightbearer),
            GameMode::UnionCampaign,
            GameMode::DurasTrials,
            GameMode::Arena,
            GameMode::SupremeArena,
            GameMode::AfkRewards,
            GameMode::FreeHourglasses,
            GameMode::DiamondHourglasses,
            GameMode::StageRewards,
            GameMode::SeasonRankingRewards(RankingScope::District),
            GameMode::SeasonRankingRewards(RankingScope::Server),
            GameMode::SeasonRankingRewards(RankingScope::Guild),
        ];
        for mode in modes {
            let mut restarts = 0;
            let mut calls = 0;
            let mut d = driver(|| {
                restarts += 1;
                Ok(())
            });
            d.states.push_back(Ok(adb::GameState::Missing));
            let result = supervised_with_driver(
                "test",
                &AtomicBool::new(false),
                |checkpoint, report| {
                    calls += 1;
                    if calls == 2 {
                        assert_eq!(checkpoint.mode, Some(mode));
                    }
                    report(ActivityEvent::ModeEntered(mode));
                    if mode.is_collection() {
                        report(ActivityEvent::Collected(mode, 1));
                    } else {
                        report(ActivityEvent::BattleStarted(mode));
                        report(ActivityEvent::BattleFinished(mode, BattleOutcome::Won));
                        if !mode.is_pvp() {
                            report(ActivityEvent::StageCleared(mode));
                        }
                    }
                    if calls == 1 {
                        Err("ADB command failed: error: closed".into())
                    } else {
                        Ok(1)
                    }
                },
                &mut d,
                |_| {},
                |_| {},
            );
            assert_eq!(result.unwrap(), 2, "{mode:?}");
            assert_eq!(calls, 2);
            drop(d);
            assert_eq!(restarts, 1);
        }
    }

    #[test]
    fn stuck_arena_result_restarts_a_running_game_and_keeps_budget_and_wins() {
        for mode in [GameMode::Arena, GameMode::SupremeArena] {
            let mut restarts = 0;
            let mut calls = 0;
            let mut activity = crate::activity::RunActivity::default();
            activity.start(mode, Instant::now());
            let result = supervised(
                "test",
                &AtomicBool::new(false),
                |checkpoint, report| {
                    calls += 1;
                    report(ActivityEvent::ModeEntered(mode));
                    if calls == 1 {
                        report(ActivityEvent::BattleStarted(mode));
                        report(ActivityEvent::BattleFinished(mode, BattleOutcome::Won));
                        report(ActivityEvent::BattleStarted(mode));
                        report(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
                        save_arena_cursor(
                            "test",
                            Cursor {
                                initial: 2,
                                ..Cursor::default()
                            },
                        );
                        return Err(restart_required(
                            "test",
                            "The Arena result screen could not be closed.".into(),
                        ));
                    }
                    assert_eq!(checkpoint.mode, Some(mode));
                    assert_eq!(checkpoint.arena.initial, 2);
                    report(ActivityEvent::BattleStarted(mode));
                    report(ActivityEvent::BattleFinished(mode, BattleOutcome::Won));
                    Ok(1)
                },
                || {
                    restarts += 1;
                    Ok(())
                },
                |_| {},
                |event| activity.apply(event),
            );
            assert_eq!(result.unwrap(), 2);
            assert_eq!((calls, restarts), (2, 1));
            assert_eq!(activity.modes[&mode].attempts, 3);
            assert_eq!(activity.modes[&mode].wins, 2);
            assert_eq!(activity.modes[&mode].cleared, 0);
        }
    }

    #[test]
    fn navigation_restarts_are_bounded_and_stop_takes_priority() {
        for stop in [false, true] {
            let cancelled = AtomicBool::new(false);
            let mut calls = 0;
            let mut restarts = 0;
            let result = supervised(
                "test",
                &cancelled,
                |_, report| {
                    calls += 1;
                    report(ActivityEvent::ModeEntered(GameMode::SupremeArena));
                    let error = restart_required("test", "result still stuck".into());
                    assert!(check_process("test").is_err());
                    assert!(check_process("unrelated").is_ok());
                    cancelled.store(stop, std::sync::atomic::Ordering::Relaxed);
                    Err(error)
                },
                || {
                    restarts += 1;
                    Ok(())
                },
                |_| {},
                |_| {},
            );
            assert!(result.is_err());
            assert_eq!(restarts, if stop { 0 } else { MAX_RESTARTS });
            assert_eq!(calls, if stop { 1 } else { MAX_RESTARTS + 1 });
        }
    }

    #[test]
    fn launcher_triggers_restart_even_when_the_process_survives() {
        let mut restarts = 0;
        let mut calls = 0;
        let mut d = driver(|| {
            restarts += 1;
            Ok(())
        });
        d.states.push_back(Ok(adb::GameState::Launcher));
        supervised_with_driver(
            "test",
            &AtomicBool::new(false),
            |_, report| {
                calls += 1;
                report(ActivityEvent::ModeEntered(GameMode::UnionCampaign));
                if calls == 1 {
                    Err("Could not detect the next floor".into())
                } else {
                    Ok(0)
                }
            },
            &mut d,
            |_| {},
            |_| {},
        )
        .unwrap();
        drop(d);
        assert_eq!((calls, restarts), (2, 1));
    }

    #[test]
    fn live_game_after_transient_transport_loss_resumes_without_duplicate_win() {
        let mode = GameMode::AfkStages;
        let mut activity = crate::activity::RunActivity::default();
        activity.start(mode, Instant::now());
        let mut calls = 0;
        let mut d = driver(|| panic!("a healthy foreground game does not need a restart"));
        let result = supervised_with_driver(
            "test",
            &AtomicBool::new(false),
            |_, report| {
                calls += 1;
                report(ActivityEvent::ModeEntered(mode));
                report(ActivityEvent::BattleStarted(mode));
                if calls == 1 {
                    return Err("ADB command failed: error: closed".into());
                }
                report(ActivityEvent::BattleFinished(mode, BattleOutcome::Won));
                report(ActivityEvent::StageCleared(mode));
                Ok(1)
            },
            &mut d,
            |_| {},
            |e| activity.apply(e),
        );
        assert_eq!(result.unwrap(), 1);
        assert_eq!(activity.modes[&mode].attempts, 2);
        assert_eq!(activity.modes[&mode].wins, 1);
        assert_eq!(activity.modes[&mode].cleared, 1);
    }

    #[test]
    fn offline_wait_and_stop_are_bounded_and_never_send_game_input() {
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let mut d = driver(|| panic!("offline devices cannot be restarted"));
        d.states = (0..MAX_CONNECTION_PROBES)
            .map(|_| Err("error: device offline".into()))
            .collect();
        assert!(connected_state(&mut d, &cancelled, &mut |_| {}).is_err());
        assert_eq!(d.probes, MAX_CONNECTION_PROBES);
        assert_eq!(d.reconnects, 5);
        let mut d = driver(|| panic!("Stop prevents restart"));
        d.states.push_back(Err("error: closed".into()));
        d.stop_on_pause = Some(cancelled.clone());
        assert!(connected_state(&mut d, &cancelled, &mut |_| {}).is_err());
        assert_eq!(d.probes, 1);
    }

    #[test]
    fn lost_launch_response_checks_if_the_game_already_started() {
        let mut restarts = 0;
        let mut calls = 0;
        let mut d = driver(|| {
            restarts += 1;
            Err("error: closed".into())
        });
        d.states = [Ok(adb::GameState::Missing), Ok(adb::GameState::Running)].into();
        supervised_with_driver(
            "test",
            &AtomicBool::new(false),
            |_, report| {
                calls += 1;
                report(ActivityEvent::ModeEntered(GameMode::Arena));
                if calls == 1 {
                    Err("error: closed".into())
                } else {
                    Ok(0)
                }
            },
            &mut d,
            |_| {},
            |_| {},
        )
        .unwrap();
        drop(d);
        assert_eq!((calls, restarts), (2, 1));
    }
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
