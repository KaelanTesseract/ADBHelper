//! Reads the game's "Hero Stats" list (the "i" beside a hero's power).

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
    rows_with(lines, |text, _| text.to_owned())
}

/// Like `parse_stat_rows`; `fix` may correct a value's text read on row `y`.
fn rows_with(lines: &[ocr::TextLine], fix: impl Fn(&str, u32) -> String) -> BTreeMap<String, f64> {
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
            .find_map(|line| stat_value(&fix(&line.text, y)));
        if let Some(value) = value {
            rows.entry(key).or_insert(value);
        }
    }
    rows
}

/// "Hero Stats" is open. The decorated title often reads as "Hero Stets",
/// so the section headers below it count as well.
fn stats_open(screen: &Screen) -> bool {
    screen
        .line(0..700, 820..1900, |text| {
            text.starts_with("herost") || SECTIONS[1..].contains(&text)
        })
        .is_some()
}

/// Value column, read enlarged: OCR over the whole screen drops the point
/// of small values ("14.0" becomes "140").
const VALUES: (u32, u32, u32, u32) = (VALUE_X, 960, 1080 - VALUE_X, 1920 - 960);

/// Rows of one screen: labels from the screen's OCR, values from the
/// enlarged value column.
fn screen_rows(screen: &Screen) -> Result<BTreeMap<String, f64>, ActionError> {
    let (x, y, width, height) = VALUES;
    let column = image::imageops::crop_imm(&screen.gray, x, y, width, height).to_image();
    let column = image::imageops::resize(
        &column,
        width * 2,
        height * 2,
        image::imageops::FilterType::CatmullRom,
    );
    let mut lines: Vec<ocr::TextLine> = screen
        .lines
        .iter()
        .filter(|line| {
            let (lx, ly) = line.center();
            lx < VALUE_X && ly > y && ly < LAST_ROW_Y
        })
        .cloned()
        .collect();
    // Enlarged values first; the screen's own values fill rows they missed.
    let screen_values: Vec<ocr::TextLine> = screen
        .lines
        .iter()
        .filter(|line| line.center().1 > y && line.center().0 >= VALUE_X)
        .cloned()
        .collect();
    lines.extend(
        ocr::recognize(&column)?
            .into_iter()
            .map(|line| ocr::TextLine {
                x: line.x / 2.0 + x as f32,
                y: line.y / 2.0 + y as f32,
                width: line.width / 2.0,
                height: line.height / 2.0,
                text: line.text,
            }),
    );
    lines.extend(screen_values);
    Ok(rows_with(&lines, |text, row| {
        with_point(text, digits_after_point(&screen.gray, row))
    }))
}

/// Rows below this are partly hidden by the back button ("…ation").
const LAST_ROW_Y: u32 = 1735;

/// Ink of the value digits on the light list background.
const INK: u8 = 190;

