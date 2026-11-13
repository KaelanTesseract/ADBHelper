use super::{afk_stages as common, ocr, phase_rewards};
use crate::{
    activity::{ActivityEvent, GameMode},
    adb,
    localization::{format_text, text},
};
use image::{GrayImage, RgbImage};
use std::sync::atomic::AtomicBool;

type Point = (u32, u32);

const MENU_BUTTON: common::Rect = common::Rect {
    x: 885,
    y: 1760,
    width: 195,
    height: 160,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Menu,
    Friends,
    Mail,
    Pass,
    QuestHub,
    Daily,
    Guild,
}

impl Page {
    fn menu_entry(self) -> &'static str {
        match self {
            Self::Friends => "social",
            Self::Mail => "mail",
            Self::Pass => "noblepath",
            Self::QuestHub | Self::Daily | Self::Guild => "quests",
            Self::Menu => unreachable!(),
        }
    }
}

struct Frame {
    gray: GrayImage,
    color: RgbImage,
    lines: Vec<ocr::TextLine>,
    size: Point,
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

impl Frame {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let (gray, size) = common::normalize_screenshot(bytes)?;
        let color = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .resize_exact(
                gray.width(),
                gray.height(),
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();
        let lines = ocr::recognize(&gray)?;
        Ok(Self {
            gray,
            color,
            lines,
            size,
        })
    }

    fn line(
        &self,
        x: std::ops::Range<u32>,
        y: std::ops::Range<u32>,
        matches: impl Fn(&str) -> bool,
    ) -> Option<&ocr::TextLine> {
        self.lines.iter().find(|line| {
            let (px, py) = line.center();
            x.contains(&px) && y.contains(&py) && matches(&normalized(&line.text))
        })
    }

    fn menu_entry(&self, label: &str) -> Option<Point> {
        let (x, y) = match label {
            "social" => (35..220, 1560..1680),
            "mail" => (240..425, 1350..1460),
            "noblepath" => (640..850, 1350..1460),
            "quests" => (740..1030, 1100..1200),
            _ => return None,
        };
        self.line(x, y, |value| value == label)
            .map(ocr::TextLine::center)
    }

    fn main_menu_button(&self) -> Result<Option<Point>, String> {
        Ok(common::find_template(
            &self.gray,
            "navigation/hamburger_menu.png",
            MENU_BUTTON,
            0.9,
        )?
        .map(|button| button.center()))
    }

    fn page(&self) -> Option<Page> {
        let header = |name| self.line(25..510, 20..220, |value| value == name).is_some();
        let footer = |name| {
            self.line(210..1040, 1780..1905, |value| value.contains(name))
                .is_some()
        };
        if ["social", "mail", "noblepath", "quests"]
            .into_iter()
            .filter(|entry| self.menu_entry(entry).is_some())
            .count()
            >= 3
        {
            Some(Page::Menu)
        } else if header("dailyquests") {
            Some(Page::Daily)
        } else if self
            .line(50..470, 20..115, |value| {
                value.ends_with("questsi") || value.ends_with("quests")
            })
            .is_some()
            && self
                .line(350..780, 145..285, |value| value.contains("weeklylimit"))
                .is_some()
        {
            Some(Page::Guild)
        } else if footer("dailyquests") && footer("luckyflip") && footer("seasonquests") {
            Some(Page::QuestHub)
        } else if header("mail") {
            Some(Page::Mail)
        } else if footer("friends")
            && self
                .line(430..930, 1510..1700, |value| value == "sendreceive")
                .is_some()
        {
            Some(Page::Friends)
        } else if self
            .line(50..390, 115..215, |value| value.contains("endsin"))
            .is_some()
            && (footer("noblepath") || footer("fabledroad"))
        {
            Some(Page::Pass)
        } else {
            None
        }
    }

    fn claim_button(&self, page: Page) -> Option<Point> {
        let line = match page {
            Page::Friends => self.line(420..910, 1540..1710, |value| value == "sendreceive"),
            Page::Mail => self.line(510..1020, 1720..1870, |value| {
                matches!(value, "readall" | "reada11")
            }),
            Page::Pass => self.line(660..1030, 1550..1720, |value| value == "quickclaim"),
            Page::Daily | Page::Guild => {
                self.line(660..1030, 1440..1590, |value| value == "quickclaim")
            }
            Page::Menu | Page::QuestHub => None,
        }?;
        phase_rewards::green_control(&self.color, line).then(|| line.center())
    }

