//! Optional paid Instant AFK and the original stage-reward collection path.
use super::{afk_stages as common, daily, ocr};
use crate::{
    activity::{ActivityEvent, GameMode},
    adb,
    localization::{format_text, text},
    routine::DiamondHourglassSettings,
};
use image::GrayImage;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub struct Cursor {
    confirmed: u32,
    reserved: u32,
    pending: Option<Receipt>,
}

enum Receipt {
    Purchase { balance: u32, price: u32 },
    StageReward,
}

fn exact_amount(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit() || c == b',') {
        return None;
    }
    let groups: Vec<_> = value.split(',').collect();
    if groups.len() > 1
        && (!(1..=3).contains(&groups[0].len()) || groups[1..].iter().any(|g| g.len() != 3))
    {
        return None;
    }
    value.replace(',', "").parse().ok()
}

fn amount(screen: &GrayImage, rect: common::Rect) -> Result<Option<u32>, String> {
    // The dark badge edge can be mistaken for punctuation. Isolate the white
    // digits, then require agreement at two thresholds rather than fixing OCR.
    let mut previous = None;
    for threshold in [180, 200] {
        let mut patch =
            GrayImage::from_pixel(rect.width + 32, rect.height + 32, image::Luma([255]));
        for y in 0..rect.height {
            for x in 0..rect.width {
                if screen.get_pixel(rect.x + x, rect.y + y).0[0] >= threshold {
                    patch.put_pixel(x + 16, y + 16, image::Luma([0]));
                }
            }
        }
        let patch = image::imageops::resize(
            &patch,
            patch.width() * 3,
            patch.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = ocr::recognize(&patch)?;
        let Some(value) = (lines.len() == 1)
            .then(|| exact_amount(&lines[0].text))
            .flatten()
        else {
            return Ok(None);
        };
        if previous.is_some_and(|before| before != value) {
            return Ok(None);
        }
        previous = Some(value);
    }
    Ok(previous)
}

fn paid_offer(screen: &GrayImage) -> Result<Option<u32>, String> {
    if !daily::page(screen)?
        || common::find_template(
            screen,
            "daily/paid_hourglass.png",
            common::Rect {
                x: 850,
                y: 1445,
                width: 85,
                height: 65,
            },
            0.9,
        )?
        .is_none()
    {
        return Ok(None);
    }
    Ok(amount(
        screen,
        common::Rect {
            x: 930,
            y: 1458,
            width: 80,
            height: 46,
        },
    )?
    .filter(|price| (50..=5000).contains(price) && price % 50 == 0))
}

fn wallet(screen: &GrayImage) -> Result<Option<u32>, String> {
    if common::find_template(
        screen,
        "daily/wallet_diamond.png",
        common::Rect {
            x: 700,
            y: 0,
            width: 110,
            height: 85,
        },
        0.93,
    )?
    .is_none()
    {
        return Ok(None);
    }
    amount(
        screen,
        common::Rect {
            x: 794,
            y: 10,
            width: 154,
            height: 53,
        },
    )
}

fn read_wallet(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<u32, String> {
    let size = common::navigate_to_current_overview(serial, cancelled, log)?;
    common::ensure_not_cancelled(cancelled)?;
    adb::tap_game_point(serial, 330, 1830, size).map_err(|e| e.to_string())?;
    let mut last = None;
    for _ in 0..16 {
        common::settle_cancellable(cancelled)?;
        let (screen, _) = common::screenshot(serial)?;
        let current = wallet(&screen)?;
        if let Some(value) = current.filter(|_| current == last) {
            return Ok(value);
        }
        last = current;
    }
    Err(text(
        "The exact diamond balance could not be verified. No further purchase was made.",
        "Der genaue Diamantenbestand konnte nicht bestätigt werden. Kein weiterer Kauf ausgeführt.",
    )
    .into())
}

fn purchase_confirmed(before: u32, after: u32, price: u32) -> bool {
    price > 0 && before.checked_sub(after) == Some(price)
}

fn finish_purchase(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    let Some(Receipt::Purchase { balance, price }) = cursor.pending else {
        return Err("No pending hourglass purchase.".into());
    };
    let after = read_wallet(serial, cancelled, log)?;
    if !purchase_confirmed(balance, after, price) {
        return Err(text("The diamond charge could not be confirmed. No additional hourglass will be purchased.", "Der Diamantenverbrauch konnte nicht bestätigt werden. Es wird keine weitere Sanduhr gekauft.").into());
    }
    cursor.pending = None;
    cursor.confirmed += 1;
    report(ActivityEvent::Collected(GameMode::DiamondHourglasses, 1));
    Ok(after)
}

fn run_paid(
    serial: &str,
    settings: DiamondHourglassSettings,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    if cursor.pending.is_some() {
        finish_purchase(serial, cursor, cancelled, log, report)?;
    }
    // Adding the optional step is the opt-in; zero budget disables spending.
    if settings.purchases == 0 || settings.budget == 0 {
        report(ActivityEvent::ModeEntered(GameMode::DiamondHourglasses));
        return Ok(cursor.confirmed);
    }
    let mut balance = read_wallet(serial, cancelled, log)?;
    loop {
        common::navigate_to_afk_stages(serial, cancelled, log)?;
        report(ActivityEvent::ModeEntered(GameMode::DiamondHourglasses));
        let (screen, _) = common::screenshot(serial)?;
        let Some(price) = paid_offer(&screen)? else {
            return Err(text("No verified diamond hourglass price. Run free hourglasses first and check the AFK page.", "Kein bestätigter Diamantenpreis für Sanduhren. Führe zuerst die kostenlosen Sanduhren aus und prüfe die AFK-Ansicht.").into());
        };
        if !settings.permits(cursor.confirmed, cursor.reserved, price) || price > balance {
            log(text(
                "The diamond hourglass limit has been reached.",
                "Das Limit für Diamanten-Sanduhren ist erreicht.",
            )
            .into());
            break;
        }
        common::settle_cancellable(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if paid_offer(&screen)? != Some(price) {
            return Err(text(
                "The hourglass price changed. No purchase was made.",
                "Der Sanduhrenpreis hat sich verändert. Kein Kauf ausgeführt.",
            )
            .into());
        }
        // This button may immediately spend diamonds. Reserve the budget and
        // receipt BEFORE the only purchase tap, including across crash recovery.
        common::ensure_not_cancelled(cancelled)?;
        cursor.reserved += price;
        cursor.pending = Some(Receipt::Purchase { balance, price });
        adb::tap_game_point(serial, 965, 1410, size).map_err(|e| e.to_string())?;
        for _ in 0..3 {
            common::settle_cancellable(cancelled)?;
        }
        let (screen, _) = common::screenshot(serial)?;
        if !daily::page(&screen)? {
            return Err(text("The paid hourglass opened an unexpected dialog. No further purchase was made.", "Die Diamanten-Sanduhr hat einen unerwarteten Dialog geöffnet. Kein weiterer Kauf ausgeführt.").into());
        }
        balance = finish_purchase(serial, cursor, cancelled, log, report)?;
        if cursor.confirmed >= settings.purchases.min(10)
            || cursor.reserved >= settings.budget.min(5000)
        {
            // Leave the game at the AFK page for the next Daily step.
            common::navigate_to_afk_stages(serial, cancelled, log)?;
            break;
        }
    }
    Ok(cursor.confirmed)
}

#[derive(Debug, PartialEq)]
enum StageView {
    Claim((u32, u32)),
    Done,
    Loot((u32, u32)),
    Other,
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn stage_view(lines: &[ocr::TextLine]) -> StageView {
    let title = lines.iter().any(|l| {
        let s = compact(&l.text);
        l.y < 1350.0 && (s.contains("stagereward") || s.contains("progressreward"))
    });
    let close = lines
        .iter()
        .find(|l| compact(&l.text) == "taptoclose" && l.y > 1000.0);
    let loot = lines.iter().any(|l| {
        l.y < 1000.0
            && matches!(
                compact(&l.text).as_str(),
                "rewards" | "reward" | "youreceived" | "youobtained" | "obtained"
            )
    });
    if loot {
        if let Some(close) = close {
            return StageView::Loot(close.center());
        }
    }
    if title {
        if let Some(claim) = lines.iter().find(|l| {
            l.y > 650.0 && matches!(compact(&l.text).as_str(), "claim" | "claimall" | "collect")
        }) {
            return StageView::Claim(claim.center());
        }
        if lines.iter().any(|l| {
            let s = compact(&l.text);
            s == "claimed"
                || s.contains("allrewardsclaimed")
                || s.contains("clearafkstage")
                || s.contains("noclaimablerewards")
        }) {
            return StageView::Done;
        }
    }
    StageView::Other
}

fn stage_target(lines: &[ocr::TextLine]) -> Option<(u32, u32)> {
    lines
        .iter()
        .find(|line| {
            let name = compact(&line.text);
            line.y < 1000.0 && (name.contains("stagereward") || name.contains("progressreward"))
        })
        .map(ocr::TextLine::center)
}

fn finish_stage_claim(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<(), String> {
    for _ in 0..10 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if let StageView::Loot(point) = stage_view(&ocr::recognize(&screen)?) {
            common::ensure_not_cancelled(cancelled)?;
            adb::tap_game_point(serial, point.0, point.1, size).map_err(|e| e.to_string())?;
            cursor.pending = None;
            cursor.confirmed += 1;
            report(ActivityEvent::Collected(GameMode::StageRewards, 1));
            common::settle_cancellable(cancelled)?;
            return Ok(());
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The stage reward could not be confirmed. No further claim was attempted.",
        "Die Stufenbelohnung konnte nicht bestätigt werden. Kein weiterer Abholversuch.",
    )
    .into())
}

fn run_stage_rewards(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    if cursor.pending.is_some() {
        let (screen, _) = common::screenshot(serial)?;
        if matches!(stage_view(&ocr::recognize(&screen)?), StageView::Loot(_)) {
            finish_stage_claim(serial, cursor, cancelled, report)?;
        } else {
            // A crash can discard the free reward overlay. Its receipt remains
            // uncounted; revisit the AFK page and check remaining rewards.
            cursor.pending = None;
            log(text("An interrupted stage reward was not counted. Checking the remaining free rewards.", "Eine unterbrochene Stufenbelohnung wurde nicht gezählt. Die übrigen kostenlosen Belohnungen werden geprüft.").into());
        }
    }
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    report(ActivityEvent::ModeEntered(GameMode::StageRewards));
    for _ in 0..50 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if !daily::page(&screen)? {
            return Err(text(
                "The AFK stage reward page could not be verified.",
                "Die AFK-Stufenbelohnungsseite konnte nicht bestätigt werden.",
            )
            .into());
        }
        let target = stage_target(&ocr::recognize(&screen)?).unwrap_or((770, 500));
        // The original opens/claims here. Only probe once on a verified AFK
        // page, then inspect the response instead of repeating blind taps.
        common::ensure_not_cancelled(cancelled)?;
        adb::tap_game_point(serial, target.0, target.1, size).map_err(|e| e.to_string())?;
        common::settle_cancellable(cancelled)?;
        let mut settled_page = 0;
        let mut claim_started = false;
        for _ in 0..8 {
            common::ensure_not_cancelled(cancelled)?;
            let (screen, size) = common::screenshot(serial)?;
            match stage_view(&ocr::recognize(&screen)?) {
                StageView::Loot(_) => {
                    cursor.pending = Some(Receipt::StageReward);
                    claim_started = true;
                    break;
                }
                StageView::Claim(point) => {
                    cursor.pending = Some(Receipt::StageReward);
                    common::ensure_not_cancelled(cancelled)?;
                    adb::tap_game_point(serial, point.0, point.1, size)
                        .map_err(|e| e.to_string())?;
                    claim_started = true;
                    break;
                }
                StageView::Done => {
                    common::navigate_to_afk_stages(serial, cancelled, log)?;
                    return Ok(cursor.confirmed);
                }
                StageView::Other => {
                    if daily::page(&screen)? {
                        settled_page += 1;
                    } else {
                        settled_page = 0;
                    }
                    if settled_page >= 3 {
                        log(text(
                            "No claimable AFK stage rewards found.",
                            "Keine abholbaren AFK-Stufenbelohnungen gefunden.",
                        )
                        .into());
                        return Ok(cursor.confirmed);
                    }
                }
            }
            common::settle_cancellable(cancelled)?;
        }
        if !claim_started {
            return Err(text(
                "The stage reward dialog was not recognized.",
                "Der Stufenbelohnungsdialog wurde nicht erkannt.",
            )
            .into());
        }
        finish_stage_claim(serial, cursor, cancelled, report)?;
        common::navigate_to_afk_stages(serial, cancelled, log)?;
    }
    Err(text(
        "Stage reward collection reached its safety limit.",
        "Das Limit für Stufenbelohnungen wurde erreicht.",
    )
    .into())
}

pub fn run(
    serial: &str,
    mode: GameMode,
    settings: DiamondHourglassSettings,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    report(ActivityEvent::ModeOpening(mode));
    let result = match mode {
        GameMode::DiamondHourglasses => run_paid(serial, settings, cursor, cancelled, log, report),
        GameMode::StageRewards => run_stage_rewards(serial, cursor, cancelled, log, report),
        _ => Err("Invalid collection mode.".into()),
    };
    if result.is_ok() {
        log(format_text!(
            "{} · {} collected.",
            "{} · {} abgeholt.",
            mode.label(),
            cursor.confirmed
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(value: &str, y: f32) -> ocr::TextLine {
        ocr::TextLine {
            text: value.into(),
            x: 450.0,
            y,
            width: 180.0,
            height: 50.0,
        }
    }

    #[test]
    fn prices_and_receipts_reject_ambiguous_or_partial_numbers() {
        assert_eq!(exact_amount("143,981"), Some(143981));
        for bad in ["143K", "1,00", "50 100", "l00", "50 Diamonds", "", "-50"] {
            assert_eq!(exact_amount(bad), None, "{bad}");
        }
        assert!(purchase_confirmed(143981, 143881, 100));
        assert!(!purchase_confirmed(143981, 143981, 100));
        assert!(!purchase_confirmed(143981, 143781, 100));
        assert!(!purchase_confirmed(10, 20, 100));
        assert!(!purchase_confirmed(0, 0, 0));
    }

    #[test]
    fn stage_rewards_need_their_own_context_and_confirmed_loot() {
        assert_eq!(stage_view(&[line("Claim", 1000.0)]), StageView::Other);
        assert_eq!(
            stage_view(&[line("AFK Stage Rewards", 500.0), line("Claim All", 1500.0)]),
            StageView::Claim((540, 1525))
        );
        assert_eq!(
            stage_view(&[line("Stage Rewards", 500.0), line("Claimed", 1500.0)]),
            StageView::Done
        );
        assert_eq!(stage_view(&[line("Rewards", 500.0)]), StageView::Other);
        assert_eq!(
            stage_view(&[line("Rewards", 500.0), line("Tap to close", 1700.0)]),
            StageView::Loot((540, 1725))
        );
        assert_eq!(
            stage_view(&[line("Victory Rewards", 500.0), line("Tap to close", 1700.0)]),
            StageView::Other
        );
    }

    #[test]
    #[ignore = "reads supplied captures of paid hourglass prices and diamond wallet without any device input"]
    fn captured_paid_offer_and_wallet() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DAILY_EXTRAS").expect("capture folder"),
        );
        let load = |name| {
            common::normalize_screenshot(&std::fs::read(folder.join(name)).unwrap())
                .unwrap()
                .0
        };
        assert_eq!(paid_offer(&load("before.png")).unwrap(), Some(50));
        assert_eq!(paid_offer(&load("stage-reward.png")).unwrap(), Some(100));
        let house = load("mystical-house.png");
        println!(
            "wallet icon: {:?}",
            common::find_template(
                &house,
                "daily/wallet_diamond.png",
                common::Rect {
                    x: 700,
                    y: 0,
                    width: 110,
                    height: 85
                },
                0.0
            )
            .unwrap()
        );
        println!(
            "wallet OCR: {:?}",
            daily::crop_lines(
                &house,
                common::Rect {
                    x: 794,
                    y: 10,
                    width: 154,
                    height: 53
                }
            )
            .unwrap()
        );
        assert_eq!(wallet(&house).unwrap(), Some(143981));
        assert_eq!(wallet(&load("world.png")).unwrap(), None);
        assert_eq!(
            stage_view(&ocr::recognize(&load("before.png")).unwrap()),
            StageView::Other
        );
    }

    #[test]
    #[ignore = "navigates to the diamond wallet and AFK page, without purchasing any hourglass"]
    fn connected_emulator_wallet_and_offer_readonly() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = AtomicBool::new(false);
        let balance = read_wallet(&serial, &cancelled, &mut |line| println!("{line}")).unwrap();
        assert!(balance > 100);
        let (screen, _) =
            common::navigate_to_afk_stages(&serial, &cancelled, &mut |line| println!("{line}"))
                .unwrap();
        assert_eq!(paid_offer(&screen).unwrap(), Some(100));
    }

    #[test]
    #[ignore = "collects available free AFK stage rewards, without purchasing hourglasses"]
    fn connected_emulator_stage_rewards() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let mut cursor = Cursor::default();
        run(
            &serial,
            GameMode::StageRewards,
            DiamondHourglassSettings::default(),
            &mut cursor,
            &AtomicBool::new(false),
            &mut |line| println!("{line}"),
            &mut |event| println!("{event:?}"),
        )
        .unwrap();
    }
}
