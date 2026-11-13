//! Dream Realm challenges and the previous day's ranking reward.
use super::{afk_stages as common, ocr, BATTLE_RESULT_TIMEOUT, BATTLE_TIMEOUT_SECONDS};
use crate::{
    activity::{ActivityEvent, BattleOutcome, GameMode},
    adb,
    localization::{format_text, text},
};
use image::GrayImage;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const MODE: GameMode = GameMode::DreamRealm;
const FOOTER: common::Rect = region(195, 1680, 885, 212);
const CLOSE: common::Rect = region(300, 1700, 450, 180);
const RANKINGS: common::Rect = region(915, 125, 165, 140);
const RANKING_GIFT: common::Rect = region(1000, 109, 80, 72);
const RANKING_BACK: common::Rect = region(0, 1740, 220, 180);
const SHORT_WAIT: Duration = Duration::from_secs(20);
pub const MAX_BATTLES: u32 = 30;
const DEFAULT_BATTLES: u32 = 2;
const BATTLE_LIMIT_FILE: &str = "dream-realm-battle-limit.txt";

const fn region(x: u32, y: u32, width: u32, height: u32) -> common::Rect {
    common::Rect {
        x,
        y,
        width,
        height,
    }
}

#[derive(Default)]
pub struct Cursor {
    checked_rankings: bool,
    confirming_gold: bool,
    launched: u32,
    completed: u32,
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub daily: bool,
    pub spend_gold: bool,
    pub max_battles: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            daily: false,
            spend_gold: false,
            max_battles: DEFAULT_BATTLES,
        }
    }
}

pub fn load_battle_limit() -> Result<u32, String> {
    let path = crate::settings::file(BATTLE_LIMIT_FILE).map_err(|error| error.to_string())?;
    match std::fs::read_to_string(path) {
        Ok(value) => value
            .trim()
            .parse::<u32>()
            .map(|limit| limit.clamp(1, MAX_BATTLES))
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT_BATTLES),
        Err(error) => Err(error.to_string()),
    }
}

pub fn save_battle_limit(limit: u32) -> std::io::Result<()> {
    crate::settings::write(
        BATTLE_LIMIT_FILE,
        limit.clamp(1, MAX_BATTLES).to_string().as_bytes(),
    )
}

struct GoldOffer {
    price: u32,
    remaining: u32,
    confirm: Option<(u32, u32)>,
}

struct Frame {
    screen: GrayImage,
    size: (u32, u32),
    lines: Vec<ocr::TextLine>,
}

impl Frame {
    fn read(serial: &str, cancelled: &AtomicBool) -> Result<Self, String> {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        let lines = ocr::recognize(&screen)?;
        Ok(Self {
            screen,
            size,
            lines,
        })
    }

    fn point(
        &self,
        path: &str,
        roi: common::Rect,
        confidence: f32,
    ) -> Result<Option<(u32, u32)>, String> {
        Ok(common::find_template(&self.screen, path, roi, confidence)?.map(common::Match::center))
    }

    fn label(&self, roi: common::Rect, matches: impl Fn(&str) -> bool) -> Option<(u32, u32)> {
        self.lines.iter().find_map(|line| {
            let (x, y) = line.center();
            (x >= roi.x
                && x < roi.x + roi.width
                && y >= roi.y
                && y < roi.y + roi.height
                && matches(&compact(&line.text)))
            .then_some((x, y))
        })
    }

    fn tap(&self, serial: &str, point: (u32, u32), cancelled: &AtomicBool) -> Result<(), String> {
        common::ensure_not_cancelled(cancelled)?;
        adb::tap_game_point(serial, point.0, point.1, self.size).map_err(|e| e.to_string())
    }

    fn home(&self) -> Result<Option<(u32, u32)>, String> {
        // The boss title changes daily. The Monsters control distinguishes
        // the overview from other screens with a Rankings icon.
        if self
            .label(region(900, 520, 180, 160), |s| s == "monsters")
            .is_none()
        {
            return Ok(None);
        }
        self.point("dream_realm/dr_ranking.png", RANKINGS, 0.82)
    }