    fn milestone(&self) -> Option<Point> {
        // The point labels turn green at the threshold. Unclaimed items still
        // have a light corner; claimed ones get a dark green overlay.
        [
            (224, 199, 278, 191, 117),
            (372, 347, 278, 342, 117),
            (520, 499, 278, 487, 117),
            (668, 644, 278, 635, 117),
            (906, 854, 357, 847, 100),
        ]
        .into_iter()
        .find_map(|(x, chip_x, chip_y, item_x, item_y)| {
            let [r, g, b] = self.color.get_pixel(chip_x, chip_y).0;
            let [white_r, white_g, white_b] = self.color.get_pixel(item_x, item_y).0;
            (g > 135
                && g.saturating_sub(r) > 30
                && g.saturating_sub(b) > 25
                && white_r > 215
                && white_g > 210
                && white_b > 195)
                .then_some((x, 175))
        })
    }

    fn pass_reward_tab(&self) -> Result<Option<Point>, String> {
        let badge = common::find_template(
            &self.gray,
            "dailies/hamburger/rewards_available.png",
            common::Rect {
                x: 220,
                y: 1718,
                width: 860,
                height: 102,
            },
            0.83,
        )?;
        Ok(badge.and_then(|badge| {
            let (x, y) = badge.center();
            let [r, g, b] = self.color.get_pixel(x, y).0;
            (r > 220 && g > 110 && b < 190).then_some((x.saturating_sub(45), y + 36))
        }))
    }

    fn quest_complete(&self, labels: &[&str]) -> Option<bool> {
        if self.page() != Some(Page::Daily) {
            return None;
        }
        let title = self.line(240..850, 415..1440, |label| labels.contains(&label))?;
        let bottom = (title.y + title.height) as u32;
        // The progress bar stays visible after the reward is collected.
        let mut empty_bar = false;
        for y in bottom + 4..bottom + 32 {
            let mut green = 0;
            let mut gray = 0;
            for x in (260..989).step_by(24) {
                let [r, g, b] = self.color.get_pixel(x, y).0;
                green += usize::from(g.saturating_sub(r) >= 7 && g.saturating_sub(b) >= 4);
                gray += usize::from(r < 190 && b < 190 && r.saturating_sub(g) >= 5);
            }
            if green >= 29 {
                return Some(true);
            }
            empty_bar |= gray >= 29;
        }
        empty_bar.then_some(false)
    }

    fn receipt_close(&self) -> Option<Point> {
        phase_rewards::loot_close(&self.lines).or_else(|| {
            self.line(260..820, 195..630, |value| {
                matches!(
                    value,
                    "friendgifts" | "herecomegifts" | "successfullyclaimed"
                )
            })?;
            self.line(280..800, 1720..1870, |value| value == "taptoclose")
                .map(ocr::TextLine::center)
        })
    }
}

#[derive(Default)]
pub struct Cursor {
    collected: u32,
    submitted: bool,
}

struct Session<'a> {
    serial: &'a str,
    cancelled: &'a AtomicBool,
}

