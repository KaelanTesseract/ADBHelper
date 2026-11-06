//! Daily PvP workflows ported from AdbAutoPlayer's arena and supreme_arena mixins.
//! These use the player's existing formations, never PvE team-copy/retry logic.
use super::afk_stages::{self as common, Match, Rect};
use super::ocr::{self, TextLine};
use crate::activity::{ActivityEvent, BattleOutcome, GameMode};
use crate::adb;
use crate::localization::{format_text, text};
use image::GrayImage;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Arena,
    Supreme,
}

impl Kind {
    pub fn mode(self) -> GameMode {
        match self {
            Self::Arena => GameMode::Arena,
            Self::Supreme => GameMode::SupremeArena,
        }
    }
    fn challenge(self) -> &'static str {
        match self {
            Self::Arena => "arena/challenge.png",
            Self::Supreme => "supreme_arena/challenge.png",
        }
    }
    fn entry(self) -> &'static str {
        match self {
            Self::Arena => "arena/label.png",
            Self::Supreme => "battle_modes/supreme_arena.png",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpponentPosition {
    Left,
    Middle,
    Right,
}
impl OpponentPosition {
    pub const ALL: [Self; 3] = [Self::Left, Self::Middle, Self::Right];
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => text("Left", "Links"),
            Self::Middle => text("Middle", "Mitte"),
            Self::Right => text("Right", "Rechts"),
        }
    }
    fn point(self) -> (u32, u32) {
        (
            match self {
                Self::Left => 165,
                Self::Middle => 540,
                Self::Right => 915,
            },
            950,
        )
    }
}