    fn ranked(&self) -> Result<bool, String> {
        let badge = region(75, 1310, 350, 112);
        Ok(self.label(badge, |s| s == "serverranking").is_some()
            || self
                .point("dream_realm/daily_done.png", badge, 0.85)?
                .is_some())
    }

    fn remaining(&self) -> Result<Option<u32>, String> {
        let counts: Vec<_> = self
            .lines
            .iter()
            .filter_map(|line| {
                let (x, y) = line.center();
                ((440..810).contains(&x) && (1795..1860).contains(&y))
                    .then(|| chance_count(&line.text))
                    .flatten()
            })
            .collect();
        if let [count] = counts.as_slice() {
            return Ok(Some(*count));
        }
        Ok(self
            .point("dream_realm/done.png", region(475, 1795, 320, 70), 0.9)?
            .map(|_| 0))
    }

    fn rankings(&self) -> bool {
        self.label(region(0, 0, 530, 205), |s| s.starts_with("dream"))
            .is_some()
            && self
                .label(region(250, 1835, 330, 85), |s| s == "serverrankings")
                .is_some()
            && self
                .label(region(675, 1835, 395, 85), |s| s == "districtrankings")
                .is_some()
    }

    fn ranking_gift(&self) -> Result<bool, String> {
        Ok(self
            .point(
                "dailies/hamburger/rewards_available.png",
                RANKING_GIFT,
                0.85,
            )?
            .is_some())
    }

    fn rankings_back(&self) -> Result<Option<(u32, u32)>, String> {
        self.point("navigation/resonating_hall_back.png", RANKING_BACK, 0.86)
    }

    fn close_prompt(&self) -> Result<Option<(u32, u32)>, String> {
        if let Some(point) = self.label(CLOSE, |s| s == "taptoclose") {
            return Ok(Some(point));
        }
        Ok(common::get_match(
            &self.screen,
            &["tap_to_close.png", "afk_stages/tap_to_close.png"],
            CLOSE,
            0.84,
        )?
        .map(|(_, found)| found.center()))
    }

    fn reward_close(&self, ranking: bool) -> Result<Option<(u32, u32)>, String> {
        let verified = if ranking {
            self.label(region(20, 315, 1040, 350), |s| {
                s.contains("rewardsobtained")
                    || s.contains("dailydreamrealmrewards")
                    || s.starts_with("obtainedtimelimiteditem")
            })
            .is_some()
        } else {
            self.label(region(20, 300, 1040, 235), |s| s == "victoryrewards")
                .is_some()
        };
        if !verified {
            return Ok(None);
        }
        // The close label can be dimmed on the verified rewards overlay.
        Ok(Some(self.close_prompt()?.unwrap_or((540, 1815))))
    }

    fn battle_summary(&self) -> Result<bool, String> {
        let details = self
            .label(region(120, 1310, 590, 130), |s| s.contains("damagedealt"))
            .is_some()
            && self
                .label(region(310, 1385, 475, 100), |s| {
                    s.contains("progressrewards")
                })
                .is_some();
        Ok(details
            && self
                .point(
                    "dream_realm/battle_ended.png",
                    region(545, 270, 520, 230),
                    0.86,
                )?
                .is_some())
    }

    fn formation_control(&self) -> Result<Option<((u32, u32), bool)>, String> {
        if self
            .point("battle/records.png", region(205, 1695, 200, 190), 0.82)?
            .is_none()
        {
            return Ok(None);
        }
        let Some(battle) = self.point("battle/battle.png", region(410, 1715, 650, 150), 0.8)?
        else {
            return Ok(None);
        };
        let skip = self.point(
            "dream_realm/skip_formation.png",
            region(420, 1715, 300, 155),
            0.86,
        )?;
        Ok(Some((skip.unwrap_or(battle), skip.is_some())))
    }

