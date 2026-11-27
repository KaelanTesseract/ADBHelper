//! Reads a hero's skills in the game's "Full" view, with the numbers of
//! every skill level.

use super::hero_db::{Skill, SkillLevel};
use super::{ocr, Driver, Screen};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::text_match::ascii_key;

/// Tab buttons on the right of a hero's profile.
const SKILL_TAB: (u32, u32) = (1010, 760);
const PROFILE_TAB: (u32, u32) = (1010, 600);
/// The Full/Lite switch beside the skill numbers.
const TEXT_SWITCH: (u32, u32) = (950, 1322);
/// Skill icons from left (Ultimate) to right.
const ICONS: [u32; 6] = [194, 340, 480, 610, 744, 882];
const ICON_Y: u32 = 1080;
/// Name and level row, and the cooldown / range / energy row.
const NAME_ROW: std::ops::Range<u32> = 1210..1285;
const NUMBER_ROW: std::ops::Range<u32> = 1285..1345;
/// Lines right of this belong to the Animation and Full/Lite buttons.
const BUTTONS_X: u32 = 760;
const TEXT_ROWS: std::ops::Range<u32> = 1360..1730;

/// What the header above a skill text says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillHeader {
    pub name: String,
    pub level: Option<u32>,
    pub cooldown_s: Option<f64>,
    pub range: Option<String>,
    pub energy: Option<u32>,
}

/// The percentages in `text`.
fn percentages(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    let mut number = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '.' && !number.is_empty()) {
            number.push(c);
            continue;
        }
        if c == '%' {
            if let Ok(value) = number.trim_end_matches('.').parse() {
                values.push(value);
            }
        }
        number.clear();
    }
    values
}

/// Level 1 holds the percentages before the first "Level N:"; every
/// "Level N:" part holds the percentages it names.
pub fn skill_levels(text: &str) -> Vec<SkillLevel> {
    let mut parts = Vec::new();
    let mut rest = text;
    let mut level = 1;
    while let Some(start) = rest.find("Level ") {
        let after = &rest[start + "Level ".len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let is_upgrade = !digits.is_empty() && after[digits.len()..].trim_start().starts_with(':');
        if !is_upgrade {
            let skip = start + "Level ".len();
            parts.push((level, &rest[..skip]));
            rest = &rest[skip..];
            continue;
        }
        parts.push((level, &rest[..start]));
        level = digits.parse().unwrap_or(level + 1);
        rest = &after[digits.len()..];
    }
    parts.push((level, rest));
    let mut levels: Vec<SkillLevel> = Vec::new();
    for (level, part) in parts {
        let values = percentages(part);
        match levels.last_mut() {
            Some(last) if last.level == level => last.values.extend(values),
            _ => levels.push(SkillLevel { level, values }),
        }
    }
    levels.retain(|level| !level.values.is_empty());
    levels
}

fn in_rows<'a>(lines: &'a [ocr::TextLine], rows: &std::ops::Range<u32>) -> Vec<&'a ocr::TextLine> {
    let mut found: Vec<_> = lines
        .iter()
        .filter(|line| {
            let (x, y) = line.center();
            x < BUTTONS_X && rows.contains(&y)
        })
        .collect();
    found.sort_by(|a, b| a.x.total_cmp(&b.x));
    found
}

