//! Optional paid Instant AFK and the original stage-reward collection path.
use super::{afk_stages as common, daily, ocr};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::vision;
use crate::{
    activity::{ActivityEvent, GameMode, HourglassStop},
    adb,
    localization::{format_text, text},
    routine::DiamondHourglassSettings,
};
use image::GrayImage;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub struct Cursor {
    confirmed: u32,
    pending: Option<Receipt>,
    after_purchase: Option<PaidStatus>,
}

enum Receipt {
    Purchase { price: u32 },
    StageReward,
}

pub(super) fn exact_amount(value: &str) -> Option<u32> {
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

pub(super) fn amount(screen: &GrayImage, rect: vision::Rect) -> Result<Option<u32>, ActionError> {
    amount_with_thresholds(screen, rect, &[180, 200], 3)
}

pub(super) fn amount_with_thresholds(
    screen: &GrayImage,
    rect: vision::Rect,
    thresholds: &[u8],
    scale: u32,
) -> Result<Option<u32>, ActionError> {
    // The dark badge edge can be mistaken for punctuation. Isolate the white
    // digits and require two matching reads. Conflicting readings stop the check.
    let mut previous = None;
    for (index, &threshold) in thresholds.iter().enumerate() {
        let mut patch =
            GrayImage::from_pixel(rect.width + 32, rect.height + 32, image::Luma([255]));
        for y in 0..rect.height {
            for x in 0..rect.width {
                if screen.get_pixel(rect.x + x, rect.y + y).0[0] >= threshold {
                    patch.put_pixel(x + 16, y + 16, image::Luma([0]));
                }
            }
        }
        let enlarged = image::imageops::resize(
            &patch,
            patch.width() * scale,
            patch.height() * scale,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = ocr::recognize(&enlarged)?;
        let Some(value) = (lines.len() == 1)
            .then(|| exact_amount(&lines[0].text))
            .flatten()
        else {
            let remaining = thresholds.len() - index - 1;
            if remaining + usize::from(previous.is_some()) < 2 {
                return Ok(None);
            }
            continue;
        };
        if let Some(before) = previous {
            return Ok((before == value).then_some(value));
        }
        previous = Some(value);
    }
    Ok(None)
}

// The price steps are visible in-game and reset with the daily allowance.
// An unfamiliar price is not an approved offer.
const PAID_PRICES: [u32; 4] = [50, 100, 200, 300];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaidStatus {
    Price(u32),
    FreeAvailable,
    Exhausted,
}

fn paid_status(screen: &GrayImage) -> Result<Option<PaidStatus>, ActionError> {
    if !daily::page(screen)? {
        return Ok(None);
    }
    if daily::hourglass_control(screen, "daily/paid_hourglass.png")?.is_some() {
        return Ok(amount(
            screen,
            vision::Rect {
                x: 930,
                y: 1458,
                width: 80,
                height: 46,
            },
        )?
        .filter(|price| PAID_PRICES.contains(price))
        .map(PaidStatus::Price));
    }
    if daily::hourglass_control(screen, "daily/hourglass_zero.png")?.is_some() {
        return Ok(Some(PaidStatus::Exhausted));
    }
    if daily::hourglass_control(screen, "daily/free_hourglass.png")?.is_some()
        && daily::free_hourglass_count(screen)?.is_some()
    {
        return Ok(Some(PaidStatus::FreeAvailable));
    }
    Ok(None)
}

fn wallet(screen: &GrayImage) -> Result<Option<u32>, ActionError> {
    if vision::find_template(
        screen,
        "daily/wallet_diamond.png",
        vision::Rect {
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
    // The bright scenery above and below the badge can join adjacent
    // digits into one OCR glyph. Read just the printed balance.
    amount(
        screen,
        vision::Rect {
            x: 793,
            y: 22,
            width: 132,
            height: 38,
        },
    )
}

fn read_wallet(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Option<u32>, ActionError> {
    let size = common::navigate_to_current_overview(serial, cancelled, log)?;
    vision::ensure_not_cancelled(cancelled)?;
    adb::tap_game_point(serial, 330, 1830, size).map_err(|e| e.to_string())?;
    let mut last = None;
    for _ in 0..10 {
        vision::settle_cancellable(cancelled)?;
        let (screen, _) = vision::screenshot(serial)?;
        let current = wallet(&screen)?;
        if current.is_some() && current == last {
            return Ok(current);
        }
        last = current;
    }
    Ok(None)
}

fn stable_paid_status(
    serial: &str,
    cancelled: &AtomicBool,
    accept: impl Fn(PaidStatus) -> bool,
) -> Result<Option<(PaidStatus, (u32, u32))>, ActionError> {
    let mut last = None;
    for _ in 0..10 {
        vision::ensure_not_cancelled(cancelled)?;
        let (screen, size) = vision::screenshot(serial)?;
        let current = paid_status(&screen)?.filter(|status| accept(*status));
        if let Some(status) = current.filter(|_| current == last) {
            return Ok(Some((status, size)));
        }
        last = current;
        vision::settle_cancellable(cancelled)?;
    }
    Ok(None)
}

fn confirms_purchase(price: u32, status: PaidStatus) -> bool {
    let Some(index) = PAID_PRICES.iter().position(|candidate| *candidate == price) else {
        return false;
    };
    match status {
        PaidStatus::Price(next) => PAID_PRICES.get(index + 1) == Some(&next),
        PaidStatus::Exhausted => true,
        PaidStatus::FreeAvailable => false,
    }
}

fn finish_purchase(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<(PaidStatus, (u32, u32)), ActionError> {
    let Some(Receipt::Purchase { price }) = cursor.pending else {
        return Err("No pending hourglass purchase.".into());
    };
    let confirmed = stable_paid_status(serial, cancelled, |status| confirms_purchase(price, status))?
        .ok_or_else(|| text(
            "The hourglass purchase could not be verified. Check Instant AFK before retrying.",
            "Der Sanduhrenkauf konnte nicht bestätigt werden. Prüfe Instant AFK vor einem erneuten Versuch.",
        ).to_owned())?;
    cursor.pending = None;
    cursor.after_purchase = Some(confirmed.0);
    cursor.confirmed += 1;
    report(ActivityEvent::Collected(GameMode::DiamondHourglasses, 1));
    Ok(confirmed)
}

fn allowed_price(
    status: PaidStatus,
    settings: DiamondHourglassSettings,
) -> Result<u32, HourglassStop> {
    let price = match status {
        PaidStatus::Price(price) => price,
        PaidStatus::FreeAvailable => return Err(HourglassStop::FreeAvailable),
        PaidStatus::Exhausted => return Err(HourglassStop::Exhausted),
    };
    // The next price identifies how many purchases the game has counted
    // today, including manual purchases and those made by a previous run.
    let bought = PAID_PRICES
        .iter()
        .position(|candidate| *candidate == price)
        .ok_or(HourglassStop::OfferUnreadable)?;
    let spent = PAID_PRICES[..bought].iter().sum();
    settings
        .permits(bought as u32, spent, price)
        .then_some(price)
        .ok_or(HourglassStop::LimitReached)
}

struct PaidOffer {
    status: PaidStatus,
    size: (u32, u32),
    balance: u32,
}

fn read_paid_offer(
    serial: &str,
    settings: DiamondHourglassSettings,
    after_purchase: Option<PaidStatus>,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Result<PaidOffer, HourglassStop>, ActionError> {
    let Some((status, _)) = stable_paid_status(serial, cancelled, |_| true)? else {
        return Ok(Err(HourglassStop::OfferUnreadable));
    };
    if after_purchase.is_some_and(|expected| expected != status) {
        return Ok(Err(HourglassStop::OfferChanged));
    }
    if let Err(reason) = allowed_price(status, settings) {
        return Ok(Err(reason));
    }
    let Some(balance) = read_wallet(serial, cancelled, log)? else {
        return Ok(Err(HourglassStop::BalanceUnreadable));
    };
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    let Some((same_status, size)) = stable_paid_status(serial, cancelled, |value| value == status)?
    else {
        return Ok(Err(HourglassStop::OfferChanged));
    };
    Ok(Ok(PaidOffer {
        status: same_status,
        size,
        balance,
    }))
}

fn retry_paid_offer(
    mut read: impl FnMut() -> Result<Result<PaidOffer, HourglassStop>, ActionError>,
) -> Result<Result<PaidOffer, HourglassStop>, ActionError> {
    match read()? {
        Err(
            HourglassStop::OfferUnreadable
            | HourglassStop::BalanceUnreadable
            | HourglassStop::OfferChanged,
        ) => read(),
        result => Ok(result),
    }
}

fn run_paid(
    serial: &str,
    settings: DiamondHourglassSettings,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<HourglassStop, ActionError> {
    if cursor.pending.is_none() && (settings.purchases == 0 || settings.budget == 0) {
        report(ActivityEvent::ModeEntered(GameMode::DiamondHourglasses));
        return Ok(HourglassStop::Disabled);
    }
    common::navigate_to_afk_stages(serial, cancelled, log)?;
    report(ActivityEvent::ModeEntered(GameMode::DiamondHourglasses));
    if cursor.pending.is_some() {
        finish_purchase(serial, cursor, cancelled, report)?;
    }
    let mut retry = false;
    let mut offer = match retry_paid_offer(|| {
        if retry {
            common::navigate_to_afk_stages(serial, cancelled, log)?;
        }
        retry = true;
        read_paid_offer(serial, settings, cursor.after_purchase, cancelled, log)
    })? {
        Ok(offer) => offer,
        Err(reason) => return Ok(reason),
    };
    loop {
        let price = match allowed_price(offer.status, settings) {
            Ok(price) => price,
            Err(reason) => return Ok(reason),
        };
        if price > offer.balance {
            return Ok(HourglassStop::InsufficientDiamonds {
                price,
                balance: offer.balance,
            });
        }
        vision::ensure_not_cancelled(cancelled)?;
        // Record the pending purchase before input. On recovery, confirm the
        // game's next price or exhausted state before attempting another.
        cursor.pending = Some(Receipt::Purchase { price });
        adb::tap_game_point(serial, 965, 1410, offer.size).map_err(|e| e.to_string())?;
        // The game advances the daily price after a purchase. This receipt
        // also works when AFK loot credits diamonds to the same wallet.
        (offer.status, offer.size) = finish_purchase(serial, cursor, cancelled, report)?;
        offer.balance -= price;
    }
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
) -> Result<(), ActionError> {
    for _ in 0..10 {
        vision::ensure_not_cancelled(cancelled)?;
        let (screen, size) = vision::screenshot(serial)?;
        if let StageView::Loot(point) = stage_view(&ocr::recognize(&screen)?) {
            vision::ensure_not_cancelled(cancelled)?;
            adb::tap_game_point(serial, point.0, point.1, size).map_err(|e| e.to_string())?;
            cursor.pending = None;
            cursor.confirmed += 1;
            report(ActivityEvent::Collected(GameMode::StageRewards, 1));
            vision::settle_cancellable(cancelled)?;
            return Ok(());
        }
        vision::settle_cancellable(cancelled)?;
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
) -> Result<u32, ActionError> {
    if cursor.pending.is_some() {
        let (screen, _) = vision::screenshot(serial)?;
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
        vision::ensure_not_cancelled(cancelled)?;
        let (screen, size) = vision::screenshot(serial)?;
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
        vision::ensure_not_cancelled(cancelled)?;
        adb::tap_game_point(serial, target.0, target.1, size).map_err(|e| e.to_string())?;
        vision::settle_cancellable(cancelled)?;
        let mut settled_page = 0;
        let mut claim_started = false;
        for _ in 0..8 {
            vision::ensure_not_cancelled(cancelled)?;
            let (screen, size) = vision::screenshot(serial)?;
            match stage_view(&ocr::recognize(&screen)?) {
                StageView::Loot(_) => {
                    cursor.pending = Some(Receipt::StageReward);
                    claim_started = true;
                    break;
                }
                StageView::Claim(point) => {
                    cursor.pending = Some(Receipt::StageReward);
                    vision::ensure_not_cancelled(cancelled)?;
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
            vision::settle_cancellable(cancelled)?;
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
) -> Result<u32, ActionError> {
    report(ActivityEvent::ModeOpening(mode));
    let result = match mode {
        GameMode::DiamondHourglasses => run_paid(serial, settings, cursor, cancelled, log, report)
            .map(|reason| {
                log(reason.message());
                report(ActivityEvent::DiamondHourglassesStopped(reason));
                cursor.confirmed
            }),
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
        assert!(confirms_purchase(50, PaidStatus::Price(100)));
        assert!(confirms_purchase(100, PaidStatus::Price(200)));
        assert!(confirms_purchase(200, PaidStatus::Price(300)));
        assert!(confirms_purchase(300, PaidStatus::Exhausted));
        assert!(!confirms_purchase(100, PaidStatus::Price(100)));
        assert!(!confirms_purchase(100, PaidStatus::Price(300)));
        assert!(!confirms_purchase(300, PaidStatus::Price(50)));
        assert!(!confirms_purchase(300, PaidStatus::FreeAvailable));
        assert!(!confirms_purchase(400, PaidStatus::Exhausted));
    }

    #[test]
    fn remaining_hourglasses_respect_total_daily_limits() {
        let settings = DiamondHourglassSettings {
            purchases: 4,
            budget: 650,
        };
        let final_offer = PaidStatus::Price(300);
        assert_eq!(allowed_price(final_offer, settings), Ok(300));
        assert_eq!(
            allowed_price(
                final_offer,
                DiamondHourglassSettings {
                    budget: 649,
                    ..settings
                }
            ),
            Err(HourglassStop::LimitReached)
        );
        assert_eq!(
            allowed_price(
                final_offer,
                DiamondHourglassSettings {
                    purchases: 3,
                    ..settings
                }
            ),
            Err(HourglassStop::LimitReached)
        );
        assert_eq!(
            allowed_price(
                PaidStatus::Price(100),
                DiamondHourglassSettings {
                    purchases: 2,
                    budget: 150
                }
            ),
            Ok(100)
        );
        assert_eq!(
            allowed_price(PaidStatus::Price(50), DiamondHourglassSettings::default()),
            Ok(50)
        );
        assert_eq!(
            allowed_price(PaidStatus::FreeAvailable, settings),
            Err(HourglassStop::FreeAvailable)
        );
        assert_eq!(
            allowed_price(PaidStatus::Exhausted, settings),
            Err(HourglassStop::Exhausted)
        );
    }

    #[test]
    fn retries_incomplete_offer_checks_before_authorizing_a_purchase() {
        for reason in [
            HourglassStop::OfferUnreadable,
            HourglassStop::BalanceUnreadable,
            HourglassStop::OfferChanged,
        ] {
            let mut reads = [
                Err(reason),
                Ok(PaidOffer {
                    status: PaidStatus::Price(50),
                    size: (1080, 1920),
                    balance: 650,
                }),
            ]
            .into_iter();
            let offer = retry_paid_offer(|| Ok(reads.next().unwrap()))
                .unwrap()
                .ok()
                .unwrap();
            assert_eq!(offer.status, PaidStatus::Price(50));
            assert!(reads.next().is_none());

            let mut reads = 0;
            let result = retry_paid_offer(|| {
                reads += 1;
                Ok(Err(reason))
            });
            assert!(matches!(result, Ok(Err(actual)) if actual == reason));
            assert_eq!(reads, 2);
        }
        let stopped = retry_paid_offer(|| Ok(Err(HourglassStop::Exhausted)));
        assert!(matches!(stopped, Ok(Err(HourglassStop::Exhausted))));
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
    #[ignore = "reads free and paid Instant AFK screenshots without device input"]
    fn captured_free_to_paid_hourglass_transition() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DAILY_CAPTURES").expect("capture folder"),
        );
        let load = |name| {
            vision::normalize_screenshot(&std::fs::read(folder.join(name)).unwrap())
                .unwrap()
                .0
        };
        let mut free = load("hourglass-after.png");
        assert_eq!(paid_status(&free).unwrap(), Some(PaidStatus::FreeAvailable));

        // The icon can remain detectable while the number beside it is unreadable.
        for y in 1464..1500 {
            for x in 963..983 {
                free.put_pixel(x, y, image::Luma([0]));
            }
        }
        assert!(daily::hourglass_control(&free, "daily/free_hourglass.png")
            .unwrap()
            .is_some());
        assert_eq!(paid_status(&free).unwrap(), None);
        assert_eq!(
            paid_status(&load("hourglass-exhausted.png")).unwrap(),
            Some(PaidStatus::Price(50))
        );
    }

    #[test]
    #[ignore = "reads supplied captures of paid hourglass prices and diamond wallet without any device input"]
    fn captured_paid_offer_and_wallet() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DAILY_EXTRAS").expect("capture folder"),
        );
        let load = |name| {
            vision::normalize_screenshot(&std::fs::read(folder.join(name)).unwrap())
                .unwrap()
                .0
        };
        assert_eq!(
            paid_status(&load("before.png")).unwrap(),
            Some(PaidStatus::Price(50))
        );
        assert_eq!(
            paid_status(&load("stage-reward.png")).unwrap(),
            Some(PaidStatus::Price(100))
        );
        let house = load("mystical-house.png");
        println!(
            "wallet icon: {:?}",
            vision::find_template(
                &house,
                "daily/wallet_diamond.png",
                vision::Rect {
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
                vision::Rect {
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
    #[ignore = "reads captures of the reported balance and the used-up Instant AFK allowance"]
    fn captured_already_used_hourglasses() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DAILY_MENU").expect("capture folder"),
        );
        let load = |name: &str| {
            let bytes = std::fs::read(folder.join(name)).unwrap();
            vision::normalize_screenshot(&bytes).unwrap().0
        };
        assert_eq!(wallet(&load("skip_fix_initial.png")).unwrap(), Some(156690));
        let after = paid_status(&load("skip_fix_afk_stable.png"))
            .unwrap()
            .unwrap();
        assert_eq!(after, PaidStatus::Price(300));
        let settings = DiamondHourglassSettings {
            purchases: 4,
            budget: 650,
        };
        assert_eq!(allowed_price(after, settings), Ok(300));
        assert!(confirms_purchase(200, after));
        assert_eq!(
            paid_status(&load("skip_fix_hourglass_zero.png")).unwrap(),
            Some(PaidStatus::Exhausted)
        );
    }

    #[test]
    #[ignore = "navigates to the diamond wallet and AFK page, without purchasing any hourglass"]
    fn connected_emulator_wallet_and_offer_readonly() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = AtomicBool::new(false);
        let mut log = |line| println!("{line}");
        let settings = DiamondHourglassSettings {
            purchases: 4,
            budget: 650,
        };
        common::navigate_to_afk_stages(&serial, &cancelled, &mut log).unwrap();
        let offer = read_paid_offer(&serial, settings, None, &cancelled, &mut log)
            .unwrap()
            .expect("verified Instant AFK offer and balance");
        let price = allowed_price(offer.status, settings).expect("remaining eligible paid offer");
        assert!(offer.balance >= price);
        println!("Eligible after checking the diamond balance: {price} diamonds.");
    }

    #[test]
    #[ignore = "checks a previously purchased Instant AFK allowance without spending diamonds"]
    fn connected_emulator_daily_continues_after_paid_hourglasses() {
        use crate::routine::{Action, Routine};

        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = AtomicBool::new(false);
        let mut cursor = Cursor::default();
        // Keep the test non-spending even if the game rolls over to a new day.
        let settings = DiamondHourglassSettings {
            purchases: 4,
            budget: 49,
        };
        let routine = Routine {
            steps: vec![Action::DiamondHourglasses, Action::StageRewards],
            diamond_hourglasses: settings,
            ..Routine::default()
        };
        let mut logs = Vec::new();
        let mut next_step = false;
        let collected = routine
            .run(&cancelled, |_, action| {
                if action == Action::StageRewards {
                    next_step = true;
                    return Ok(0);
                }
                run(
                    &serial,
                    GameMode::DiamondHourglasses,
                    settings,
                    &mut cursor,
                    &cancelled,
                    &mut |line| {
                        println!("{line}");
                        logs.push(line);
                    },
                    &mut |_| {},
                )
                .map_err(String::from)
            })
            .unwrap();
        assert!(logs
            .iter()
            .any(|line| line == "The daily hourglass count or diamond budget has been reached."));
        assert_eq!(collected, 0);
        assert!(cursor.pending.is_none());
        assert!(cursor.after_purchase.is_none());
        assert!(next_step);
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
