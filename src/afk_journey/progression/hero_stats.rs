//! Reads the game's "Hero Stats" list (the "i" beside a hero's power).
// The hero database scan that calls `read_stats` follows in a later step.
#![allow(dead_code)]

use super::hero_db::{today, Stats};
use super::{ocr, Driver, Screen};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::text_match::ascii_key;
use std::collections::BTreeMap;

/// The "i" beside the power on a hero's profile.
const STATS_BUTTON: (u32, u32) = (920, 1686);
/// The back arrow at the lower left of "Hero Stats".
const BACK: (u32, u32) = (115, 1795);
/// Labels are left of this x, values right of it.
const VALUE_X: u32 = 700;
/// Label and value of one row differ by at most this many pixels in y.
const SAME_ROW: u32 = 22;
const SECTIONS: [&str; 6] = [
    "herostats",
    "rivalrystats",
    "basicstats",
    "offensivestats",
    "defensivestats",
    "otherstats",
];

/// A stat value as shown: "2152M", "1.2B", "31,311K", "530.0%", "44.1".
pub fn stat_value(text: &str) -> Option<f64> {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let compact = compact.trim_end_matches('%');
    let (number, factor) = match compact.chars().last()? {
        'K' | 'k' => (&compact[..compact.len() - 1], 1e3),
        'M' => (&compact[..compact.len() - 1], 1e6),
        'B' => (&compact[..compact.len() - 1], 1e9),
        _ => (compact, 1.0),
    };
    let value: f64 = number.parse().ok()?;
    Some(tidy(value * factor, number))
}

/// Removes float noise ("1.2" × 1e9) while keeping the decimals shown.
fn tidy(value: f64, shown: &str) -> f64 {
    if value.abs() >= 1000.0 {
        return value.round();
    }
    let decimals = shown.split_once('.').map_or(1, |(_, d)| d.len().max(1));
    let scale = 10f64.powi(i32::try_from(decimals).unwrap_or(1));
    (value * scale).round() / scale
}

/// Pairs each label on the left with the value on its row. Rows without a
/// readable value are left out, never stored as 0.
pub fn parse_stat_rows(lines: &[ocr::TextLine]) -> BTreeMap<String, f64> {
    let mut rows = BTreeMap::new();
    for label in lines.iter().filter(|line| line.center().0 < VALUE_X) {
        let key = ascii_key(&label.text);
        if key.is_empty() || SECTIONS.contains(&key.as_str()) {
            continue;
        }
        let y = label.center().1;
        let value = lines
            .iter()
            .filter(|line| line.center().0 >= VALUE_X && line.center().1.abs_diff(y) <= SAME_ROW)
            .find_map(|line| stat_value(&line.text));
        if let Some(value) = value {
            rows.entry(key).or_insert(value);
        }
    }
    rows
}

fn stats_open(screen: &Screen) -> bool {
    screen
        .line(0..700, 820..1040, |text| text == "herostats")
        .is_some()
}

/// Opens "Hero Stats" from a hero's profile, reads every row while scrolling,
/// and returns to the profile. `None` when the list did not open.
pub(super) fn read_stats(
    driver: &Driver<'_>,
    profile: &Screen,
) -> Result<Option<Stats>, ActionError> {
    driver.tap(profile, STATS_BUTTON)?;
    let Some(mut screen) = driver.wait_for(stats_open)? else {
        return Ok(None);
    };
    let mut values = BTreeMap::new();
    for _ in 0..7 {
        let rows: Vec<ocr::TextLine> = screen
            .lines
            .iter()
            .filter(|line| line.center().1 > 960)
            .cloned()
            .collect();
        for (key, value) in parse_stat_rows(&rows) {
            values.entry(key).or_insert(value);
        }
        if values.contains_key("ultimatespower") {
            break;
        }
        crate::adb::swipe_game_points(driver.serial, (540, 1600), (540, 1000), screen.size, 600)
            .map_err(|error| error.to_string())?;
        crate::afk_journey::vision::settle_cancellable(driver.cancelled)?;
        screen = driver.read()?;
    }
    driver.tap(&screen, BACK)?;
    Ok((!values.is_empty()).then(|| Stats {
        values,
        scanned: today(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f32, y: f32) -> ocr::TextLine {
        ocr::TextLine {
            text: text.into(),
            x,
            y,
            width: 120.0,
            height: 30.0,
        }
    }

    #[test]
    fn stat_values_keep_their_size() {
        assert_eq!(stat_value("2152M"), Some(2_152_000_000.0));
        assert_eq!(stat_value("1.2B"), Some(1_200_000_000.0));
        assert_eq!(stat_value("31,311K"), Some(31_311_000.0));
        assert_eq!(stat_value("530.0%"), Some(530.0));
        assert_eq!(stat_value("44.1"), Some(44.1));
        assert_eq!(stat_value("1040"), Some(1040.0));
        assert_eq!(stat_value("--"), None);
    }

    #[test]
    #[ignore = "needs private Hero Stats captures (ADBHELPER_TEST_HERO_DB)"]
    fn captured_hero_stats() {
        let folder = std::env::var_os("ADBHELPER_TEST_HERO_DB").expect("capture folder");
        let mut values = BTreeMap::new();
        for index in 1..=4 {
            let path = std::path::Path::new(&folder).join(format!("stats-{index}.png"));
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let screen = Screen::decode(&bytes).unwrap();
            values.extend(parse_stat_rows(&screen.lines));
        }
        println!("{values:?}");
        for key in ["atk", "physdef", "crit", "proficiency"] {
            assert!(values.contains_key(key), "{key} missing");
        }
    }

    #[test]
    fn rows_pair_label_and_value_on_one_line() {
        let rows = parse_stat_rows(&[
            line("Basic Stats", 60.0, 960.0),
            line("ATK", 80.0, 1030.0),
            line("2152M", 900.0, 1032.0),
            line("Phys DEF", 80.0, 1100.0),
            line("Crit", 80.0, 1170.0),
            line("530.0%", 900.0, 1168.0),
        ]);
        assert_eq!(rows.get("atk"), Some(&2_152_000_000.0));
        assert_eq!(rows.get("crit"), Some(&530.0));
        assert!(
            !rows.contains_key("physdef"),
            "a label without a value is left out"
        );
        assert!(!rows.contains_key("basicstats"));
    }
}