    fn gold_offer(&self) -> Result<Option<GoldOffer>, String> {
        if self
            .label(region(250, 920, 580, 64), |s| s == "realmchallengeattempt")
            .is_none()
        {
            return Ok(None);
        }
        let price = self.lines.iter().find_map(|line| {
            let (_, y) = line.center();
            if !(850..930).contains(&y) {
                return None;
            }
            let prompt = compact(&line.text);
            let price: u32 = prompt
                .strip_prefix("use")?
                .strip_suffix("topurchaseadream")?
                .parse()
                .ok()?;
            (price > 0).then_some(price)
        });
        let remaining = self.lines.iter().find_map(|line| {
            let (_, y) = line.center();
            if !(985..1065).contains(&y) {
                return None;
            }
            attempt_count(compact(&line.text).strip_prefix("dailyattempts")?)
        });
        let (Some(price), Some(remaining)) = (price, remaining) else {
            return Ok(None);
        };
        if self
            .point(
                "dream_realm/gold_price.png",
                region(225, 840, 210, 90),
                0.91,
            )?
            .is_none()
        {
            return Ok(None);
        }
        let confirm = self.point("dream_realm/buy.png", region(777, 1200, 204, 218), 0.86)?;
        Ok(Some(GoldOffer {
            price,
            remaining,
            confirm,
        }))
    }

