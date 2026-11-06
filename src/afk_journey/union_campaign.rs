//! Port of AdbAutoPlayer's UnionCampaignMixin. Sweep is deliberately absent:
//! only the current floor's Battle and post-victory Continue are activated.
use super::afk_stages::{self as common, BattleSettings, Match, Rect, StageResult};
use crate::activity::{ActivityEvent, GameMode};
use crate::adb;
use crate::localization::text;
use image::GrayImage;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn floor_label_region() -> Rect {
    Rect {
        x: 216,
        y: 1056,
        width: 648,
        height: 576,
    }
}
fn bottom_bar() -> Rect {
    Rect {
        x: 0,
        y: 1632,
        width: 1080,
        height: 288,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreBattle {
    FloorBattle,
    Formation,
    Overlay,
}

fn pre_battle_state(screen: &GrayImage) -> Result<Option<(PreBattle, Match)>, String> {
    for (paths, state, roi) in [
        (
            &[
                "guide/close.png",
                "guide/next.png",
                "battle/skip.png",
                "battle/skip_orange.png",
                "quests/skip.png",
            ][..],
            PreBattle::Overlay,
            common::full_screen(),
        ),
        (
            &["battle/records.png"][..],
            PreBattle::Formation,
            common::lower_screen(),
        ),
        (
            &["union_campaign/battle.png"][..],
            PreBattle::FloorBattle,
            common::full_screen(),
        ),
    ] {
        if let Some((_, found)) = common::get_match(screen, paths, roi, 0.8)? {
            return Ok(Some((state, found)));
        }
    }
    Ok(None)
}

pub(super) fn navigate(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    common::ensure_not_cancelled(cancelled)?;
    let (screen, _) = common::screenshot(serial)?;
    if common::find_template(
        &screen,
        "union_campaign/current_label.png",
        floor_label_region(),
        0.78,
    )?
    .is_some()
    {
        return Ok(());
    }
    for _ in 0..3 {
        common::ensure_not_cancelled(cancelled)?;
        let size = common::navigate_to_current_overview(serial, cancelled, log)?;
        adb::tap_game_point(serial, 460, 1830, size).map_err(|e| e.to_string())?;
        common::settle_cancellable(cancelled)?;
        let mut menu = None;
        for _ in 0..2 {
            menu = common::poll_for_match(
                serial,
                &[
                    "battle_modes/union_campaign.png",
                    "battle_modes/guild_mode_tab.png",
                    "popup/quick_purchase.png",
                ],
                common::full_screen(),
                Duration::from_secs(20),
                cancelled,
            )?;
            if menu
                .as_ref()
                .is_some_and(|(path, _, _)| path == "popup/quick_purchase.png")
            {
                adb::press_back(serial).map_err(|e| e.to_string())?;
                common::settle_cancellable(cancelled)?;
            } else {
                break;
            }
        }
        let Some((path, tab, size)) = menu else {
            continue;
        };
        // When Guild Mode is already active its tab pill has a different image.
        // Detect the Union card first, exactly as in the original navigation.
        if path == "battle_modes/guild_mode_tab.png" {
            let (screen, _) = common::screenshot(serial)?;
            if common::find_template(
                &screen,
                "battle_modes/union_campaign.png",
                common::full_screen(),
                0.75,
            )?
            .is_none()
            {
                common::tap_match(serial, tab, size)?;
                common::settle_cancellable(cancelled)?;
            }
        }
        for swipe in 0..=4 {
            common::ensure_not_cancelled(cancelled)?;
            let (screen, size) = common::screenshot(serial)?;
            if let Some(entry) = common::find_template(
                &screen,
                "battle_modes/union_campaign.png",
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
                            "Union Campaign is not unlocked yet (Coming Soon).",
                            "Union Campaign ist noch nicht freigeschaltet (Coming Soon).",
                        )
                        .into());
                    }
                }
                for _ in 0..3 {
                    let (screen, size) = common::screenshot(serial)?;
                    let Some(entry) = common::find_template(
                        &screen,
                        "battle_modes/union_campaign.png",
                        common::full_screen(),
                        0.75,
                    )?
                    else {
                        break;
                    };
                    common::tap_match(serial, entry, size)?;
                    common::settle_cancellable(cancelled)?;
                }
                if common::poll_for_match(
                    serial,
                    &["union_campaign/current_label.png"],
                    floor_label_region(),
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
    Err(text("Could not open Union Campaign. Check guild membership, game access, and connection.", "Union Campaign konnte nicht geöffnet werden. Bitte Gildenmitgliedschaft, Freischaltung und Spielverbindung prüfen.").into())
}

fn prepare_floor(serial: &str, cancelled: &AtomicBool) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(45) {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        match pre_battle_state(&screen)? {
            Some((PreBattle::Formation, _)) => return Ok(()),
            Some((PreBattle::FloorBattle | PreBattle::Overlay, found)) => {
                common::tap_match(serial, found, size)?;
                common::settle_cancellable(cancelled)?;
            }
            None => std::thread::sleep(Duration::from_millis(400)),
        }
    }
    Err(text(
        "The Union Campaign formation screen was not detected.",
        "Die Union-Campaign-Teamaufstellung wurde nicht erkannt.",
    )
    .into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostBattle {
    Ready,
    Continue,
    NoFurtherFloor,
}

fn post_battle_state(screen: &GrayImage) -> Result<Option<(PostBattle, Match)>, String> {
    for (path, state) in [
        ("battle/records.png", PostBattle::Ready),
        ("arena/continue.png", PostBattle::Continue),
    ] {
        if let Some(found) = common::find_template(screen, path, bottom_bar(), 0.78)? {
            return Ok(Some((state, found)));
        }
    }
    Ok(None)
}

trait PostBattleDriver {
    fn observe(&mut self) -> Result<PostBattle, String>;
    fn tap_continue(&mut self) -> Result<(), String>;
    fn wait_until_continue_disappears(&mut self) -> Result<(), String>;
}

fn advance_after_win(driver: &mut impl PostBattleDriver) -> Result<bool, String> {
    // At most three Continue taps. Observe once more after the last tap so
    // a successful third tap cannot be mistaken for the end of the campaign.
    for taps in 0..=3 {
        match driver.observe()? {
            PostBattle::Ready => return Ok(true),
            PostBattle::NoFurtherFloor => return Ok(false),
            PostBattle::Continue if taps < 3 => {
                driver.tap_continue()?;
                driver.wait_until_continue_disappears()?;
            }
            PostBattle::Continue => return Ok(false),
        }
    }
    Ok(false)
}

struct AdbPostBattle<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
    found: Option<Match>,
    size: (u32, u32),
}

impl PostBattleDriver for AdbPostBattle<'_> {
    fn observe(&mut self) -> Result<PostBattle, String> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if let Some((state, found)) = post_battle_state(&screen)? {
                self.found = Some(found);
                self.size = size;
                return Ok(state);
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        common::ensure_not_cancelled(self.cancelled)?;
        // Original's last-floor screen is unknown: only a genuine timeout ends
        // progression. Device failures and Stop remain errors, never completion.
        Ok(PostBattle::NoFurtherFloor)
    }

    fn tap_continue(&mut self) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        common::tap_match(
            self.serial,
            self.found.take().ok_or(text(
                "Continue button not detected",
                "Continue wurde nicht erkannt",
            ))?,
            self.size,
        )
    }

    fn wait_until_continue_disappears(&mut self) -> Result<(), String> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, _) = common::screenshot(self.serial)?;
            if common::find_template(&screen, "arena/continue.png", bottom_bar(), 0.78)?.is_none() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(350));
        }
        Ok(()) // Missed tap: the bounded outer loop may retry.
    }
}

