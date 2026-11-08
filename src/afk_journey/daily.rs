use super::{afk_stages as common, ocr};
use crate::{
    activity::{ActivityEvent, GameMode},
    adb,
    localization::{format_text, text},
};
use image::GrayImage;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy)]
pub enum Collection {
    AfkRewards,
    FreeHourglasses,
}

impl Collection {
    pub fn mode(self) -> GameMode {
        match self {
            Self::AfkRewards => GameMode::AfkRewards,
            Self::FreeHourglasses => GameMode::FreeHourglasses,
        }
    }
    fn limit(self) -> u32 {
        match self {
            Self::AfkRewards => 2,
            Self::FreeHourglasses => 3,
        }
    }
}

#[derive(Default)]
pub struct Cursor {
    confirmed: u32,
    pending: Option<Pending>,
}

enum Pending {
    AfkRewards,
    FreeHourglasses(u32),
}

fn find(
    screen: &GrayImage,
    path: &str,
    region: common::Rect,
) -> Result<Option<common::Match>, String> {
    common::find_template(screen, path, region, 0.9)
}

pub(super) fn page(screen: &GrayImage) -> Result<bool, String> {
    Ok(find(
        screen,
        "navigation/resonating_hall_label.png",
        common::Rect {
            x: 324,
            y: 1728,
            width: 432,
            height: 192,
        },
    )?
    .is_some()
        && claim_control(screen)?.is_none()
        && common::get_match(
            screen,
            &["navigation/confirm.png", "confirm_text.png", "cancel.png"],
            common::lower_screen(),
            0.9,
        )?
        .is_none())
}

fn claim_control(screen: &GrayImage) -> Result<Option<common::Match>, String> {
    if find(
        screen,
        "daily/afk_time.png",
        common::Rect {
            x: 740,
            y: 560,
            width: 300,
            height: 190,
        },
    )?
    .is_none()
    {
        return Ok(None);
    }
    find(
        screen,
        "daily/claim.png",
        common::Rect {
            x: 200,
            y: 980,
            width: 680,
            height: 190,
        },
    )
}

pub(super) fn crop_lines(
    screen: &GrayImage,
    rect: common::Rect,
) -> Result<Vec<ocr::TextLine>, String> {
    let crop =
        image::imageops::crop_imm(screen, rect.x, rect.y, rect.width, rect.height).to_image();
    let mut padded = GrayImage::from_pixel(rect.width + 32, rect.height + 32, image::Luma([255]));
    image::imageops::replace(&mut padded, &crop, 16, 16);
    let enlarged = image::imageops::resize(
        &padded,
        padded.width() * 3,
        padded.height() * 3,
        image::imageops::FilterType::CatmullRom,
    );
    ocr::recognize(&enlarged)
}

fn hourglasses(screen: &GrayImage) -> Result<Option<(u32, common::Match)>, String> {
    if !page(screen)? {
        return Ok(None);
    }
    let region = common::Rect {
        x: 850,
        y: 1310,
        width: 200,
        height: 220,
    };
    if let Some(paid) = find(screen, "daily/paid_hourglass.png", region)? {
        return Ok(Some((0, paid)));
    }
    let Some(button) = find(screen, "daily/free_hourglass.png", region)? else {
        return Ok(None);
    };
    let lines = ocr::repeated_numeric_patch(
        screen,
        common::Rect {
            x: 963,
            y: 1464,
            width: 20,
            height: 36,
        },
    )?;
    let mut values = lines
        .iter()
        .filter_map(|line| ocr::repeated_counter(&line.text))
        .filter(|count| (1..=3).contains(count));
    let count = values
        .next()
        .filter(|first| values.all(|next| next == *first));
    Ok(count.map(|count| (count, button)))
}

fn timer_reset(screen: &GrayImage) -> Result<bool, String> {
    Ok(common::find_template(
        screen,
        "daily/timer_reset.png",
        common::Rect {
            x: 460,
            y: 1475,
            width: 190,
            height: 55,
        },
        0.94,
    )?
    .is_some())
}

fn wait_hourglasses(
    serial: &str,
    cancelled: &AtomicBool,
) -> Result<(u32, common::Match, (u32, u32)), String> {
    for _ in 0..8 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if let Some((count, button)) = hourglasses(&screen)? {
            return Ok((count, button, size));
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The free hourglass count could not be verified.",
        "Die Anzahl kostenloser Sanduhren konnte nicht bestätigt werden.",
    )
    .into())
}

