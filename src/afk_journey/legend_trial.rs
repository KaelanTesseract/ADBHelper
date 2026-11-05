//! Port of AdbAutoPlayer's SeasonLegendTrial: four faction towers, current
//! floor selection, shared team attempts, and mode-specific victory navigation.
use super::afk_stages::{self as common, BattleSettings, Match, Rect, StageResult};
use crate::activity::{ActivityEvent, GameMode, LegendTower};
use crate::adb;
use image::GrayImage;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const SELECT_MARKERS: &[&str] = &[
    "legend_trials/s_header.png",
    "legend_trials/banner_lightbearer.png",
    "legend_trials/banner_wilder.png",
    "legend_trials/banner_graveborn.png",
    "legend_trials/banner_mauler.png",
];
const CHALLENGES: &[&str] = &[
    "legend_trials/challenge_ch.png",
    "legend_trials/challenge_ge.png",
];
const END_MARKERS: &[&str] = &[
    "legend_trials/top_floor_reached.png",
    "legend_trials/unlocks_at_season_phase.png",
    "legend_trials/available_after.png",
];

#[derive(Clone, Copy)]
pub struct TowerSelection {
    pub enabled: [bool; 4],
}

impl Default for TowerSelection {
    fn default() -> Self {
        Self { enabled: [true; 4] }
    }
}