pub fn run(
    serial: &str,
    settings: BattleSettings,
    cancelled: &AtomicBool,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    let mode = GameMode::UnionCampaign;
    progress(ActivityEvent::ModeOpening(mode));
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    for _ in 0..3 {
        common::settle_cancellable(cancelled)?;
    }
    navigate(serial, cancelled, &mut log)?;
    let size = adb::display_size(serial).map_err(|e| e.to_string())?;
    progress(ActivityEvent::ModeEntered(mode));
    let mut cleared = 0u32;
    loop {
        common::ensure_not_cancelled(cancelled)?;
        prepare_floor(serial, cancelled)?;
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
                log(format!("Union Campaign: {cleared} Etagen geschafft."));
                let mut driver = AdbPostBattle {
                    serial,
                    cancelled,
                    found: None,
                    size,
                };
                if !advance_after_win(&mut driver)? {
                    log("Union Campaign: Keine weitere Etage erkannt.".into());
                    return Ok(cleared);
                }
            }
            StageResult::FormationFailed | StageResult::FinalStage => return Ok(cleared),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{BattleOutcome, RunActivity};
    use std::collections::VecDeque;

    struct FakePost {
        states: VecDeque<Result<PostBattle, String>>,
        trace: Vec<&'static str>,
        disappear_error: Option<String>,
    }
    impl PostBattleDriver for FakePost {
        fn observe(&mut self) -> Result<PostBattle, String> {
            self.trace.push("observe");
            self.states
                .pop_front()
                .expect("unexpected extra observation")
        }
        fn tap_continue(&mut self) -> Result<(), String> {
            self.trace.push("tap");
            Ok(())
        }
        fn wait_until_continue_disappears(&mut self) -> Result<(), String> {
            self.trace.push("wait_gone");
            self.disappear_error.clone().map_or(Ok(()), Err)
        }
    }
    fn post(states: &[PostBattle]) -> FakePost {
        FakePost {
            states: states.iter().map(|s| Ok(*s)).collect(),
            trace: vec![],
            disappear_error: None,
        }
    }
    fn screen(paths: &[(&str, i64, i64)]) -> GrayImage {
        let mut image = GrayImage::new(1080, 1920);
        for &(path, x, y) in paths {
            image::imageops::replace(&mut image, &common::load_template(path).unwrap(), x, y);
        }
        image
    }

    #[test]
    fn next_floor_ready_does_not_tap_any_buttons() {
        let mut driver = post(&[PostBattle::Ready]);
        assert!(advance_after_win(&mut driver).unwrap());
        assert_eq!(driver.trace, vec!["observe"]);
    }

    #[test]
    fn lingering_victory_waits_for_continue_to_disappear_before_reobserving() {
        let mut driver = post(&[PostBattle::Continue, PostBattle::Ready]);
        assert!(advance_after_win(&mut driver).unwrap());
        assert_eq!(driver.trace, vec!["observe", "tap", "wait_gone", "observe"]);
    }

    #[test]
    fn third_continue_tap_can_still_load_a_real_next_floor() {
        let mut driver = post(&[
            PostBattle::Continue,
            PostBattle::Continue,
            PostBattle::Continue,
            PostBattle::Ready,
        ]);
        assert!(advance_after_win(&mut driver).unwrap());
        assert_eq!(driver.trace.iter().filter(|&&a| a == "tap").count(), 3);
        assert_eq!(
            driver.trace.iter().filter(|&&a| a == "wait_gone").count(),
            3
        );
    }

    #[test]
    fn continue_retries_are_bounded_and_a_real_timeout_ends_progression() {
        let mut driver = post(&[PostBattle::Continue; 4]);
        assert!(!advance_after_win(&mut driver).unwrap());
        assert_eq!(driver.trace.iter().filter(|&&a| a == "tap").count(), 3);
        let mut driver = post(&[PostBattle::NoFurtherFloor]);
        assert!(!advance_after_win(&mut driver).unwrap());
        assert!(!driver.trace.contains(&"tap"));
    }

    #[test]
    fn stop_and_device_errors_are_not_reported_as_campaign_completion() {
        let mut driver = post(&[]);
        driver.states.push_back(Err("ADB getrennt".into()));
        assert_eq!(advance_after_win(&mut driver).unwrap_err(), "ADB getrennt");
        let mut driver = post(&[PostBattle::Continue]);
        driver.disappear_error = Some("Cancelled by user.".into());
        assert_eq!(
            advance_after_win(&mut driver).unwrap_err(),
            "Cancelled by user."
        );
    }

    #[test]
    fn post_battle_checks_the_bottom_left_records_button_and_victory_continue() {
        // Original regression: Records is outside a centre crop after victory.
        for (path, expected, x) in [
            ("battle/records.png", PostBattle::Ready, 20),
            ("arena/continue.png", PostBattle::Continue, 430),
        ] {
            assert_eq!(
                post_battle_state(&screen(&[(path, x, 1740)]))
                    .unwrap()
                    .unwrap()
                    .0,
                expected,
                "{path}"
            );
            assert!(
                post_battle_state(&screen(&[(path, x, 1300)]))
                    .unwrap()
                    .is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn pre_battle_only_activates_battle_records_or_known_guides_and_skips() {
        for (path, expected) in [
            ("union_campaign/battle.png", PreBattle::FloorBattle),
            ("battle/records.png", PreBattle::Formation),
            ("guide/close.png", PreBattle::Overlay),
            ("guide/next.png", PreBattle::Overlay),
            ("battle/skip.png", PreBattle::Overlay),
            ("battle/skip_orange.png", PreBattle::Overlay),
            ("quests/skip.png", PreBattle::Overlay),
        ] {
            assert_eq!(
                pre_battle_state(&screen(&[(path, 300, 1450)]))
                    .unwrap()
                    .unwrap_or_else(|| panic!(
                        "Template not recognized: {path}; best {:?}",
                        common::find_template(
                            &screen(&[(path, 300, 1450)]),
                            path,
                            common::full_screen(),
                            0.0
                        )
                        .unwrap()
                    ))
                    .0,
                expected,
                "{path}"
            );
        }
        // A floor label is not permission to tap an arbitrary bottom button.
        assert!(
            pre_battle_state(&screen(&[("union_campaign/current_label.png", 300, 1250)]))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn union_campaign_counters_feed_the_existing_statistics_once_per_win() {
        let mode = GameMode::UnionCampaign;
        let mut activity = RunActivity::default();
        activity.start(mode, Instant::now());
        activity.apply(ActivityEvent::ModeEntered(mode));
        for outcome in [BattleOutcome::Lost, BattleOutcome::Won] {
            activity.apply(ActivityEvent::BattleStarted(mode));
            activity.apply(ActivityEvent::BattleFinished(mode, outcome));
        }
        activity.apply(ActivityEvent::StageCleared(mode));
        activity.apply(ActivityEvent::StageCleared(mode)); // repeated Continue must not duplicate a clear
        assert_eq!(activity.modes[&mode].attempts, 2);
        assert_eq!(activity.modes[&mode].cleared, 1);
        assert_eq!(activity.success_rate(), Some(50.0));
        assert_eq!(activity.attempts_per_clear(), Some(2.0));
    }

    #[test]
    fn cancellation_before_start_does_not_touch_adb_or_emit_activity() {
        let mut events = vec![];
        assert!(run(
            "not-a-device",
            BattleSettings {
                mode: GameMode::UnionCampaign,
                use_suggested_formations: true,
                current_team_first: true,
                skip_manual_formations: false,
                max_formations: 10,
                attempts_per_formation: 5
            },
            &AtomicBool::new(true),
            |_| {},
            |e| events.push(e)
        )
        .is_err());
        assert!(events.is_empty());
    }

    #[test]
    #[ignore = "requires a connected emulator; navigates and opens formation only, no battle"]
    fn connected_emulator_union_navigation() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        navigate(&serial, &cancelled, &mut |m| println!("{m}")).unwrap();
        println!("Union Campaign floor reached");
        prepare_floor(&serial, &cancelled).unwrap();
        println!("Union Campaign formation reached; no battle started");
    }
}