/// Name, level, cooldown, range and energy from the header lines.
pub fn skill_header(lines: &[ocr::TextLine]) -> Option<SkillHeader> {
    let title = in_rows(lines, &NAME_ROW)
        .iter()
        .map(|line| line.text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let (name, level) = match title.find("Level") {
        Some(at) => {
            let digits: String = title[at..].chars().filter(char::is_ascii_digit).collect();
            (title[..at].trim().to_owned(), digits.parse().ok())
        }
        None => (title.trim().to_owned(), None),
    };
    if name.is_empty() {
        return None;
    }
    let numbers = in_rows(lines, &NUMBER_ROW);
    let field = |index: usize| numbers.get(index).map(|line| line.text.trim().to_owned());
    Some(SkillHeader {
        name,
        level,
        cooldown_s: field(0).and_then(|text| text.trim_end_matches('s').trim().parse().ok()),
        range: field(1).filter(|text| !text.is_empty()),
        energy: field(2).and_then(|text| text.parse().ok()),
    })
}

fn slot(index: usize) -> String {
    match index {
        0 => "ult".into(),
        1 => "s1".into(),
        2 => "s2".into(),
        _ => format!("game{index}"),
    }
}

/// The skill text of the open skill, read while scrolling it.
fn skill_text(driver: &Driver<'_>, mut screen: Screen) -> Result<(String, Screen), ActionError> {
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..5 {
        let mut fresh = in_rows(&screen.lines, &TEXT_ROWS)
            .into_iter()
            .collect::<Vec<_>>();
        fresh.sort_by(|a, b| a.y.total_cmp(&b.y));
        let before = seen.len();
        for line in fresh {
            let text = line.text.trim().to_owned();
            if !text.is_empty() && !seen.contains(&text) {
                seen.push(text);
            }
        }
        if seen.len() == before {
            break;
        }
        crate::adb::swipe_game_points(driver.serial, (540, 1650), (540, 1420), screen.size, 500)
            .map_err(|error| error.to_string())?;
        crate::afk_journey::vision::settle_cancellable(driver.cancelled)?;
        screen = driver.read()?;
    }
    Ok((seen.join(" "), screen))
}

fn switch_reads(screen: &Screen, label: &str) -> bool {
    screen
        .line(BUTTONS_X..1080, NUMBER_ROW, |text| text == label)
        .is_some()
}

/// Opens the skill tab from a hero's profile, reads every skill in the
/// "Full" view, and returns to the profile.
pub(super) fn read_skills(
    driver: &Driver<'_>,
    profile: &Screen,
) -> Result<Vec<Skill>, ActionError> {
    driver.tap(profile, SKILL_TAB)?;
    let Some(mut screen) = driver.wait_for(|screen| skill_header(&screen.lines).is_some())? else {
        return Ok(Vec::new());
    };
    if switch_reads(&screen, "full") {
        driver.tap(&screen, TEXT_SWITCH)?;
        screen = driver.read()?;
    }
    let mut skills = Vec::new();
    for (index, x) in ICONS.into_iter().enumerate() {
        driver.tap(&screen, (x, ICON_Y))?;
        screen = driver.read()?;
        let Some(header) = skill_header(&screen.lines) else {
            continue;
        };
        if skills
            .iter()
            .any(|skill: &Skill| ascii_key(&skill.name) == ascii_key(&header.name))
        {
            continue;
        }
        let (text, after) = skill_text(driver, screen)?;
        screen = after;
        skills.push(Skill {
            slot: slot(index),
            name: header.name,
            level: header.level,
            cooldown_s: header.cooldown_s,
            range: header.range,
            energy: header.energy,
            unlock: None,
            levels: skill_levels(&text),
            text,
            source: "game".into(),
        });
    }
    driver.tap(&screen, PROFILE_TAB)?;
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f32, y: f32) -> ocr::TextLine {
        ocr::TextLine {
            text: text.into(),
            x,
            y,
            width: 60.0,
            height: 30.0,
        }
    }

    const ZOLTRAAK: &str = "Frieren moves to the rearmost position that maximizes the number \
        of enemies her Ultimate can hit. The Ultimate strikes enemies in a 3-tile rectangular \
        area for 3s, dealing 220% + 30% damage per second and 1400% + 180% true damage evenly \
        split among all enemies hit. Level 2: Increases damage per second to 240% + 30%, and \
        the extra true damage to 1500% + 180%.";

    #[test]
    fn level_one_and_upgrades_are_split() {
        assert_eq!(
            skill_levels(ZOLTRAAK),
            [
                SkillLevel {
                    level: 1,
                    values: vec![220.0, 30.0, 1400.0, 180.0]
                },
                SkillLevel {
                    level: 2,
                    values: vec![240.0, 30.0, 1500.0, 180.0]
                },
            ]
        );
    }

    #[test]
    fn header_reads_name_level_and_numbers() {
        let header = skill_header(&[
            line("Zoltraak", 50.0, 1245.0),
            line("Level: 5", 260.0, 1250.0),
            line("Animation", 860.0, 1225.0),
            line("0s", 100.0, 1305.0),
            line("Global", 270.0, 1305.0),
            line("350", 400.0, 1305.0),
            line("Lite", 900.0, 1305.0),
        ])
        .unwrap();
        assert_eq!(header.name, "Zoltraak");
        assert_eq!(header.level, Some(5));
        assert_eq!(header.cooldown_s, Some(0.0));
        assert_eq!(header.range.as_deref(), Some("Global"));
        assert_eq!(header.energy, Some(350));
    }

    #[test]
    #[ignore = "needs a private skill capture (ADBHELPER_TEST_HERO_DB)"]
    fn captured_hero_skill() {
        let folder = std::env::var_os("ADBHELPER_TEST_HERO_DB").expect("capture folder");
        let bytes = std::fs::read(std::path::Path::new(&folder).join("skill-full.png")).unwrap();
        let screen = Screen::decode(&bytes).unwrap();
        let header = skill_header(&screen.lines).expect("skill header");
        println!("{header:?}");
        let text: Vec<&str> = in_rows(&screen.lines, &TEXT_ROWS)
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        let levels = skill_levels(&text.join(" "));
        println!("{levels:?}");
        assert!(!levels.is_empty());
    }

    #[test]
    fn a_skill_without_energy_keeps_it_empty() {
        let header = skill_header(&[
            line("Soul Plunder Level: 4", 50.0, 1245.0),
            line("15s", 100.0, 1305.0),
            line("1", 270.0, 1305.0),
        ])
        .unwrap();
        assert_eq!(header.name, "Soul Plunder");
        assert_eq!(header.level, Some(4));
        assert_eq!(header.cooldown_s, Some(15.0));
        assert_eq!(header.range.as_deref(), Some("1"));
        assert_eq!(header.energy, None);
    }
}