impl TowerSelection {
    pub fn selected(self) -> Vec<LegendTower> {
        LegendTower::ALL
            .into_iter()
            .zip(self.enabled)
            .filter_map(|(tower, enabled)| enabled.then_some(tower))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TowerEnd {
    Closed,
    Completed,
    AttemptsExhausted,
    Unavailable,
}

struct TowerReport {
    cleared: u32,
    end: TowerEnd,
}

fn run_towers(
    selection: TowerSelection,
    cancelled: &AtomicBool,
    mut execute: impl FnMut(LegendTower) -> Result<TowerReport, String>,
) -> Result<u32, String> {
    let towers = selection.selected();
    if towers.is_empty() {
        return Err(
            "Bitte unter Einstellungen mindestens einen Legend-Trial-Turm auswählen.".into(),
        );
    }
    let mut cleared = 0u32;
    for tower in towers {
        common::ensure_not_cancelled(cancelled)?;
        let report = execute(tower)?;
        common::ensure_not_cancelled(cancelled)?;
        match report.end {
            TowerEnd::Closed
            | TowerEnd::Completed
            | TowerEnd::AttemptsExhausted
            | TowerEnd::Unavailable => {
                cleared = cleared.saturating_add(report.cleared);
            }
        }
    }
    Ok(cleared)
}

fn selection_region() -> Rect {
    Rect {
        x: 108,
        y: 192,
        width: 864,
        height: 1536,
    }
}

fn tower_closed(screen: &GrayImage, tower: LegendTower) -> Result<bool, String> {
    // As in the original, the faction icon in the left strip marks today's
    // unavailable tower; the large central banner is the selectable entry.
    Ok(common::find_template(
        screen,
        &format!("legend_trials/faction_icon_{}.png", tower.slug()),
        Rect {
            x: 0,
            y: 576,
            width: 324,
            height: 1152,
        },
        0.78,
    )?
    .is_some())
}

fn navigate_to_select(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    let (screen, _) = common::screenshot(serial)?;
    if common::get_match(&screen, SELECT_MARKERS, selection_region(), 0.78)?.is_some() {
        return Ok(());
    }
    for _ in 0..3 {
        common::ensure_not_cancelled(cancelled)?;
        common::navigate_to_battle_modes(serial, cancelled, log)?;
        // Original uses at most four short, slow upward swipes to avoid flings.
        let mut entry = None;
        for swipe in 0..=4 {
            common::ensure_not_cancelled(cancelled)?;
            let (screen, size) = common::screenshot(serial)?;
            if let Some(found) = common::find_template(
                &screen,
                "battle_modes/legend_trial.png",
                common::full_screen(),
                0.75,
            )? {
                entry = Some((found, size));
                break;
            }
            if swipe < 4 {
                adb::swipe_game_points(serial, (540, 1200), (540, 700), size, 1500)
                    .map_err(|e| e.to_string())?;
                common::settle_cancellable(cancelled)?;
            }
        }
        let Some((entry, size)) = entry else { continue };
        common::tap_match(serial, entry, size)?;
        common::settle_cancellable(cancelled)?;
        if common::poll_for_match(
            serial,
            SELECT_MARKERS,
            selection_region(),
            Duration::from_secs(20),
            cancelled,
        )?
        .is_some()
        {
            return Ok(());
        }
    }
    Err("Season Legend Trial konnte nicht geöffnet werden. Bitte Freischaltung und Spielverbindung prüfen.".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FloorAction {
    End,
    Ready,
    Next,
    Challenge,
    Milestone,
    Reward,
}

fn floor_action(screen: &GrayImage) -> Result<Option<(FloorAction, Match)>, String> {
    // Overlays and limits win over buttons visible behind them.
    for (paths, action, roi) in [
        (END_MARKERS, FloorAction::End, common::full_screen()),
        (
            &["tap_to_close.png", "afk_stages/tap_to_close.png"][..],
            FloorAction::Milestone,
            common::lower_screen(),
        ),
        (
            &["next.png", "battle/next.png"][..],
            FloorAction::Next,
            common::lower_screen(),
        ),
        (
            &["battle/victory_rewards.png"][..],
            FloorAction::Reward,
            common::lower_screen(),
        ),
        (CHALLENGES, FloorAction::Challenge, selection_region()),
        (
            &["battle/records.png", "battle/battle.png"][..],
            FloorAction::Ready,
            common::lower_screen(),
        ),
    ] {
        if let Some((_, found)) = common::get_match(screen, paths, roi, 0.8)? {
            return Ok(Some((action, found)));
        }
    }
    Ok(None)
}

trait FloorDriver {
    fn observe(&mut self) -> Result<FloorAction, String>;
    fn activate(&mut self, action: FloorAction) -> Result<(), String>;
    fn select_floor(&mut self) -> Result<bool, String>;
}

fn continue_after_win(driver: &mut impl FloorDriver) -> Result<bool, String> {
    for _ in 0..12 {
        let action = driver.observe()?;
        match action {
            FloorAction::End => return Ok(false),
            FloorAction::Ready => return Ok(true),
            FloorAction::Milestone => {
                driver.activate(action)?;
                if !driver.select_floor()? {
                    return Ok(false);
                }
            }
            FloorAction::Next | FloorAction::Challenge | FloorAction::Reward => {
                driver.activate(action)?
            }
        }
    }
    Err("Die nächste Legend-Trial-Etage konnte nicht geöffnet werden.".into())
}

struct AdbFloorDriver<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    tower: LegendTower,
    found: Option<Match>,
    size: (u32, u32),
}

impl FloorDriver for AdbFloorDriver<'_> {
    fn observe(&mut self) -> Result<FloorAction, String> {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if let Some((action, found)) = floor_action(&screen)? {
                self.found = Some(found);
                self.size = size;
                return Ok(action);
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        Err("Die nächste Legend-Trial-Etage oder deren Abschluss wurde nicht erkannt.".into())
    }

    fn activate(&mut self, action: FloorAction) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        if action == FloorAction::Reward {
            adb::tap_game_point(self.serial, 550, 1800, self.size).map_err(|e| e.to_string())?;
        } else {
            common::tap_match(
                self.serial,
                self.found.take().ok_or("Legend-Trial-Schaltfläche fehlt")?,
                self.size,
            )?;
        }
        common::settle_cancellable(self.cancelled)
    }

    fn select_floor(&mut self) -> Result<bool, String> {
        common::wait_for_match(
            self.serial,
            &[&format!(
                "legend_trials/tower_icon_{}.png",
                self.tower.slug()
            )],
            Rect {
                x: 0,
                y: 0,
                width: 216,
                height: 384,
            },
            Duration::from_secs(20),
            self.cancelled,
        )?;
        let paths: Vec<_> = END_MARKERS.iter().chain(CHALLENGES).copied().collect();
        let Some((path, found, size)) = common::poll_for_match(
            self.serial,
            &paths,
            selection_region(),
            Duration::from_secs(8),
            self.cancelled,
        )?
        else {
            return Ok(false);
        };
        if END_MARKERS.contains(&path.as_str()) {
            return Ok(false);
        }
        common::tap_match(self.serial, found, size)?;
        common::settle_cancellable(self.cancelled)?;
        common::wait_for_match(
            self.serial,
            &["battle/records.png", "battle/battle.png"],
            common::lower_screen(),
            Duration::from_secs(20),
            self.cancelled,
        )?;
        Ok(true)
    }
}

pub fn run(
    serial: &str,
    settings: BattleSettings,
    selection: TowerSelection,
    cancelled: &AtomicBool,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    if selection.selected().is_empty() {
        return Err("Bitte mindestens einen Legend-Trial-Turm auswählen.".into());
    }
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    for _ in 0..3 {
        common::settle_cancellable(cancelled)?;
    }
    run_towers(selection, cancelled, |tower| {
        let mode = GameMode::SeasonLegendTrial(tower);
        progress(ActivityEvent::ModeOpening(mode));
        navigate_to_select(serial, cancelled, &mut log)?;
        common::settle_cancellable(cancelled)?;
        let (screen, _) = common::screenshot(serial)?;
        if tower_closed(&screen, tower)? {
            log(format!(
                "{} ist heute geschlossen und wird übersprungen.",
                tower.label()
            ));
            return Ok(TowerReport {
                cleared: 0,
                end: TowerEnd::Closed,
            });
        }
        let banner = format!("legend_trials/banner_{}.png", tower.slug());
        let Some((_, found, size)) = common::poll_for_match(
            serial,
            &[&banner],
            selection_region(),
            Duration::from_secs(15),
            cancelled,
        )?
        else {
            log(format!("{}-Turm ist nicht verfügbar.", tower.label()));
            return Ok(TowerReport {
                cleared: 0,
                end: TowerEnd::Unavailable,
            });
        };
        common::tap_match(serial, found, size)?;
        common::settle_cancellable(cancelled)?;
        let mut driver = AdbFloorDriver {
            serial,
            cancelled,
            tower,
            found: None,
            size,
        };
        if !driver.select_floor()? {
            log(format!("{}: Keine weitere Etage verfügbar.", tower.label()));
            return Ok(TowerReport {
                cleared: 0,
                end: TowerEnd::Completed,
            });
        }
        progress(ActivityEvent::ModeEntered(mode));
        let mut cleared = 0u32;
        loop {
            common::ensure_not_cancelled(cancelled)?;
            match common::play_one_stage(
                serial,
                &mut log,
                size,
                cancelled,
                BattleSettings { mode, ..settings },
                &mut progress,
            )? {
                StageResult::Won => {
                    cleared = cleared.saturating_add(1);
                    log(format!("{}: {cleared} Etagen geschafft.", tower.label()));
                    if !continue_after_win(&mut driver)? {
                        return Ok(TowerReport {
                            cleared,
                            end: TowerEnd::Completed,
                        });
                    }
                }
                StageResult::FormationFailed => {
                    return Ok(TowerReport {
                        cleared,
                        end: TowerEnd::AttemptsExhausted,
                    })
                }
                StageResult::FinalStage => {
                    return Ok(TowerReport {
                        cleared,
                        end: TowerEnd::Completed,
                    })
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{BattleOutcome, RunActivity};
    use std::collections::VecDeque;
    use std::sync::atomic::Ordering;

    struct FakeFloor {
        actions: VecDeque<Result<FloorAction, String>>,
        activated: Vec<FloorAction>,
        selections: usize,
        select_result: Result<bool, String>,
    }
    impl FloorDriver for FakeFloor {
        fn observe(&mut self) -> Result<FloorAction, String> {
            self.actions.pop_front().expect("unexpected observation")
        }
        fn activate(&mut self, action: FloorAction) -> Result<(), String> {
            self.activated.push(action);
            Ok(())
        }
        fn select_floor(&mut self) -> Result<bool, String> {
            self.selections += 1;
            self.select_result.clone()
        }
    }
    fn floor(actions: &[FloorAction]) -> FakeFloor {
        FakeFloor {
            actions: actions.iter().map(|a| Ok(*a)).collect(),
            activated: vec![],
            selections: 0,
            select_result: Ok(true),
        }
    }
    fn screen_with(paths: &[(&str, i64, i64)]) -> GrayImage {
        let mut screen = GrayImage::new(1080, 1920);
        for &(path, x, y) in paths {
            image::imageops::replace(&mut screen, &common::load_template(path).unwrap(), x, y);
        }
        screen
    }

    #[test]
    fn tower_order_and_exclusions_match_original() {
        let mut visited = vec![];
        let cleared = run_towers(
            TowerSelection {
                enabled: [true, false, true, true],
            },
            &AtomicBool::new(false),
            |tower| {
                visited.push(tower);
                Ok(TowerReport {
                    cleared: 2,
                    end: TowerEnd::AttemptsExhausted,
                })
            },
        )
        .unwrap();
        assert_eq!(
            visited,
            vec![
                LegendTower::Lightbearer,
                LegendTower::Graveborn,
                LegendTower::Mauler
            ]
        );
        assert_eq!(cleared, 6);
    }

    #[test]
    fn closed_and_completed_towers_do_not_prevent_trying_the_remaining_towers() {
        let mut visited = vec![];
        assert_eq!(
            run_towers(
                TowerSelection::default(),
                &AtomicBool::new(false),
                |tower| {
                    visited.push(tower);
                    Ok(TowerReport {
                        cleared: if tower == LegendTower::Wilder { 3 } else { 0 },
                        end: match tower {
                            LegendTower::Lightbearer => TowerEnd::Closed,
                            LegendTower::Wilder => TowerEnd::AttemptsExhausted,
                            LegendTower::Graveborn => TowerEnd::Completed,
                            LegendTower::Mauler => TowerEnd::Unavailable,
                        },
                    })
                }
            )
            .unwrap(),
            3
        );
        assert_eq!(visited, LegendTower::ALL);
    }

    #[test]
    fn stop_and_connection_failures_prevent_starting_another_tower() {
        let cancelled = AtomicBool::new(false);
        let mut visited = vec![];
        assert!(run_towers(TowerSelection::default(), &cancelled, |tower| {
            visited.push(tower);
            cancelled.store(true, Ordering::Relaxed);
            Ok(TowerReport {
                cleared: 1,
                end: TowerEnd::Completed,
            })
        })
        .is_err());
        assert_eq!(visited, vec![LegendTower::Lightbearer]);
        visited.clear();
        assert_eq!(
            run_towers(
                TowerSelection::default(),
                &AtomicBool::new(false),
                |tower| {
                    visited.push(tower);
                    Err("ADB getrennt".into())
                }
            )
            .unwrap_err(),
            "ADB getrennt"
        );
        assert_eq!(visited, vec![LegendTower::Lightbearer]);
    }

    #[test]
    fn next_and_challenge_buttons_are_handled_before_the_next_battle() {
        for action in [FloorAction::Next, FloorAction::Challenge] {
            let mut driver = floor(&[action, FloorAction::Ready]);
            assert!(continue_after_win(&mut driver).unwrap());
            assert_eq!(driver.activated, vec![action]);
            assert_eq!(driver.selections, 0);
        }
    }

    #[test]
    fn milestone_reward_reselects_floor_and_final_floor_stops_without_another_battle() {
        let mut driver = floor(&[
            FloorAction::Reward,
            FloorAction::Milestone,
            FloorAction::Ready,
        ]);
        assert!(continue_after_win(&mut driver).unwrap());
        assert_eq!(
            driver.activated,
            vec![FloorAction::Reward, FloorAction::Milestone]
        );
        assert_eq!(driver.selections, 1);
        let mut final_floor = floor(&[FloorAction::End]);
        assert!(!continue_after_win(&mut final_floor).unwrap());
        assert!(final_floor.activated.is_empty());
        let mut no_challenge = floor(&[FloorAction::Milestone]);
        no_challenge.select_result = Ok(false);
        assert!(!continue_after_win(&mut no_challenge).unwrap());
    }

    #[test]
    fn navigation_errors_and_stop_are_not_interpreted_as_tower_completion() {
        let mut driver = floor(&[]);
        driver.actions.push_back(Err("Cancelled by user.".into()));
        assert_eq!(
            continue_after_win(&mut driver).unwrap_err(),
            "Cancelled by user."
        );
        let mut driver = floor(&[FloorAction::Milestone]);
        driver.select_result = Err("ADB getrennt".into());
        assert_eq!(continue_after_win(&mut driver).unwrap_err(), "ADB getrennt");
        let mut stuck = floor(&[FloorAction::Challenge; 12]);
        assert!(continue_after_win(&mut stuck).is_err());
    }

    #[test]
    fn faction_icon_closes_only_its_tower_and_only_in_the_original_region() {
        for tower in LegendTower::ALL {
            let path = format!("legend_trials/faction_icon_{}.png", tower.slug());
            let screen = screen_with(&[(&path, 30, 750)]);
            assert!(tower_closed(&screen, tower).unwrap(), "{tower:?}");
            assert!(!tower_closed(&screen_with(&[(&path, 650, 750)]), tower).unwrap());
            let banner = format!("legend_trials/banner_{}.png", tower.slug());
            assert!(!tower_closed(&screen_with(&[(&banner, 350, 800)]), tower).unwrap());
        }
    }

    #[test]
    fn all_floor_controls_and_completion_markers_use_real_embedded_templates() {
        for (path, expected) in [
            ("legend_trials/top_floor_reached.png", FloorAction::End),
            (
                "legend_trials/unlocks_at_season_phase.png",
                FloorAction::End,
            ),
            ("legend_trials/available_after.png", FloorAction::End),
            ("legend_trials/challenge_ch.png", FloorAction::Challenge),
            ("legend_trials/challenge_ge.png", FloorAction::Challenge),
            ("tap_to_close.png", FloorAction::Milestone),
            ("afk_stages/tap_to_close.png", FloorAction::Milestone),
            ("next.png", FloorAction::Next),
            ("battle/next.png", FloorAction::Next),
            ("battle/victory_rewards.png", FloorAction::Reward),
            ("battle/records.png", FloorAction::Ready),
            ("battle/battle.png", FloorAction::Ready),
        ] {
            assert_eq!(
                floor_action(&screen_with(&[(path, 300, 1300)]))
                    .unwrap()
                    .unwrap()
                    .0,
                expected,
                "{path}"
            );
        }
        let overlay = screen_with(&[
            ("legend_trials/unlocks_at_season_phase.png", 280, 1300),
            ("battle/records.png", 120, 1680),
        ]);
        assert_eq!(floor_action(&overlay).unwrap().unwrap().0, FloorAction::End);
    }

    #[test]
    fn tower_counters_remain_separate_and_feed_the_existing_statistics() {
        let mut run = RunActivity::default();
        run.start(
            GameMode::SeasonLegendTrial(LegendTower::Lightbearer),
            std::time::Instant::now(),
        );
        for tower in [LegendTower::Lightbearer, LegendTower::Wilder] {
            let mode = GameMode::SeasonLegendTrial(tower);
            run.apply(ActivityEvent::ModeEntered(mode));
            run.apply(ActivityEvent::BattleStarted(mode));
            run.apply(ActivityEvent::BattleFinished(
                mode,
                if tower == LegendTower::Wilder {
                    BattleOutcome::Won
                } else {
                    BattleOutcome::Lost
                },
            ));
            if tower == LegendTower::Wilder {
                run.apply(ActivityEvent::StageCleared(mode));
            }
        }
        assert_eq!(
            run.modes[&GameMode::SeasonLegendTrial(LegendTower::Lightbearer)].cleared,
            0
        );
        assert_eq!(
            run.modes[&GameMode::SeasonLegendTrial(LegendTower::Wilder)].cleared,
            1
        );
        assert_eq!(run.outcomes(), (1, 1));
        assert_eq!(run.attempts_per_clear(), Some(1.0));
    }

    #[test]
    fn empty_selection_and_stop_do_not_touch_adb_or_emit_activity() {
        let settings = BattleSettings {
            mode: GameMode::SeasonLegendTrial(LegendTower::Lightbearer),
            use_suggested_formations: true,
            current_team_first: true,
            skip_manual_formations: false,
            max_formations: 10,
            attempts_per_formation: 5,
        };
        for (selection, cancelled) in [
            (TowerSelection::default(), true),
            (
                TowerSelection {
                    enabled: [false; 4],
                },
                false,
            ),
        ] {
            let mut events = vec![];
            assert!(run(
                "not-a-device",
                settings,
                selection,
                &AtomicBool::new(cancelled),
                |_| {},
                |e| events.push(e)
            )
            .is_err());
            assert!(events.is_empty());
        }
    }

    #[test]
    fn ending_after_a_closed_tower_keeps_the_last_played_mode_visible() {
        let mut run = RunActivity::default();
        let played = GameMode::SeasonLegendTrial(LegendTower::Graveborn);
        run.start(played, std::time::Instant::now());
        run.apply(ActivityEvent::ModeEntered(played));
        run.apply(ActivityEvent::ModeOpening(GameMode::SeasonLegendTrial(
            LegendTower::Mauler,
        )));
        run.finish(std::time::Instant::now());
        assert_eq!(run.current_mode, Some(played));
        assert!(!run
            .modes
            .contains_key(&GameMode::SeasonLegendTrial(LegendTower::Mauler)));
    }

    #[test]
    #[ignore = "requires an explicitly selected emulator; starts at most one free battle"]
    fn connected_emulator_single_stage() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        navigate_to_select(&serial, &cancelled, &mut |message| println!("{message}")).unwrap();
        let (screen, _) = common::screenshot(&serial).unwrap();
        let tower = LegendTower::ALL
            .into_iter()
            .find(|&tower| !tower_closed(&screen, tower).unwrap())
            .expect("no open tower today");
        let mode = GameMode::SeasonLegendTrial(tower);
        let banner = format!("legend_trials/banner_{}.png", tower.slug());
        let (_, found, size) = common::wait_for_match(
            &serial,
            &[&banner],
            selection_region(),
            Duration::from_secs(15),
            &cancelled,
        )
        .unwrap();
        common::tap_match(&serial, found, size).unwrap();
        common::settle_cancellable(&cancelled).unwrap();
        let mut driver = AdbFloorDriver {
            serial: &serial,
            cancelled: &cancelled,
            tower,
            found: None,
            size,
        };
        assert!(
            driver.select_floor().unwrap(),
            "no remaining floor in open tower"
        );
        println!("{} floor and battle setup opened", tower.label());
        let mut activity = RunActivity::default();
        activity.start(mode, std::time::Instant::now());
        activity.apply(ActivityEvent::ModeEntered(mode));
        let result = common::play_one_stage(
            &serial,
            &mut |m| println!("{m}"),
            size,
            &cancelled,
            BattleSettings {
                mode,
                use_suggested_formations: false,
                current_team_first: true,
                skip_manual_formations: false,
                max_formations: 1,
                attempts_per_formation: 1,
            },
            &mut |event| {
                println!("{event:?}");
                activity.apply(event);
            },
        )
        .unwrap();
        assert_eq!(activity.modes[&mode].attempts, 1);
        println!(
            "Single-stage outcome: {result:?}; attempts {}, cleared {}",
            activity.modes[&mode].attempts, activity.modes[&mode].cleared
        );
        if result == StageResult::Won {
            assert_eq!(activity.modes[&mode].cleared, 1);
            println!(
                "Next floor available: {}",
                continue_after_win(&mut driver).unwrap()
            );
        } else {
            assert_eq!(activity.modes[&mode].cleared, 0);
        }
        navigate_to_select(&serial, &cancelled, &mut |m| println!("{m}")).unwrap();
    }

    #[test]
    #[ignore = "requires an explicitly selected, connected emulator; navigates only, no battle"]
    fn connected_emulator_navigation() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        navigate_to_select(&serial, &cancelled, &mut |message| println!("{message}")).unwrap();
        let (screen, _) = common::screenshot(&serial).unwrap();
        assert!(
            common::get_match(&screen, SELECT_MARKERS, selection_region(), 0.78)
                .unwrap()
                .is_some()
        );
        for tower in LegendTower::ALL {
            println!(
                "{} closed: {}",
                tower.label(),
                tower_closed(&screen, tower).unwrap()
            );
        }
    }
}