fn free_confirmation(lines: &[ocr::TextLine], expected: u32) -> bool {
    let content = lines
        .iter()
        .map(|line| line.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let remaining = compact
        .split_once("remainingattempts:")
        .and_then(|(_, value)| {
            value
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok()
        });
    compact.contains("useafreeinstantafkattempttoget")
        && remaining == Some(expected)
        && !compact.contains("diamond")
        && !compact.contains("spend")
}

fn verify_pending(serial: &str, pending: &Pending, cancelled: &AtomicBool) -> Result<(), String> {
    for _ in 0..12 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, _) = common::screenshot(serial)?;
        let confirmed = match pending {
            Pending::AfkRewards => page(&screen)? && timer_reset(&screen)?,
            Pending::FreeHourglasses(before) => hourglasses(&screen)?
                .is_some_and(|(after, _)| after.checked_add(1) == Some(*before)),
        };
        if confirmed {
            return Ok(());
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The collection could not be confirmed. Check the AFK screen before restarting the Daily routine.",
        "Das Abholen konnte nicht bestätigt werden. Prüfe die AFK-Ansicht vor einem erneuten Daily-Start.",
    ).into())
}

pub fn run(
    serial: &str,
    collection: Collection,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    let mode = collection.mode();
    report(ActivityEvent::ModeOpening(mode));
    // A restarted game opens outside AFK. Re-enter the verified page before
    // reconciling a pending receipt, without clicking another hourglass.
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    report(ActivityEvent::ModeEntered(mode));
    if let Some(pending) = &cursor.pending {
        let confirmed = match pending {
            Pending::FreeHourglasses(_) => {
                verify_pending(serial, pending, cancelled)?;
                true
            }
            Pending::AfkRewards => {
                let (screen, _) = common::screenshot(serial)?;
                if !page(&screen)? {
                    return Err(text(
                        "The AFK page changed during recovery.",
                        "Die AFK-Ansicht hat sich während der Wiederherstellung verändert.",
                    )
                    .into());
                }
                timer_reset(&screen)?
            }
        };
        cursor.pending = None;
        if confirmed {
            cursor.confirmed += 1;
            report(ActivityEvent::Collected(mode, 1));
        } else {
            // An old chest timer is no proof of collection after a long restart.
            // Do not count it. Fresh free chest collections can still complete
            // the requested daily progress, without buying any hourglass.
            log(text("An interrupted AFK chest claim could not be counted. Continuing with confirmed free collections.", "Eine unterbrochene AFK-Truhenabholung konnte nicht gezählt werden. Es folgen bestätigte kostenlose Abholungen.").into());
        }
    }
    while cursor.confirmed < collection.limit() {
        common::ensure_not_cancelled(cancelled)?;
        match collection {
            Collection::AfkRewards => {
                for _ in 0..14 {
                    common::settle_cancellable(cancelled)?;
                }
                let (screen, size) = common::screenshot(serial)?;
                if !page(&screen)? {
                    return Err(text(
                        "The AFK rewards page was not verified.",
                        "Die AFK-Belohnungsseite wurde nicht bestätigt.",
                    )
                    .into());
                }
                adb::tap_game_point(serial, 520, 1400, size).map_err(|e| e.to_string())?;
                let (_, claim, size) = common::wait_for_match(
                    serial,
                    &["daily/claim.png"],
                    common::Rect {
                        x: 200,
                        y: 980,
                        width: 680,
                        height: 190,
                    },
                    std::time::Duration::from_secs(10),
                    cancelled,
                )?;
                let (screen, _) = common::screenshot(serial)?;
                if claim_control(&screen)?.is_none() {
                    return Err(text(
                        "The AFK claim dialog changed.",
                        "Der AFK-Abholdialog hat sich verändert.",
                    )
                    .into());
                }
                cursor.pending = Some(Pending::AfkRewards);
                common::ensure_not_cancelled(cancelled)?;
                common::tap_match(serial, claim, size)?;
            }
            Collection::FreeHourglasses => {
                let (before, button, size) = wait_hourglasses(serial, cancelled)?;
                if before == 0 {
                    log(text(
                        "No free hourglasses remain.",
                        "Keine kostenlosen Sanduhren mehr verfügbar.",
                    )
                    .into());
                    break;
                }
                cursor.pending = Some(Pending::FreeHourglasses(before));
                common::ensure_not_cancelled(cancelled)?;
                common::tap_match(serial, button, size)?;
                common::settle_cancellable(cancelled)?;
                let (screen, size) = common::screenshot(serial)?;
                if !page(&screen)? {
                    let rect = common::Rect {
                        x: 60,
                        y: 480,
                        width: 960,
                        height: 800,
                    };
                    let lines = crop_lines(&screen, rect)?;
                    if !free_confirmation(&lines, before) {
                        adb::press_back(serial).map_err(|e| e.to_string())?;
                        return Err(text(
                            "The Instant AFK offer was not explicitly free.",
                            "Das Instant-AFK-Angebot war nicht eindeutig kostenlos.",
                        )
                        .into());
                    }
                    common::settle_cancellable(cancelled)?;
                    let (screen, _) = common::screenshot(serial)?;
                    if !free_confirmation(&crop_lines(&screen, rect)?, before) {
                        return Err(text(
                            "The free Instant AFK offer changed.",
                            "Das kostenlose Instant-AFK-Angebot hat sich verändert.",
                        )
                        .into());
                    }
                    let (_, confirm) = common::get_match(
                        &screen,
                        &["navigation/confirm.png", "confirm_text.png"],
                        common::lower_screen(),
                        0.9,
                    )?
                    .ok_or(text(
                        "Free Instant AFK confirmation not found.",
                        "Die kostenlose Instant-AFK-Bestätigung wurde nicht erkannt.",
                    ))?;
                    common::ensure_not_cancelled(cancelled)?;
                    common::tap_match(serial, confirm, size)?;
                }
            }
        }
        common::settle_cancellable(cancelled)?;
        verify_pending(
            serial,
            cursor.pending.as_ref().expect("collection reserved"),
            cancelled,
        )?;
        cursor.pending = None;
        cursor.confirmed += 1;
        report(ActivityEvent::Collected(mode, 1));
        log(format_text!(
            "{} · {} collected.",
            "{} · {} abgeholt.",
            mode.label(),
            cursor.confirmed
        ));
    }
    Ok(cursor.confirmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn free_popup_requires_explicit_free_attempts() {
        let line = |value: &str| ocr::TextLine {
            text: value.into(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        assert!(free_confirmation(&[line("Use a free Instant AFK attempt to get 120 minutes worth of AFK Rewards. Remaining attempts: 3")], 3));
        assert!(!free_confirmation(
            &[line("Confirm to use 50 Diamonds? Remaining attempts: 3")],
            3
        ));
        assert!(!free_confirmation(&[line("Use a free Instant AFK attempt to get 120 minutes worth of AFK Rewards. Remaining attempts: 30")], 3));
    }
    #[test]
    #[ignore = "reads supplied AFK screenshots to verify claim and free-versus-paid recognition"]
    fn captured_daily_controls() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DAILY_CAPTURES").expect("capture folder"),
        );
        let load = |name| {
            common::normalize_screenshot(&std::fs::read(folder.join(name)).unwrap())
                .unwrap()
                .0
        };
        let claim = load("claimed-once.png");
        assert!(claim_control(&claim).unwrap().is_some());
        assert!(timer_reset(&load("claim-result.png")).unwrap());
        assert!(timer_reset(&load("live-final.png")).unwrap());
        assert!(!timer_reset(&load("pre-claim.png")).unwrap());
        assert!(!timer_reset(&load("hourglass-exhausted.png")).unwrap());
        assert!(!timer_reset(&load("before.png")).unwrap());
        assert_eq!(hourglasses(&load("pre-claim.png")).unwrap().unwrap().0, 2);
        assert_eq!(
            hourglasses(&load("hourglass-after.png"))
                .unwrap()
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            hourglasses(&load("hourglass-exhausted.png"))
                .unwrap()
                .unwrap()
                .0,
            0
        );
        assert_eq!(hourglasses(&load("live-final.png")).unwrap().unwrap().0, 0);
    }

    #[test]
    #[ignore = "collects only the second daily chest after one manual claim"]
    fn connected_emulator_second_daily_claim() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        let mut cursor = Cursor {
            confirmed: 1,
            pending: None,
        };
        let mut events = Vec::new();
        assert_eq!(
            run(
                &serial,
                Collection::AfkRewards,
                &mut cursor,
                &cancelled,
                &mut |message| println!("{message}"),
                &mut |event| events.push(event)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ActivityEvent::Collected(GameMode::AfkRewards, 1)))
                .count(),
            1
        );
    }

    #[test]
    #[ignore = "checks that the current paid hourglass offer is treated as zero free attempts, without input"]
    fn connected_emulator_exhausted_hourglasses() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        assert_eq!(
            wait_hourglasses(&serial, &AtomicBool::new(false))
                .unwrap()
                .0,
            0
        );
    }
}