/// Digits after the decimal point of the value on row `row_y`, found in the
/// picture: the point is a small mark on the digits' baseline. `None` when
/// the value has no point.
fn digits_after_point(gray: &image::GrayImage, row_y: u32) -> Option<usize> {
    let top = row_y.saturating_sub(28);
    let bottom = (row_y + 28).min(gray.height());
    let right = 1070.min(gray.width());
    let mut seen = vec![false; ((right - VALUE_X) * (bottom - top)) as usize];
    let index = |x: u32, y: u32| ((y - top) * (right - VALUE_X) + (x - VALUE_X)) as usize;
    // Bounding boxes (left, top, right, bottom) of the dark marks.
    let mut marks: Vec<(u32, u32, u32, u32)> = Vec::new();
    for y in top..bottom {
        for x in VALUE_X..right {
            if seen[index(x, y)] || gray.get_pixel(x, y)[0] >= INK {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[index(x, y)] = true;
            let mut bounds = (x, y, x, y);
            let mut area = 0;
            while let Some((px, py)) = stack.pop() {
                area += 1;
                bounds = (
                    bounds.0.min(px),
                    bounds.1.min(py),
                    bounds.2.max(px),
                    bounds.3.max(py),
                );
                let neighbours = [
                    (px.wrapping_sub(1), py),
                    (px + 1, py),
                    (px, py.wrapping_sub(1)),
                    (px, py + 1),
                ];
                for (nx, ny) in neighbours {
                    if (VALUE_X..right).contains(&nx)
                        && (top..bottom).contains(&ny)
                        && !seen[index(nx, ny)]
                        && gray.get_pixel(nx, ny)[0] < INK
                    {
                        seen[index(nx, ny)] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if area >= 3 {
                marks.push(bounds);
            }
        }
    }
    let height = |mark: &(u32, u32, u32, u32)| mark.3 - mark.1 + 1;
    let digit_height = marks.iter().map(height).max()?;
    let baseline = marks
        .iter()
        .filter(|mark| height(mark) * 10 >= digit_height * 6)
        .map(|mark| mark.3)
        .max()?;
    let point = marks.iter().find(|mark| {
        height(mark) * 3 <= digit_height
            && (mark.2 - mark.0 + 1) * 3 <= digit_height
            && mark.3 + 4 >= baseline
    })?;
    let after = marks
        .iter()
        .filter(|mark| mark.0 > point.2 && height(mark) * 10 >= digit_height * 6)
        .count();
    (after > 0).then_some(after)
}

/// Puts the point back into a value whose point OCR dropped ("140" with one
/// digit after the point becomes "14.0").
fn with_point(text: &str, after: Option<usize>) -> String {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let has_suffix = compact.ends_with(['K', 'k', 'M', 'B']);
    let digits = compact.trim_end_matches('%');
    match after {
        Some(after)
            if !compact.contains('.')
                && !has_suffix
                && digits.len() > after
                && digits.chars().all(|c| c.is_ascii_digit()) =>
        {
            let (whole, fraction) = digits.split_at(digits.len() - after);
            format!("{whole}.{fraction}{}", &compact[digits.len()..])
        }
        _ => compact,
    }
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
        for (key, value) in screen_rows(&screen)? {
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
    #[ignore = "needs a private Hero Stats capture (ADBHELPER_TEST_HERO_DB)"]
    fn captured_hero_stats() {
        let folder = std::env::var_os("ADBHELPER_TEST_HERO_DB").expect("capture folder");
        let bytes = std::fs::read(std::path::Path::new(&folder).join("stats-1.png")).unwrap();
        let screen = Screen::decode(&bytes).unwrap();
        // The decorated title reads as "Hero Stets".
        assert!(stats_open(&screen));
        let values = screen_rows(&screen).unwrap();
        println!("{values:?}");
        // Whole-screen OCR drops the point of "14.0"; the enlarged value
        // column keeps it.
        assert_eq!(values.get("energyregenreduction"), Some(&14.0));
        assert_eq!(values.get("debufffocus"), Some(&6.0));
        assert_eq!(values.get("inspiration"), Some(&4.0));
        // Windows OCR does not read a lone "0.0"; the row is left out, never
        // guessed. A half-hidden label at the bottom is not taken either.
        assert_eq!(values.get("critdmgboost"), None);
        assert!(!values.contains_key("ation"));
    }

    fn number_image(marks: &[(u32, u32, u32, u32)]) -> image::GrayImage {
        let mut image = image::GrayImage::from_pixel(1080, 1920, image::Luma([235]));
        for &(x, y, width, height) in marks {
            for py in y..y + height {
                for px in x..x + width {
                    image.put_pixel(px, py, image::Luma([70]));
                }
            }
        }
        image
    }

    #[test]
    fn the_decimal_point_is_found_in_the_picture() {
        // "14.0": two digits, a small dot on the baseline, one digit.
        let point = number_image(&[
            (940, 1270, 10, 26),
            (954, 1270, 14, 26),
            (972, 1291, 5, 5),
            (980, 1270, 14, 26),
        ]);
        assert_eq!(digits_after_point(&point, 1283), Some(1));
        // "140": three digits, no dot.
        let whole = number_image(&[
            (940, 1270, 10, 26),
            (954, 1270, 14, 26),
            (972, 1270, 14, 26),
        ]);
        assert_eq!(digits_after_point(&whole, 1283), None);
        assert_eq!(with_point("140", Some(1)), "14.0");
        assert_eq!(with_point("14 0", Some(1)), "14.0");
        assert_eq!(with_point("14.0", Some(1)), "14.0");
        assert_eq!(with_point("2152M", Some(1)), "2152M");
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