    fn battle_button(&self) -> Result<(u32, u32), String> {
        self.point("battle/battle.png", FOOTER, 0.8)?
            .ok_or_else(|| {
                text(
                    "The Dream Realm Battle button could not be verified.",
                    "Der Dream-Realm-Battle-Button konnte nicht bestätigt werden.",
                )
                .to_owned()
            })
    }
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn chance_count(value: &str) -> Option<u32> {
    let normalized = compact(value);
    attempt_count(normalized.strip_prefix("chances")?)
}

fn attempt_count(value: &str) -> Option<u32> {
    let (left, right) = value.split_once('/')?;
    let available: u32 = left.parse().ok()?;
    let max: u32 = right.parse().ok()?;
    (max > 0 && max <= 99 && available <= max).then_some(available)
}

fn purchase_chance(
    serial: &str,
    home: &Frame,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Option<Frame>, String> {
    home.tap(serial, home.battle_button()?, cancelled)?;
    let start = Instant::now();
    while start.elapsed() < SHORT_WAIT {
        common::settle_cancellable(cancelled)?;
        let frame = Frame::read(serial, cancelled)?;
        let Some(offer) = frame.gold_offer()? else {
            continue;
        };
        if offer.remaining == 0 {
            common::ensure_not_cancelled(cancelled)?;
            adb::press_back(serial).map_err(|e| e.to_string())?;
            log(text(
                "No extra Dream Realm chances remain.",
                "Keine zusätzlichen Dream-Realm-Versuche mehr verfügbar.",
            )
            .into());
            return Ok(None);
        }
        let Some(confirm) = offer.confirm else {
            continue;
        };
        // An ADB failure can occur after the tap was delivered. Keep the
        // receipt until a purchased chance or its team screen is visible.
        cursor.confirming_gold = true;
        frame.tap(serial, confirm, cancelled)?;
        let purchased = Instant::now();
        while purchased.elapsed() < SHORT_WAIT {
            common::settle_cancellable(cancelled)?;
            let next = Frame::read(serial, cancelled)?;
            if (next.home()?.is_some() && next.remaining()?.is_some_and(|n| n > 0))
                || next.formation_control()?.is_some()
            {
                cursor.confirming_gold = false;
                log(format_text!(
                    "Bought an extra Dream Realm chance for {} Gold.",
                    "Zusätzlicher Dream-Realm-Versuch für {} Gold gekauft.",
                    offer.price
                ));
                return Ok(Some(next));
            }
        }
        return Err(unconfirmed_purchase());
    }
    Err(text(
        "A Dream Realm offer with a verified Gold price was not found.",
        "Es wurde kein Dream-Realm-Angebot mit bestätigtem Goldpreis gefunden.",
    )
    .into())
}

fn unconfirmed_purchase() -> String {
    text(
        "The extra Dream Realm chance could not be confirmed.",
        "Der zusätzliche Dream-Realm-Versuch konnte nicht bestätigt werden.",
    )
    .into()
}

fn wait_home(serial: &str, cancelled: &AtomicBool) -> Result<Frame, String> {
    let start = Instant::now();
    while start.elapsed() < SHORT_WAIT {
        let frame = Frame::read(serial, cancelled)?;
        if frame.home()?.is_some() {
            return Ok(frame);
        }
        common::settle_cancellable(cancelled)?;
    }
    Err(text(
        "The Dream Realm overview could not be verified.",
        "Die Dream-Realm-Übersicht konnte nicht bestätigt werden.",
    )
    .into())
}

fn navigate(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<Frame, String> {
    let frame = Frame::read(serial, cancelled)?;
    if frame.home()?.is_some() {
        return Ok(frame);
    }
    if frame.rankings() {
        return return_from_rankings(serial, frame, cancelled);
    }
    common::navigate_to_battle_modes(serial, cancelled, log)?;
    let (_, entry, size) = common::wait_for_match(
        serial,
        &["dream_realm/label.png"],
        region(340, 525, 410, 310),
        SHORT_WAIT,
        cancelled,
    )?;
    common::ensure_not_cancelled(cancelled)?;
    common::tap_match(serial, entry, size)?;
    wait_home(serial, cancelled)
}

fn return_from_rankings(
    serial: &str,
    mut frame: Frame,
    cancelled: &AtomicBool,
) -> Result<Frame, String> {
    let start = Instant::now();
    while start.elapsed() < SHORT_WAIT {
        if let Some(close) = frame.reward_close(true)? {
            frame.tap(serial, close, cancelled)?;
        } else if frame.home()?.is_some() {
            return Ok(frame);
        } else if frame.rankings() {
            if let Some(back) = frame.rankings_back()? {
                frame.tap(serial, back, cancelled)?;
            } else {
                common::ensure_not_cancelled(cancelled)?;
                adb::press_back(serial).map_err(|e| e.to_string())?;
            }
        }
        common::settle_cancellable(cancelled)?;
        frame = Frame::read(serial, cancelled)?;
    }
    Err(text(
        "Could not return from the rankings to the Dream Realm overview.",
        "Die Rückkehr von der Rangliste zur Dream-Realm-Übersicht konnte nicht bestätigt werden.",
    )
    .into())
}

fn claim_yesterday(
    serial: &str,
    home: &Frame,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    progress: &mut dyn FnMut(ActivityEvent),
) -> Result<Frame, String> {
    let point = home.home()?.ok_or("Dream Realm rankings entry missing.")?;
    let gift_available = home.ranking_gift()?;
    home.tap(serial, point, cancelled)?;
    let start = Instant::now();
    let mut reward_seen = false;
    while start.elapsed() < SHORT_WAIT {
        common::settle_cancellable(cancelled)?;
        let frame = Frame::read(serial, cancelled)?;
        if let Some(close) = frame.reward_close(true)? {
            frame.tap(serial, close, cancelled)?;
            reward_seen = true;
        } else if frame.rankings()
            || (gift_available && frame.home()?.is_some() && !frame.ranking_gift()?)
        {
            let home = return_from_rankings(serial, frame, cancelled)?;
            if reward_seen || (gift_available && !home.ranking_gift()?) {
                progress(ActivityEvent::Collected(MODE, 1));
            }
            cursor.checked_rankings = true;
            return Ok(home);
        }
    }
    Err(text(
        "The Dream Realm ranking rewards could not be verified.",
        "Die Dream-Realm-Ranglistenbelohnung konnte nicht bestätigt werden.",
    )
    .into())
}

fn formation(serial: &str, cancelled: &AtomicBool) -> Result<(Frame, (u32, u32), bool), String> {
    let start = Instant::now();
    while start.elapsed() < SHORT_WAIT {
        common::settle_cancellable(cancelled)?;
        let frame = Frame::read(serial, cancelled)?;
        if let Some((button, skipped)) = frame.formation_control()? {
            return Ok((frame, button, skipped));
        }
    }
    Err(text(
        "The Dream Realm team or Battle button could not be verified.",
        "Die Dream-Realm-Aufstellung oder der Battle-Button konnte nicht bestätigt werden.",
    )
    .into())
}

fn finish_battle(
    serial: &str,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    progress: &mut dyn FnMut(ActivityEvent),
) -> Result<Frame, String> {
    let start = Instant::now();
    let mut scored = false;
    let mut skip_sent = false;
    while start.elapsed() < Duration::from_secs(BATTLE_TIMEOUT_SECONDS) {
        common::settle_cancellable(cancelled)?;
        let frame = Frame::read(serial, cancelled)?;
        if frame.battle_summary()? {
            if !scored {
                scored = true;
                cursor.completed += 1;
                progress(ActivityEvent::BattleFinished(MODE, BattleOutcome::Scored));
            }
            if let Some(close) = frame.close_prompt()? {
                frame.tap(serial, close, cancelled)?;
            }
        } else if let Some(close) = frame.reward_close(false)? {
            frame.tap(serial, close, cancelled)?;
        } else if frame.home()?.is_some() {
            if !scored {
                progress(ActivityEvent::BattleFinished(MODE, BattleOutcome::Unclear));
                return Err(text(
                    "Dream Realm returned without a confirmed battle result.",
                    "Dream Realm ist ohne bestätigtes Kampfergebnis zur Übersicht zurückgekehrt.",
                )
                .into());
            }
            return Ok(frame);
        } else if !scored
            && !skip_sent
            && frame
                .label(region(0, 355, 460, 155), |s| s.contains("totaldamage"))
                .is_some()
        {
            if let Some((_, skip)) = common::get_match(
                &frame.screen,
                &["battle/skip_orange.png", "battle/skip.png"],
                FOOTER,
                0.86,
            )? {
                frame.tap(serial, skip.center(), cancelled)?;
                skip_sent = true;
            }
        }
    }
    Err(BATTLE_RESULT_TIMEOUT.into())
}

pub fn run(
    serial: &str,
    settings: Settings,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    progress: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    let limit = settings.max_battles.clamp(1, MAX_BATTLES);
    if cursor.launched >= limit {
        return Ok(cursor.completed);
    }
    progress(ActivityEvent::ModeOpening(MODE));
    let mut home = navigate(serial, cancelled, log)?;
    progress(ActivityEvent::ModeEntered(MODE));
    if cursor.confirming_gold {
        if !home.remaining()?.is_some_and(|n| n > 0) {
            return Err(unconfirmed_purchase());
        }
        cursor.confirming_gold = false;
    }
    if settings.daily && !cursor.checked_rankings {
        home = claim_yesterday(serial, &home, cursor, cancelled, progress)?;
    }
    while cursor.launched < limit {
        common::ensure_not_cancelled(cancelled)?;
        if settings.daily && home.ranked()? {
            log(text(
                "Today's Dream Realm ranking is already recorded.",
                "Die heutige Dream-Realm-Platzierung steht bereits fest.",
            )
            .into());
            break;
        }
        let remaining = home.remaining()?.ok_or_else(|| {
            text(
                "The remaining Dream Realm chances could not be read.",
                "Die verbleibenden Dream-Realm-Versuche konnten nicht gelesen werden.",
            )
            .to_owned()
        })?;
        if remaining == 0 && !settings.spend_gold {
            log(text(
                "No free Dream Realm chances remain.",
                "Keine kostenlosen Dream-Realm-Versuche mehr verfügbar.",
            )
            .into());
            break;
        }
        let ready = if remaining == 0 {
            let Some(next) = purchase_chance(serial, &home, cursor, cancelled, log)? else {
                break;
            };
            next
        } else {
            home
        };
        let (team, battle, skipped) = if let Some((button, skip)) = ready.formation_control()? {
            (ready, button, skip)
        } else {
            ready.tap(serial, ready.battle_button()?, cancelled)?;
            formation(serial, cancelled)?
        };
        common::ensure_not_cancelled(cancelled)?;
        // Reserve the attempt before sending input: ADB can report an error
        // after the game has already received the tap.
        cursor.launched += 1;
        team.tap(serial, battle, cancelled)?;
        progress(ActivityEvent::BattleStarted(MODE));
        if skipped {
            log(text(
                "Using the game's Dream Realm Skip button.",
                "Die Skip-Schaltfläche von Dream Realm wird verwendet.",
            )
            .into());
        }
        log(format_text!(
            "Dream Realm battle {} started.",
            "Dream-Realm-Kampf {} gestartet.",
            cursor.launched
        ));
        home = finish_battle(serial, cursor, cancelled, progress)?;
    }
    Ok(cursor.completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chances_must_be_explicit_and_valid() {
        for (label, count) in [
            ("Chances: 10/10", Some(10)),
            ("Chances: 0 / 10", Some(0)),
            ("Chances: 2/0", None),
            ("Chances: 11/10", None),
            ("Chances: O/10", None),
            ("Rank 2/10", None),
            ("Chances: 2/10 Gold", None),
        ] {
            assert_eq!(chance_count(label), count, "{label}");
        }
    }

    #[test]
    fn recovery_does_not_start_more_battles_than_the_configured_limit() {
        let defaults = Settings::default();
        assert_eq!(defaults.max_battles, 2);
        for settings in [
            defaults,
            Settings {
                max_battles: 4,
                ..defaults
            },
        ] {
            let mut cursor = Cursor {
                launched: settings.max_battles,
                completed: settings.max_battles - 1,
                ..Cursor::default()
            };
            let completed = run(
                "invalid-device",
                settings,
                &mut cursor,
                &AtomicBool::new(false),
                &mut |_| {},
                &mut |event| panic!("unexpected activity after reaching the limit: {event:?}"),
            )
            .unwrap();
            assert_eq!(completed, settings.max_battles - 1);
        }
    }

    #[test]
    #[ignore = "verifies Dream Realm from captured AFK Journey views"]
    fn captured_dream_realm_views() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ADBHELPER_TEST_DREAM_CAPTURES").expect("capture folder"),
        );
        let load = |name: &str| {
            let bytes = std::fs::read(root.join(format!("{name}.png"))).unwrap();
            let (screen, size) = common::normalize_screenshot(&bytes).unwrap();
            let lines = ocr::recognize(&screen).unwrap();
            println!("{name}: {lines:?}");
            Frame {
                screen,
                size,
                lines,
            }
        };
        for (name, ranked) in [("home", false), ("home_after", true)] {
            let home = load(name);
            assert!(home.home().unwrap().is_some(), "{name}");
            assert_eq!(home.remaining().unwrap(), Some(10));
            assert_eq!(home.ranked().unwrap(), ranked);
            assert_eq!(home.ranking_gift().unwrap(), !ranked);
            assert!(home
                .point("battle/battle.png", FOOTER, 0.8)
                .unwrap()
                .is_some());
        }
        for name in ["ranking_table", "immediate_claim_rankings"] {
            let ranking = load(name);
            assert!(ranking.rankings(), "{name}");
            assert!(ranking.home().unwrap().is_none(), "{name}");
            assert!(ranking.rankings_back().unwrap().is_some(), "{name}");
            assert!(ranking.reward_close(true).unwrap().is_none(), "{name}");
        }
        let home = load("immediate_claim_android_back");
        assert!(home.home().unwrap().is_some());
        assert!(!home.ranking_gift().unwrap());
        assert_eq!(home.remaining().unwrap(), Some(10));
        let reward = load("ranking");
        assert!(reward.reward_close(true).unwrap().is_some());
        let formation = load("formation_or_battle");
        assert_eq!(formation.formation_control().unwrap().unwrap().1, false);
        let endless = load("endless_formation");
        assert_eq!(endless.formation_control().unwrap().unwrap().1, true);
        for name in ["result_summary", "endless_summary"] {
            let summary = load(name);
            assert!(summary.battle_summary().unwrap(), "{name}");
            assert!(summary.close_prompt().unwrap().is_some(), "{name}");
        }
        let reward = load("victory_rewards");
        assert!(reward.reward_close(false).unwrap().is_some());
        assert!(!reward.battle_summary().unwrap());
        let exhausted = load("exhausted");
        assert!(exhausted.home().unwrap().is_some());
        assert_eq!(exhausted.remaining().unwrap(), Some(0));
        assert!(exhausted.gold_offer().unwrap().is_none());
        let mut purchase = load("gold_offer");
        let offer = purchase
            .gold_offer()
            .unwrap()
            .expect("verified Dream Realm Gold offer");
        assert_eq!((offer.price, offer.remaining), (10_000, 10));
        let (x, y) = offer.confirm.expect("verified purchase button");
        assert!((777..981).contains(&x) && (1200..1418).contains(&y));
        for y in 865..908 {
            for x in 286..328 {
                purchase.screen.put_pixel(x, y, image::Luma([0]));
            }
        }
        assert!(purchase.gold_offer().unwrap().is_none());
    }

    #[test]
    #[ignore = "checks ranking collection and return to the Dream Realm overview without starting a battle"]
    fn connected_emulator_dream_ranking_return() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("device serial");
        let cancelled = AtomicBool::new(false);
        let mut log = |line| println!("{line}");
        let home = navigate(&serial, &cancelled, &mut log).unwrap();
        let mut cursor = Cursor::default();
        let mut collected = 0;
        let before = home.ranking_gift().unwrap();
        let home = claim_yesterday(&serial, &home, &mut cursor, &cancelled, &mut |event| {
            if let ActivityEvent::Collected(MODE, count) = event {
                collected += count;
            }
        })
        .unwrap();
        assert!(cursor.checked_rankings);
        assert!(home.home().unwrap().is_some());
        assert!(home.remaining().unwrap().is_some());
        assert_eq!(collected, u32::from(before));
        println!("Returned to Dream Realm with {:?} available chances and {collected} confirmed reward(s).", home.remaining().unwrap());
    }

    #[test]
    #[ignore = "runs Dream Realm with free chances on a connected emulator"]
    fn connected_emulator_dream_realm() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("device serial");
        let daily = std::env::var("ADBHELPER_TEST_DREAM_DAILY").as_deref() == Ok("1");
        let limit = std::env::var("ADBHELPER_TEST_DREAM_LIMIT")
            .map_or(DEFAULT_BATTLES, |value| value.parse().expect("valid limit"));
        let cancelled = AtomicBool::new(false);
        let mut cursor = Cursor::default();
        let total = super::super::recovery::run(
            &serial,
            &cancelled,
            |_, report| {
                run(
                    &serial,
                    Settings {
                        daily,
                        max_battles: limit,
                        ..Settings::default()
                    },
                    &mut cursor,
                    &cancelled,
                    &mut |line| println!("{line}"),
                    report,
                )
            },
            |message| println!("{message}"),
            |event| println!("{event:?}"),
        )
        .unwrap();
        println!(
            "Confirmed Dream Realm activity: {total}, battles: {}",
            cursor.completed
        );
        assert_eq!(cursor.launched, cursor.completed);
        if daily {
            assert!(cursor.checked_rankings);
        } else {
            assert!(cursor.completed > 0);
        }
    }
}