impl Session<'_> {
    fn read(&self) -> Result<Frame, String> {
        common::ensure_not_cancelled(self.cancelled)?;
        let frame =
            Frame::decode(&adb::screenshot(self.serial).map_err(|error| error.to_string())?)?;
        common::ensure_not_cancelled(self.cancelled)?;
        Ok(frame)
    }

    fn tap(&self, frame: &Frame, (x, y): Point) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        adb::tap_game_point(self.serial, x, y, frame.size).map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)
    }

    fn back(&self) -> Result<(), String> {
        common::ensure_not_cancelled(self.cancelled)?;
        adb::press_back(self.serial).map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)
    }

    fn open_main_menu(&self, log: &mut impl FnMut(String)) -> Result<(), String> {
        let size = common::navigate_to_current_overview(self.serial, self.cancelled, log)?;
        common::ensure_not_cancelled(self.cancelled)?;
        adb::tap_game_point(self.serial, 990, 1840, size).map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)
    }

    fn leave_reward_page(&self, frame: &Frame) -> Result<(), String> {
        let back = common::find_template(
            &frame.gray,
            "back.png",
            common::Rect {
                x: 0,
                y: 1710,
                width: 210,
                height: 190,
            },
            0.88,
        )?;
        match back {
            Some(back) => self.tap(frame, back.center()),
            None => self.back(),
        }
    }

    fn reset_pass_tabs(&self, frame: &Frame) -> Result<Frame, String> {
        common::ensure_not_cancelled(self.cancelled)?;
        adb::swipe_game_points(self.serial, (270, 1805), (1010, 1805), frame.size, 400)
            .map_err(|error| error.to_string())?;
        common::settle_cancellable(self.cancelled)?;
        self.wait_for(|frame| frame.page() == Some(Page::Pass))?
            .ok_or_else(|| {
                text(
                    "Could not verify the Noble Path tabs.",
                    "Die Noble-Path-Tabs konnten nicht eindeutig erkannt werden.",
                )
                .into()
            })
    }

    fn wait_for(&self, accept: impl Fn(&Frame) -> bool) -> Result<Option<Frame>, String> {
        for _ in 0..8 {
            let frame = self.read()?;
            if accept(&frame) {
                return Ok(Some(frame));
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Ok(None)
    }

    fn open(&self, target: Page, log: &mut impl FnMut(String)) -> Result<Frame, String> {
        let mut frame = self.read()?;
        if let Some(close) = frame.receipt_close() {
            self.tap(&frame, close)?;
            frame = self.read()?;
        }
        for _ in 0..8 {
            let page = frame.page();
            match page {
                Some(current) if current == target => return Ok(frame),
                Some(Page::QuestHub) if matches!(target, Page::Daily | Page::Guild) => {
                    self.tap(&frame, (275, 1840))?;
                }
                Some(Page::Daily | Page::Guild) if matches!(target, Page::Daily | Page::Guild) => {
                    let x = if target == Page::Daily { 245 } else { 860 };
                    self.tap(&frame, (x, 1665))?;
                    if let Some(next) = self.wait_for(|next| next.page() == Some(target))? {
                        return Ok(next);
                    }
                    break;
                }
                Some(Page::Menu) => {
                    if let Some(point) = frame.menu_entry(target.menu_entry()) {
                        self.tap(&frame, point)?;
                    } else {
                        break;
                    }
                }
                Some(_) => self.leave_reward_page(&frame)?,
                None => self.open_main_menu(log)?,
            }
            let Some(next) = self.wait_for_page_change(page)? else {
                break;
            };
            frame = next;
        }
        Err(text(
            "The reward screen could not be verified.",
            "Die Belohnungsansicht konnte nicht eindeutig erkannt werden.",
        )
        .into())
    }

    fn wait_for_page_change(&self, previous: Option<Page>) -> Result<Option<Frame>, String> {
        for _ in 0..8 {
            let frame = self.read()?;
            let page = frame.page();
            if page.is_some() && page != previous {
                return Ok(Some(frame));
            }
            if page.is_none() && matches!(previous, Some(Page::Pass | Page::Friends | Page::Mail)) {
                // The Back control can close directly onto the world.
                if let Some(menu) = frame.main_menu_button()? {
                    self.tap(&frame, menu)?;
                    return self.wait_for(|frame| frame.page() == Some(Page::Menu));
                }
            }
            common::settle_cancellable(self.cancelled)?;
        }
        Ok(None)
    }

    fn confirm(
        &self,
        page: Page,
        milestone: bool,
        report: &mut impl FnMut(),
    ) -> Result<Frame, String> {
        let mut missing = false;
        for _ in 0..8 {
            let frame = self.read()?;
            if let Some(close) = frame.receipt_close() {
                report();
                self.tap(&frame, close)?;
                return self
                    .wait_for(|frame| frame.page() == Some(page))?
                    .ok_or_else(|| {
                        text(
                            "Could not close the reward summary.",
                            "Die Belohnungsübersicht konnte nicht geschlossen werden.",
                        )
                        .into()
                    });
            }
            let gone = frame.page() == Some(page)
                && if milestone {
                    frame.milestone().is_none()
                } else {
                    frame.claim_button(page).is_none()
                };
            if gone && missing {
                if !matches!(page, Page::Friends | Page::Mail) {
                    report();
                }
                return Ok(frame);
            }
            missing = gone;
            common::settle_cancellable(self.cancelled)?;
        }
        Err(text(
            "The game did not confirm the reward claim.",
            "Das Spiel hat das Abholen der Belohnung nicht bestätigt.",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires Windows OCR and AFK Journey captures"]
    fn captured_menu_rewards() {
        let root =
            std::path::PathBuf::from(std::env::var_os("ADBHELPER_TEST_MENU_CAPTURES").unwrap());
        let load = |name| {
            Frame::decode(&std::fs::read(root.join(format!("{name}.png"))).unwrap()).unwrap()
        };
        assert_eq!(load("guild_after_pass_reported").page(), Some(Page::Pass));
        assert!(load("guild_after_pass_reported")
            .main_menu_button()
            .unwrap()
            .is_none());
        assert_eq!(load("guild_check_on_screen_back").page(), Some(Page::Menu));
        let world = load("guild_pass_second_back");
        assert_eq!(world.page(), None);
        assert!(world.main_menu_button().unwrap().is_some());
        assert_eq!(load("menu").page(), Some(Page::Menu));
        assert_eq!(load("live_daily_failure").page(), Some(Page::QuestHub));
        for (name, page) in [
            ("social", Page::Friends),
            ("mail", Page::Mail),
            ("pass", Page::Pass),
            ("daily", Page::Daily),
            ("guild", Page::Guild),
        ] {
            let frame = load(name);
            assert_eq!(frame.page(), Some(page), "{name}");
            assert!(frame.claim_button(page).is_some(), "{name}");
        }
        assert!(load("pass").pass_reward_tab().unwrap().is_some());
        assert_eq!(load("passswipe").page(), Some(Page::Pass));
        assert!(load("passother").claim_button(Page::Pass).is_none());
        assert!(load("daily").milestone().is_none());
        assert_eq!(load("dailyreceipt").milestone(), Some((224, 175)));
        assert!(load("dailyclaimed").milestone().is_none());
        assert!(load("friends_claimed")
            .claim_button(Page::Friends)
            .is_none());
        assert!(load("mail_claimed").claim_button(Page::Mail).is_none());
        for name in ["milestone20", "friends_after", "mail_after"] {
            let receipt = load(name);
            assert!(
                receipt.receipt_close().is_some(),
                "{name}: {:?}",
                receipt.lines
            );
        }
    }

    #[test]
    #[ignore = "verifies the transition from an open Battle Pass to Guild Quests without claiming"]
    fn connected_emulator_pass_to_guild_quests() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let cancelled = AtomicBool::new(false);
        let session = Session {
            serial: &serial,
            cancelled: &cancelled,
        };
        let log = &mut |message| println!("{message}");
        session.open(Page::Pass, log).unwrap();
        let frame = session.open(Page::Guild, log).unwrap();
        assert_eq!(frame.page(), Some(Page::Guild));
        println!("Guild Quests reached from the Battle Pass.");
    }

    #[test]
    #[ignore = "collects available free rewards on a connected AFK Journey account"]
    fn connected_emulator_menu_rewards() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let selected = std::env::var("ADBHELPER_TEST_MENU_MODE").unwrap_or_default();
        for (name, mode) in [
            ("friends", GameMode::FriendRewards),
            ("mail", GameMode::MailRewards),
            ("pass", GameMode::BattlePassRewards),
            ("daily", GameMode::DailyQuestRewards),
            ("guild", GameMode::GuildQuestRewards),
        ] {
            if !selected.is_empty() && !selected.split(',').any(|candidate| candidate == name) {
                continue;
            }
            let mut cursor = Cursor::default();
            let cancelled = AtomicBool::new(false);
            let result = super::super::recovery::run(
                &serial,
                &cancelled,
                |_, report| {
                    run(
                        &serial,
                        mode,
                        &mut cursor,
                        &cancelled,
                        &mut |log| println!("{log}"),
                        report,
                    )
                },
                |status| println!("{status}"),
                |event| println!("{event:?}"),
            );
            println!("{name}: {result:?}");
            assert!(result.is_ok(), "{name}: {result:?}");
        }
    }
}

const EMPORIUM_QUEST: &[&str] = &["purchasefromemporiumx1", "purchasefromemporiumxl"];

#[test]
#[ignore = "verifies open and completed Daily Quest progress from a supplied AFK Journey capture"]
fn captured_daily_quest_progress() {
    let folder = std::path::PathBuf::from(
        std::env::var_os("ADBHELPER_TEST_DAILY_MENU").expect("capture folder"),
    );
    let frame =
        Frame::decode(&std::fs::read(folder.join("skip_fix_quest_mid.png")).unwrap()).unwrap();
    assert_eq!(frame.quest_complete(EMPORIUM_QUEST), Some(false));
    assert_eq!(
        frame.quest_complete(&["participateindreamrealmxl", "participateindreamrealmx1"]),
        Some(true)
    );
    assert_eq!(frame.quest_complete(&["questnotonscreen"]), None);
}

pub(super) fn emporium_already_purchased(
    serial: &str,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
) -> Result<bool, String> {
    let session = Session { serial, cancelled };
    let mut frame = session.open(Page::Daily, log)?;
    // The quest list remembers its scroll position. Completed rows may be
    // below unfinished rows after their rewards have been collected.
    for (from, to) in [((540, 550), (540, 1460)), ((540, 1460), (540, 550))] {
        for _ in 0..5 {
            if let Some(done) = frame.quest_complete(EMPORIUM_QUEST) {
                if !done || session.read()?.quest_complete(EMPORIUM_QUEST) == Some(true) {
                    return Ok(done);
                }
            }
            if frame.page() != Some(Page::Daily) {
                return Ok(false);
            }
            common::ensure_not_cancelled(cancelled)?;
            adb::swipe_game_points(serial, from, to, frame.size, 380)
                .map_err(|error| error.to_string())?;
            common::settle_cancellable(cancelled)?;
            frame = session.read()?;
        }
    }
    Ok(false)
}

pub fn run(
    serial: &str,
    mode: GameMode,
    cursor: &mut Cursor,
    cancelled: &AtomicBool,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, String> {
    let page = match mode {
        GameMode::FriendRewards => Page::Friends,
        GameMode::MailRewards => Page::Mail,
        GameMode::BattlePassRewards => Page::Pass,
        GameMode::DailyQuestRewards => Page::Daily,
        GameMode::GuildQuestRewards => Page::Guild,
        _ => return Err("Invalid menu reward mode".into()),
    };
    let once = matches!(page, Page::Friends | Page::Mail);
    if once && cursor.submitted {
        return Ok(cursor.collected);
    }
    report(ActivityEvent::ModeOpening(mode));
    let session = Session { serial, cancelled };
    let mut frame = session.open(page, log)?;
    if page == Page::Pass {
        frame = session.reset_pass_tabs(&frame)?;
    }
    report(ActivityEvent::ModeEntered(mode));
    for _ in 0..15 {
        let button = frame.claim_button(page);
        let milestone = page == Page::Daily && button.is_none();
        let point = button.or_else(|| milestone.then(|| frame.milestone()).flatten());
        if let Some(point) = point {
            session.tap(&frame, point)?;
            frame = session.confirm(page, milestone, &mut || {
                cursor.collected += 1;
                cursor.submitted = once;
                report(ActivityEvent::Collected(mode, 1));
            })?;
            if once {
                return Ok(cursor.collected);
            }
        } else if page == Page::Pass {
            let Some(tab) = frame.pass_reward_tab()? else {
                return Ok(cursor.collected);
            };
            session.tap(&frame, tab)?;
            frame = session.wait_for(|next| next.page() == Some(page) && next.claim_button(page).is_some())?
                .ok_or_else(|| format_text!(
                    "{} shows an available reward, but its Quick Claim button could not be verified.",
                    "{} zeigt eine verfügbare Belohnung, aber Quick Claim konnte nicht eindeutig erkannt werden.",
                    mode.label()
                ))?;
        } else {
            return Ok(cursor.collected);
        }
    }
    Err(text(
        "The reward screen did not settle after several claims.",
        "Die Belohnungsansicht wurde nach mehreren Abholversuchen nicht stabil.",
    )
    .into())
}