#[derive(Clone, Copy)]
pub struct ArenaSettings {
    pub attempts: u32,
    pub claim_free_attempts: bool,
}
impl Default for ArenaSettings {
    fn default() -> Self {
        Self {
            attempts: 5,
            claim_free_attempts: true,
        }
    }
}
#[derive(Clone, Copy)]
pub struct SupremeSettings {
    pub attempts: u32,
    pub opponent: OpponentPosition,
}
impl Default for SupremeSettings {
    fn default() -> Self {
        Self {
            attempts: 5,
            opponent: OpponentPosition::Left,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Settings {
    Arena(ArenaSettings),
    Supreme(SupremeSettings),
}
impl Settings {
    fn kind(self) -> Kind {
        match self {
            Self::Arena(_) => Kind::Arena,
            Self::Supreme(_) => Kind::Supreme,
        }
    }
    fn attempts(self) -> u32 {
        match self {
            Self::Arena(s) => s.attempts.clamp(1, 5),
            Self::Supreme(s) => s.attempts.clamp(1, 10),
        }
    }
    fn free_claims(self) -> u32 {
        match self {
            Self::Arena(s) if s.claim_free_attempts => 2,
            _ => 0,
        }
    }
    fn opponent(self) -> OpponentPosition {
        match self {
            Self::Arena(_) => OpponentPosition::Left,
            Self::Supreme(s) => s.opponent,
        }
    }
}

fn footer() -> Rect {
    Rect {
        x: 0,
        y: 1440,
        width: 1080,
        height: 480,
    }
}
fn popup() -> Rect {
    Rect {
        x: 0,
        y: 380,
        width: 1080,
        height: 1120,
    }
}
fn find(screen: &GrayImage, path: &str, roi: Rect) -> Result<Option<Match>, String> {
    common::find_template(screen, path, roi, 0.85)
}
fn purchase_popup(screen: &GrayImage) -> Result<bool, String> {
    Ok(find(screen, "supreme_arena/no_attempts_popup.png", popup())?.is_some())
}
fn confirmation(screen: &GrayImage) -> Result<bool, String> {
    Ok(common::get_match(
        screen,
        &["navigation/confirm.png", "confirm_text.png", "cancel.png"],
        common::lower_screen(),
        0.85,
    )?
    .is_some())
}

fn home_control(screen: &GrayImage, kind: Kind) -> Result<Option<(String, Match)>, String> {
    let label = match kind {
        Kind::Arena => "arena/home_label.png",
        Kind::Supreme => "supreme_arena/label.png",
    };
    let header = Rect {
        x: 0,
        y: 0,
        width: 790,
        height: 180,
    };
    if find(screen, label, header)?.is_none() {
        return Ok(None);
    }
    common::get_match(
        screen,
        &[kind.challenge(), "arena/continue.png"],
        footer(),
        0.85,
    )
}

fn attempt_count(value: &str) -> Option<u32> {
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let (remaining, cost) = compact.split_once('/')?;
    if cost != "1" || !remaining.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    remaining.parse().ok()
}

fn challenge_attempts(screen: &GrayImage) -> Result<Option<u32>, String> {
    // Read only the ratio beside the ticket. Matching the whole badge made
    // the shared ticket and "/1" pixels outweigh the difference between 0 and 5.
    let padded = GrayImage::from_fn(106, 83, |x, y| {
        let ink = (18..88).contains(&x)
            && (18..65).contains(&y)
            && screen.get_pixel(714 + x - 18, 1817 + y - 18).0[0] >= 130;
        image::Luma([if ink { 0 } else { 255 }])
    });
    let enlarged =
        image::imageops::resize(&padded, 424, 332, image::imageops::FilterType::CatmullRom);
    let lines = ocr::recognize(&enlarged)?;
    let mut counts = lines.iter().filter_map(|line| attempt_count(&line.text));
    let count = counts.next();
    Ok(count.filter(|first| counts.all(|next| next == *first)))
}

fn navigate(
    serial: &str,
    kind: Kind,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    common::ensure_not_cancelled(cancelled)?;
    common::navigate_to_battle_modes(serial, cancelled, log)?;
    for swipe in 0..=4 {
        let (screen, size) = common::screenshot(serial)?;
        // The regular Arena is the upper-right mode card. Restrict its short
        // "Arena" label so it cannot select the Supreme Arena card below it.
        let region = if kind == Kind::Arena {
            Rect {
                x: 648,
                y: 0,
                width: 432,
                height: 1100,
            }
        } else {
            common::full_screen()
        };
        if let Some(entry) = find(&screen, kind.entry(), region)? {
            common::ensure_not_cancelled(cancelled)?;
            common::tap_match(serial, entry, size)?;
            common::settle_cancellable(cancelled)?;
            return wait_home(serial, kind, cancelled).map(|_| ());
        }
        if swipe < 4 && kind == Kind::Supreme {
            adb::swipe_game_points(serial, (540, 1200), (540, 700), size, 1500)
                .map_err(|e| e.to_string())?;
            common::settle_cancellable(cancelled)?;
        } else {
            break;
        }
    }
    Err(format_text!(
        "{} is unavailable in Battle Modes.",
        "{} ist in den Kampfmodi nicht verfügbar.",
        kind.mode().label()
    ))
}

fn dismiss_notice(
    serial: &str,
    kind: Kind,
    screen: &GrayImage,
    size: (u32, u32),
) -> Result<bool, String> {
    if let Some(close) = find(screen, "tap_to_close.png", common::lower_screen())? {
        common::tap_match(serial, close, size)?;
        return Ok(true);
    }
    if kind == Kind::Arena
        && common::get_match(
            screen,
            &["arena/weekly_rewards.png", "arena/weekly_notice.png"],
            common::full_screen(),
            0.85,
        )?
        .is_some()
    {
        adb::press_back(serial).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    if kind == Kind::Supreme {
        if let Some(rewards) = find(
            screen,
            "supreme_arena/daily_rewards.png",
            common::lower_screen(),
        )? {
            common::tap_match(serial, rewards, size)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn wait_home(
    serial: &str,
    kind: Kind,
    cancelled: &AtomicBool,
) -> Result<(GrayImage, (u32, u32)), String> {
    for _ in 0..12 {
        common::ensure_not_cancelled(cancelled)?;
        let (screen, size) = common::screenshot(serial)?;
        if purchase_popup(&screen)? || confirmation(&screen)? {
            adb::press_back(serial).map_err(|e| e.to_string())?;
            return Err(text(
                "The Arena dialog could not be verified. No purchase was made.",
                "Arena-Dialog nicht eindeutig bestätigt; kein Kauf ausgeführt.",
            )
            .into());
        }
        if home_control(&screen, kind)?.is_some() {
            return Ok((screen, size));
        }
        dismiss_notice(serial, kind, &screen, size)?;
        common::settle_cancellable(cancelled)?;
    }
    Err(format_text!(
        "The {} overview was not detected.",
        "{}-Übersicht nicht erkannt.",
        kind.mode().label()
    ))
}

/// Mirrors the original Spend + diamond + 0 prompt without treating 10/20 as 0.
fn zero_cost(lines: &[TextLine]) -> bool {
    let costs = lines
        .iter()
        .filter(|line| {
            line.y > 380.0 && line.y < 1500.0 && line.text.to_lowercase().contains("spend")
        })
        .map(|line| {
            line.text
                .split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    costs.len() == 1 && costs[0] == ["0"]
}

fn verified_free_offer(screen: &GrayImage) -> Result<bool, String> {
    let Some(price) = common::find_template(screen, "arena/buy_free.png", popup(), 0.95)? else {
        return Ok(false);
    };
    let lines = ocr::recognize(screen)?;
    if zero_cost(&lines) {
        return Ok(true);
    }
    // Windows OCR can read the isolated cost digit 0 as O. Do not substitute
    // characters: require an exact, independent match of the zero glyph itself.
    // Any explicit numeric nonzero/ambiguous cost in the caption still blocks it.
    let captions = lines
        .iter()
        .filter(|line| {
            line.y > 380.0
                && line.y < 1500.0
                && line.text.to_lowercase().contains("spend")
                && line.text.to_lowercase().contains("purchase")
        })
        .collect::<Vec<_>>();
    if captions.len() != 1 || captions[0].text.bytes().any(|b| b.is_ascii_digit()) {
        return Ok(false);
    }
    let rect = price.rect();
    Ok(common::find_template(
        screen,
        "arena/free_price_zero.png",
        Rect {
            x: rect.x + 195,
            y: rect.y,
            width: 29,
            height: 36,
        },
        0.98,
    )?
    .is_some())
}

fn result_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect()
}

fn result_outcome(lines: &[TextLine]) -> BattleOutcome {
    let headings = lines
        .iter()
        .filter(|line| line.y < 1050.0 && line.height >= 45.0)
        .filter_map(|line| match result_text(&line.text).as_str() {
            "victory" | "win" | "youwin" | "sieg" => Some(true),
            "defeat" | "defeated" | "youlose" | "niederlage" => Some(false),
            _ => None,
        })
        .collect::<Vec<_>>();
    // The decorative VICTORY title is often unreadable to OCR. This banner
    // explicitly identifies a win on the results screen.
    let victory_rewards = lines.iter().any(|line| {
        let (x, y) = line.center();
        (325..=755).contains(&x)
            && (1350..=1560).contains(&y)
            && result_text(&line.text) == "victoryrewards"
    });
    match (
        headings.contains(&true) || victory_rewards,
        headings.contains(&false),
    ) {
        (true, false) => BattleOutcome::Won,
        (false, true) => BattleOutcome::Lost,
        _ => BattleOutcome::Unclear,
    }
}

fn result_prompt(lines: &[TextLine], outcome: BattleOutcome) -> Option<(u32, u32)> {
    let close = lines.iter().find(|line| {
        let (x, y) = line.center();
        (320..=760).contains(&x)
            && (1650..=1890).contains(&y)
            && matches!(
                result_text(&line.text).as_str(),
                "taptoclose" | "tapanywheretoclose" | "tippenzumschließen"
            )
    })?;
    if matches!(outcome, BattleOutcome::Unclear) {
        // A generic close prompt is also used outside battle. When OCR
        // cannot decide the outcome, require the result screen's two controls.
        let footer_control = |label, x_range: std::ops::RangeInclusive<u32>| {
            lines.iter().any(|line| {
                let (x, y) = line.center();
                x_range.contains(&x)
                    && (1740..=1910).contains(&y)
                    && line.text.trim().eq_ignore_ascii_case(label)
            })
        };
        if !footer_control("Replay", 0..=230) || !footer_control("Details", 850..=1080) {
            return None;
        }
    }
    Some(close.center())
}

fn battle_result(
    screen: &GrayImage,
    seen_battle: bool,
) -> Result<Option<(BattleOutcome, (u32, u32))>, String> {
    let lines = ocr::recognize(screen)?;
    let outcome = result_outcome(&lines);
    if let Some(close) = result_prompt(&lines, outcome) {
        return Ok(Some((outcome, close)));
    }
    if seen_battle || !matches!(outcome, BattleOutcome::Unclear) {
        let button = common::get_match(screen, &["arena/done.png", "next.png"], footer(), 0.85)?;
        return Ok(button.map(|(_, found)| (outcome, found.center())));
    }
    Ok(None)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Choice {
    Ready,
    Exhausted,
}
trait Driver {
    fn choose(&mut self) -> Result<Choice, String>;
    fn battle(&mut self) -> Result<BattleOutcome, String>;
    fn free_attempt(&mut self) -> Result<bool, String>;
    fn checkpoint(&mut self, _: Cursor) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub initial: u32,
    pub initial_done: bool,
    pub claims: u32,
    pub extra: u32,
    pub pending_extra: bool,
}

#[cfg(test)]
fn attempts(
    driver: &mut impl Driver,
    settings: Settings,
    cancelled: &AtomicBool,
) -> Result<u32, String> {
    attempts_from(driver, settings, &mut Cursor::default(), cancelled)
}

fn attempts_from(
    driver: &mut impl Driver,
    settings: Settings,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
) -> Result<u32, String> {
    let mut wins = 0;
    while !cursor.initial_done && cursor.initial < settings.attempts() {
        common::ensure_not_cancelled(cancelled)?;
        // Reserve before opponent selection as well: a crash at any challenge
        // transition must never reset the attempt budget and request an extra.
        cursor.initial += 1;
        driver.checkpoint(*cursor);
        if driver.choose()? == Choice::Exhausted {
            cursor.initial -= 1;
            driver.checkpoint(*cursor);
            break;
        }
        common::ensure_not_cancelled(cancelled)?;
        match driver.battle() {
            Ok(BattleOutcome::Won) => wins += 1,
            Err(error)
                if error == super::BATTLE_RESULT_TIMEOUT
                    && cursor.initial == settings.attempts()
                    && cursor.claims >= settings.free_claims() =>
            {
                return Ok(wins);
            }
            result => {
                result?;
            }
        }
    }
    cursor.initial_done = true;
    driver.checkpoint(*cursor);
    while cursor.pending_extra || cursor.claims < settings.free_claims() {
        common::ensure_not_cancelled(cancelled)?;
        if !cursor.pending_extra {
            cursor.claims += 1;
            cursor.pending_extra = true;
            driver.checkpoint(*cursor);
            if !driver.free_attempt()? {
                cursor.pending_extra = false;
                driver.checkpoint(*cursor);
                break;
            }
        }
        common::ensure_not_cancelled(cancelled)?;
        cursor.pending_extra = false;
        cursor.extra += 1;
        driver.checkpoint(*cursor);
        if driver.choose()? == Choice::Exhausted {
            cursor.extra -= 1;
            driver.checkpoint(*cursor);
            continue;
        }
        common::ensure_not_cancelled(cancelled)?;
        match driver.battle() {
            Ok(BattleOutcome::Won) => wins += 1,
            Err(error)
                if error == super::BATTLE_RESULT_TIMEOUT
                    && cursor.claims >= settings.free_claims() =>
            {
                return Ok(wins);
            }
            result => {
                result?;
            }
        }
    }
    Ok(wins)
}

struct AdbDriver<'a, P> {
    serial: &'a str,
    settings: Settings,
    cancelled: &'a AtomicBool,
    progress: &'a mut P,
}

impl<P: FnMut(ActivityEvent)> AdbDriver<'_, P> {
    fn wait_control(&self, path: &str, roi: Rect) -> Result<(Match, (u32, u32)), String> {
        for _ in 0..14 {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if purchase_popup(&screen)? || confirmation(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                return Err(text(
                    "An unexpected Arena dialog appeared. No confirmation or purchase was made.",
                    "Unerwarteter Arena-Dialog; keine Bestätigung oder zusätzlicher Kauf.",
                )
                .into());
            }
            if let Some(control) = find(&screen, path, roi)? {
                return Ok((control, size));
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Err(format_text!(
            "Arena control {path} was not detected.",
            "Arena-Steuerelement {path} nicht erkannt."
        ))
    }

    fn complete_battle(
        &mut self,
        mut outcome: BattleOutcome,
        close: (u32, u32),
        size: (u32, u32),
    ) -> Result<BattleOutcome, String> {
        let finish = (|| {
            common::ensure_not_cancelled(self.cancelled)?;
            adb::tap_game_point(self.serial, close.0, close.1, size).map_err(|e| e.to_string())?;
            common::settle_cancellable(self.cancelled)?;
            self.finish_result(&mut outcome)
        })();
        (self.progress)(ActivityEvent::BattleFinished(
            self.settings.kind().mode(),
            outcome,
        ));
        finish.map(|()| outcome)
    }

    fn finish_result(&self, outcome: &mut BattleOutcome) -> Result<(), String> {
        for _ in 0..8 {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if purchase_popup(&screen)? || confirmation(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                return Err(text(
                    "The run stopped after an unexpected Arena dialog appeared.",
                    "Unerwarteter Arena-Dialog nach dem Kampf; Lauf beendet.",
                )
                .into());
            }
            if home_control(&screen, self.settings.kind())?.is_some() {
                return Ok(());
            }
            let lines = ocr::recognize(&screen)?;
            let observed = result_outcome(&lines);
            if let Some((x, y)) = result_prompt(&lines, observed) {
                if matches!(*outcome, BattleOutcome::Unclear) {
                    *outcome = observed;
                }
                common::ensure_not_cancelled(self.cancelled)?;
                adb::tap_game_point(self.serial, x, y, size).map_err(|e| e.to_string())?;
            } else {
                dismiss_notice(self.serial, self.settings.kind(), &screen, size)?;
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Err(text(
            "The Arena result screen could not be closed. The run stopped.",
            "Arena-Ergebnis nicht geschlossen; kein weiterer Kampf gestartet.",
        )
        .into())
    }
}

impl<P: FnMut(ActivityEvent)> Driver for AdbDriver<'_, P> {
    fn checkpoint(&mut self, cursor: Cursor) {
        super::recovery::save_arena_cursor(self.serial, cursor);
    }
    fn choose(&mut self) -> Result<Choice, String> {
        common::ensure_not_cancelled(self.cancelled)?;
        let kind = self.settings.kind();
        let (screen, size) = wait_home(self.serial, kind, self.cancelled)?;
        let (path, challenge) = home_control(&screen, kind)?.ok_or(text(
            "Arena Challenge button not detected",
            "Arena-Challenge nicht erkannt",
        ))?;
        if path == kind.challenge() && challenge_attempts(&screen)? == Some(0) {
            return Ok(Choice::Exhausted);
        }
        common::ensure_not_cancelled(self.cancelled)?;
        common::tap_match(self.serial, challenge, size)?;
        common::settle_cancellable(self.cancelled)?;
        for _ in 0..12 {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if purchase_popup(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                return Ok(Choice::Exhausted);
            }
            if confirmation(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                return Err(text(
                    "The Arena confirmation could not be verified. No purchase was made.",
                    "Arena-Bestätigung ist nicht eindeutig; kein Kauf.",
                )
                .into());
            }
            if kind == Kind::Arena {
                if let Some(opponent) = find(
                    &screen,
                    "arena/opponent.png",
                    Rect {
                        x: 0,
                        y: 0,
                        width: 432,
                        height: 1920,
                    },
                )? {
                    common::tap_match(self.serial, opponent, size)?;
                    common::settle_cancellable(self.cancelled)?;
                    return Ok(Choice::Ready);
                }
            } else if find(
                &screen,
                "supreme_arena/select_opponent.png",
                Rect {
                    x: 0,
                    y: 0,
                    width: 1080,
                    height: 700,
                },
            )?
            .is_some()
            {
                let (x, y) = self.settings.opponent().point();
                adb::tap_game_point(self.serial, x, y, size).map_err(|e| e.to_string())?;
                common::settle_cancellable(self.cancelled)?;
                let (detail, size) =
                    self.wait_control("supreme_arena/challenge_detail.png", footer())?;
                common::ensure_not_cancelled(self.cancelled)?;
                common::tap_match(self.serial, detail, size)?;
                common::settle_cancellable(self.cancelled)?;
                return Ok(Choice::Ready);
            }
            dismiss_notice(self.serial, kind, &screen, size)?;
            common::settle_cancellable(self.cancelled)?;
        }
        Err(text(
            "The Arena opponent selection was not detected.",
            "Arena-Gegnerauswahl nicht erkannt.",
        )
        .into())
    }

    fn battle(&mut self) -> Result<BattleOutcome, String> {
        let kind = self.settings.kind();
        // Supreme Arena has exactly three existing formations: Next, Next, Battle.
        if kind == Kind::Supreme {
            for _ in 0..2 {
                let (next, size) = self.wait_control("next.png", footer())?;
                common::ensure_not_cancelled(self.cancelled)?;
                common::tap_match(self.serial, next, size)?;
                common::settle_cancellable(self.cancelled)?;
            }
        }
        let (battle, size) = self.wait_control("arena/battle.png", footer())?;
        common::ensure_not_cancelled(self.cancelled)?;
        let mode = kind.mode();
        (self.progress)(ActivityEvent::BattleStarted(mode));
        common::tap_match(self.serial, battle, size)?;
        common::settle_cancellable(self.cancelled)?;
        let started = Instant::now();
        let mut seen_battle = false;
        while started.elapsed() < Duration::from_secs(super::BATTLE_TIMEOUT_SECONDS) {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, size) = common::screenshot(self.serial)?;
            if purchase_popup(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                return Err(text(
                    "No Arena attempts are available. The purchase was cancelled.",
                    "Keine Arena-Versuche verfügbar; Kauf abgebrochen.",
                )
                .into());
            }
            if let Some((outcome, close)) = battle_result(&screen, seen_battle)? {
                return self.complete_battle(outcome, close, size);
            }
            if confirmation(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                if seen_battle {
                    (self.progress)(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
                }
                return Err(text(
                    "The run stopped after an unknown Arena confirmation appeared.",
                    "Unbekannte Arena-Bestätigung; Lauf beendet.",
                )
                .into());
            }
            if !seen_battle {
                if let Some(skip) = find(&screen, "arena/skip.png", common::full_screen())? {
                    common::ensure_not_cancelled(self.cancelled)?;
                    common::tap_match(self.serial, skip, size)?;
                    seen_battle = true;
                }
            }
            common::settle_cancellable(self.cancelled)?;
        }
        (self.progress)(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
        Err(super::BATTLE_RESULT_TIMEOUT.into())
    }

    fn free_attempt(&mut self) -> Result<bool, String> {
        if self.settings.kind() != Kind::Arena {
            return Ok(false);
        }
        let (screen, size) = wait_home(self.serial, Kind::Arena, self.cancelled)?;
        let Some(buy) = find(
            &screen,
            "arena/buy.png",
            Rect {
                x: 972,
                y: 0,
                width: 108,
                height: 120,
            },
        )?
        else {
            return Ok(false);
        };
        common::ensure_not_cancelled(self.cancelled)?;
        common::tap_match(self.serial, buy, size)?;
        common::settle_cancellable(self.cancelled)?;
        // Verify both the original exact zero-diamond template and the price
        // text twice. Unknown/nonzero offers are declined with Back, never a tap.
        for _ in 0..2 {
            common::ensure_not_cancelled(self.cancelled)?;
            let (screen, _) = common::screenshot(self.serial)?;
            if !verified_free_offer(&screen)? {
                adb::press_back(self.serial).map_err(|e| e.to_string())?;
                common::settle_cancellable(self.cancelled)?;
                return Ok(false);
            }
            common::settle_cancellable(self.cancelled)?;
        }
        let (screen, size) = common::screenshot(self.serial)?;
        if !verified_free_offer(&screen)? {
            adb::press_back(self.serial).map_err(|e| e.to_string())?;
            return Ok(false);
        }
        let Some((_, confirm)) = common::get_match(
            &screen,
            &["navigation/confirm.png", "confirm_text.png"],
            common::lower_screen(),
            0.85,
        )?
        else {
            adb::press_back(self.serial).map_err(|e| e.to_string())?;
            return Ok(false);
        };
        common::ensure_not_cancelled(self.cancelled)?;
        common::tap_match(self.serial, confirm, size)?;
        common::settle_cancellable(self.cancelled)?;
        wait_home(self.serial, Kind::Arena, self.cancelled)?;
        Ok(true)
    }
}

pub fn run(
    serial: &str,
    settings: Settings,
    cancelled: &AtomicBool,
    mut log: impl FnMut(String),
    mut progress: impl FnMut(ActivityEvent),
) -> Result<u32, String> {
    common::ensure_not_cancelled(cancelled)?;
    // Fail before any ticket can be used if local OCR support is unavailable.
    ocr::recognize(&GrayImage::new(128, 64))?;
    let kind = settings.kind();
    progress(ActivityEvent::ModeOpening(kind.mode()));
    adb::launch_afk_journey(serial).map_err(|e| e.to_string())?;
    for _ in 0..3 {
        common::settle_cancellable(cancelled)?;
    }
    navigate(serial, kind, cancelled, &mut log)?;
    progress(ActivityEvent::ModeEntered(kind.mode()));
    let mut cursor = super::recovery::arena_cursor(serial);
    let wins = attempts_from(
        &mut AdbDriver {
            serial,
            settings,
            cancelled,
            progress: &mut progress,
        },
        settings,
        &mut cursor,
        cancelled,
    )?;
    log(format_text!(
        "{} finished with {wins} confirmed wins.",
        "{} abgeschlossen: {wins} bestätigte Siege.",
        kind.mode().label()
    ));
    Ok(wins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_ratio_reads_the_available_count_without_guessing() {
        assert_eq!(attempt_count("0/1"), Some(0));
        assert_eq!(attempt_count("5 / 1"), Some(5));
        for unclear in ["O/1", "0/I", "0/2", "0", "5/1 0/1", ""] {
            assert_eq!(attempt_count(unclear), None);
        }
    }

    #[test]
    #[ignore = "opens the formation of an existing Arena attempt, without spending another ticket or starting a fight"]
    fn connected_emulator_arena_formation_without_combat() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        let mut log = |message| println!("{message}");
        navigate(&serial, Kind::Arena, &cancelled, &mut log).unwrap();
        let (home, _) = wait_home(&serial, Kind::Arena, &cancelled).unwrap();
        let (control, _) = home_control(&home, Kind::Arena).unwrap().unwrap();
        assert_eq!(control, "arena/continue.png", "open an Arena attempt first");
        let mut driver = AdbDriver {
            serial: &serial,
            settings: Settings::Arena(ArenaSettings::default()),
            cancelled: &cancelled,
            progress: &mut |_| panic!("the diagnostic must not start a fight"),
        };
        assert!(driver.choose().unwrap() == Choice::Ready);
        driver.wait_control("arena/battle.png", footer()).unwrap();
        std::fs::write(
            ".update-test/arena-formation.png",
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
        println!("Battle button reached using the pending Arena attempt.");
    }

    #[derive(Default)]
    struct Fake {
        battles: u32,
        claims: u32,
        available: u32,
        free: u32,
        failed: bool,
        choose_failed: bool,
        timed_out: bool,
    }
    impl Driver for Fake {
        fn choose(&mut self) -> Result<Choice, String> {
            if self.choose_failed {
                return Err("crashed during opponent selection".into());
            }
            Ok(if self.available > 0 {
                Choice::Ready
            } else {
                Choice::Exhausted
            })
        }
        fn battle(&mut self) -> Result<BattleOutcome, String> {
            if self.failed {
                return Err("unrecognized result".into());
            }
            self.available -= 1;
            self.battles += 1;
            if self.timed_out {
                return Err(super::super::BATTLE_RESULT_TIMEOUT.into());
            }
            Ok(BattleOutcome::Won)
        }
        fn free_attempt(&mut self) -> Result<bool, String> {
            self.claims += 1;
            if self.free == 0 {
                return Ok(false);
            }
            self.free -= 1;
            self.available += 1;
            Ok(true)
        }
    }

    #[test]
    fn timeout_spends_one_attempt_and_only_recovers_with_budget_remaining() {
        let settings = Settings::Supreme(SupremeSettings::default());
        let mut cursor = Cursor {
            initial: settings.attempts() - 2,
            ..Cursor::default()
        };
        let mut driver = Fake {
            available: 2,
            timed_out: true,
            ..Fake::default()
        };
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            attempts_from(&mut driver, settings, &mut cursor, &cancelled).unwrap_err(),
            super::super::BATTLE_RESULT_TIMEOUT
        );
        assert_eq!(cursor.initial, settings.attempts() - 1);
        assert_eq!(
            attempts_from(&mut driver, settings, &mut cursor, &cancelled).unwrap(),
            0
        );
        assert_eq!(cursor.initial, settings.attempts());
        assert_eq!(driver.battles, 2);
    }

    #[test]
    #[ignore = "opens and reads the Arena extra-attempt offer, then cancels without claiming or buying"]
    fn connected_emulator_arena_offer_read_only() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        navigate(&serial, Kind::Arena, &cancelled, &mut |m| println!("{m}")).unwrap();
        let (screen, size) = common::screenshot(&serial).unwrap();
        let buy = find(
            &screen,
            "arena/buy.png",
            Rect {
                x: 972,
                y: 0,
                width: 108,
                height: 120,
            },
        )
        .unwrap()
        .expect("extra attempt plus");
        common::tap_match(&serial, buy, size).unwrap();
        common::settle_cancellable(&cancelled).unwrap();
        let bytes = adb::screenshot(&serial).unwrap();
        let screen = common::normalize_screenshot(&bytes).unwrap().0;
        let lines = ocr::recognize(&screen);
        let template = common::find_template(&screen, "arena/buy_free.png", popup(), 0.95);
        let verified = verified_free_offer(&screen);
        // Close the offer before asserting or printing any diagnostic result.
        adb::press_back(&serial).unwrap();
        common::settle_cancellable(&cancelled).unwrap();
        std::fs::write(".update-test/arena-offer-read-only.png", bytes).unwrap();
        let lines = lines.unwrap();
        for line in &lines {
            println!("OCR: {line:?}");
        }
        println!(
            "Offer: zero-cost text={}, zero-price template={}",
            zero_cost(&lines),
            template.unwrap().is_some()
        );
        println!("Offer closed; no attempt claimed or battle started.");
        println!("Verified free offer: {}", verified.unwrap());
    }
    #[test]
    fn crash_resume_keeps_requested_battles_and_the_free_claim_phase() {
        let settings = Settings::Supreme(SupremeSettings::default());
        for fail_at_selection in [false, true] {
            let mut cursor = Cursor::default();
            let mut d = Fake {
                available: 5,
                failed: !fail_at_selection,
                choose_failed: fail_at_selection,
                ..Fake::default()
            };
            assert!(attempts_from(&mut d, settings, &mut cursor, &AtomicBool::new(false)).is_err());
            assert_eq!(cursor.initial, 1);
            d.failed = false;
            d.choose_failed = false;
            attempts_from(&mut d, settings, &mut cursor, &AtomicBool::new(false)).unwrap();
            assert_eq!(d.battles, 4);
            assert_eq!(cursor.initial, 5);
        }
        let mut cursor = Cursor {
            initial: 0,
            initial_done: true,
            claims: 1,
            pending_extra: true,
            ..Cursor::default()
        };
        let mut d = Fake {
            available: 1,
            ..Fake::default()
        };
        attempts_from(
            &mut d,
            Settings::Arena(ArenaSettings::default()),
            &mut cursor,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((d.battles, cursor.initial, cursor.extra), (1, 0, 1));
        assert_eq!(cursor.claims, 2);
    }
    #[test]
    fn arena_matches_original_five_attempts_and_at_most_two_free_claims() {
        let mut d = Fake {
            available: 20,
            free: 20,
            ..Fake::default()
        };
        assert_eq!(
            attempts(
                &mut d,
                Settings::Arena(ArenaSettings::default()),
                &AtomicBool::new(false)
            )
            .unwrap(),
            7
        );
        assert_eq!((d.battles, d.claims), (7, 2));
    }
    #[test]
    fn supreme_never_claims_extra_attempts_and_stops_at_actual_availability() {
        let mut d = Fake {
            available: 3,
            free: 20,
            ..Fake::default()
        };
        assert_eq!(
            attempts(
                &mut d,
                Settings::Supreme(SupremeSettings::default()),
                &AtomicBool::new(false)
            )
            .unwrap(),
            3
        );
        assert_eq!((d.battles, d.claims), (3, 0));
    }
    #[test]
    fn disabled_free_claims_and_paid_offers_cannot_add_more_battles() {
        let mut d = Fake {
            available: 5,
            free: 2,
            ..Fake::default()
        };
        attempts(
            &mut d,
            Settings::Arena(ArenaSettings {
                attempts: 2,
                claim_free_attempts: false,
            }),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((d.battles, d.claims), (2, 0));
        let mut d = Fake {
            available: 0,
            free: 0,
            ..Fake::default()
        };
        attempts(
            &mut d,
            Settings::Arena(ArenaSettings::default()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((d.battles, d.claims), (0, 1));
    }
    #[test]
    fn errors_and_stop_prevent_any_later_free_claim_or_battle() {
        let mut d = Fake {
            available: 5,
            free: 2,
            failed: true,
            ..Fake::default()
        };
        assert!(attempts(
            &mut d,
            Settings::Arena(ArenaSettings::default()),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!((d.battles, d.claims), (0, 0));
        assert!(attempts(
            &mut d,
            Settings::Arena(ArenaSettings::default()),
            &AtomicBool::new(true)
        )
        .is_err());
        assert_eq!((d.battles, d.claims), (0, 0));
    }
    fn line(text: &str, y: f32, height: f32) -> TextLine {
        TextLine {
            text: text.into(),
            x: 100.0,
            y,
            width: 500.0,
            height,
        }
    }
    #[test]
    fn free_price_requires_explicit_single_zero_not_twenty_or_unknown_text() {
        assert!(zero_cost(&[line("Spend @ 0", 800.0, 30.0)]));
        for text in ["Spend @20", "Spend 10", "Spend 0/2", "Spend O", "Confirm 0"] {
            assert!(!zero_cost(&[line(text, 800.0, 30.0)]));
        }
        assert!(!zero_cost(&[line("Spend 0", 100.0, 30.0)]));
    }
    #[test]
    fn explicit_result_headlines_decide_pvp_outcomes() {
        assert!(matches!(
            result_outcome(&[line("VICTORY", 200.0, 120.0)]),
            BattleOutcome::Won
        ));
        assert!(matches!(
            result_outcome(&[line("DEFEAT", 200.0, 120.0)]),
            BattleOutcome::Lost
        ));
        for lines in [
            vec![line("Victory", 900.0, 22.0)],
            vec![line("Tap to Close", 1800.0, 50.0)],
            vec![line("Victory", 200.0, 80.0), line("Defeat", 400.0, 80.0)],
        ] {
            assert!(matches!(result_outcome(&lines), BattleOutcome::Unclear));
        }
    }

    #[test]
    fn arena_victory_rewards_and_center_close_identify_the_reported_result() {
        let rewards = line("Victory Rewards", 1421.0, 37.0);
        let prompt = line("Tap to Close", 1753.0, 37.0);
        let lines = [rewards, prompt.clone()];
        let outcome = result_outcome(&lines);
        assert!(matches!(outcome, BattleOutcome::Won));
        assert_eq!(result_prompt(&lines, outcome), Some(prompt.center()));
        assert_eq!(
            result_prompt(&[prompt.clone()], BattleOutcome::Unclear),
            None
        );
        assert_eq!(
            result_prompt(
                &[line("Replay", 1818.0, 29.0), prompt],
                BattleOutcome::Unclear
            ),
            None
        );
        assert!(matches!(
            result_outcome(&[line("DEFEAT", 430.0, 120.0), lines[0].clone()]),
            BattleOutcome::Unclear
        ));
    }

    #[test]
    #[ignore = "verifies a supplied Arena result and closes an existing result without starting a battle"]
    fn connected_emulator_arena_existing_victory() {
        let path =
            std::env::var_os("ADBHELPER_TEST_ARENA_RESULT_IMAGE").expect("set a result capture");
        let (captured, _) = common::normalize_screenshot(&std::fs::read(path).unwrap()).unwrap();
        assert!(matches!(
            battle_result(&captured, false).unwrap(),
            Some((BattleOutcome::Won, _))
        ));

        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        let (screen, size) = common::screenshot(&serial).unwrap();
        let (outcome, close) = battle_result(&screen, false)
            .unwrap()
            .expect("open Arena result");
        assert!(matches!(outcome, BattleOutcome::Won));
        let mut events = Vec::new();
        let mut driver = AdbDriver {
            serial: &serial,
            settings: Settings::Arena(ArenaSettings::default()),
            cancelled: &cancelled,
            progress: &mut |event| events.push(event),
        };
        assert!(matches!(
            driver.complete_battle(outcome, close, size).unwrap(),
            BattleOutcome::Won
        ));
        assert!(matches!(
            events.as_slice(),
            [ActivityEvent::BattleFinished(
                GameMode::Arena,
                BattleOutcome::Won
            )]
        ));
        println!("One Arena victory recorded and Arena overview reached.");
    }
    #[test]
    fn supreme_opponent_positions_are_the_original_three_card_centres() {
        assert_eq!(
            OpponentPosition::ALL.map(OpponentPosition::point),
            [(165, 950), (540, 950), (915, 950)]
        );
    }
    #[test]
    #[ignore = "restarts the game once from the Arena overview; no PvP battle, opponent selection, or purchase"]
    fn connected_emulator_crash_recovery_without_combat() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let cancelled = AtomicBool::new(false);
        let mut calls = 0;
        let result = super::super::recovery::run(
            &serial,
            &cancelled,
            |checkpoint, report| {
                calls += 1;
                if calls > 1 {
                    assert_eq!(checkpoint.mode, Some(GameMode::Arena));
                }
                let package = adb::launch_afk_journey(&serial).map_err(|e| e.to_string())?;
                report(ActivityEvent::ModeOpening(GameMode::Arena));
                navigate(&serial, Kind::Arena, &cancelled, &mut |m| println!("{m}"))?;
                report(ActivityEvent::ModeEntered(GameMode::Arena));
                if calls == 1 {
                    assert!(adb::afk_journey_running(&serial).unwrap());
                    // Intentionally bypass intentional_stop ONLY in this ignored
                    // integration probe to simulate an actual process crash.
                    adb::shell(&serial, &["am", "force-stop", &package])
                        .map_err(|e| e.to_string())?;
                    for _ in 0..2 {
                        common::settle_cancellable(&cancelled)?;
                    }
                    common::screenshot(&serial)?;
                    panic!("crash was not detected");
                }
                Ok(0)
            },
            |m| println!("{m}"),
            |e| println!("{e:?}"),
        );
        assert_eq!(result.unwrap(), 0);
        assert_eq!(calls, 2);
        std::fs::write(
            ".update-test/pvp-after-crash-recovery.png",
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
    }
    #[test]
    #[ignore = "navigation only; does not choose opponents, claim attempts, or start battles"]
    fn connected_emulator_pvp_navigation() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set ADBHELPER_TEST_SERIAL");
        let kind = match std::env::var("ADBHELPER_TEST_PVP_MODE").as_deref() {
            Ok("arena") => Kind::Arena,
            Ok("supreme") => Kind::Supreme,
            _ => panic!("set ADBHELPER_TEST_PVP_MODE to arena or supreme"),
        };
        navigate(&serial, kind, &AtomicBool::new(false), &mut |m| {
            println!("{m}")
        })
        .unwrap();
        std::fs::write(
            format!(".update-test/pvp-{:?}-overview.png", kind),
            adb::screenshot(&serial).unwrap(),
        )
        .unwrap();
        println!(
            "{:?} overview reached without a battle or attempt claim",
            kind
        );
    }
}
