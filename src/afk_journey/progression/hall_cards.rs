//! The hero cards of the Resonating Hall, in the order the hero profiles
//! follow with "next": the five Hands of Resonance, then the Resonance
//! Heroes row by row. The whole card (frame, stars, level) is kept as the
//! hero's picture.

use super::{hall, Driver, Screen};
use crate::afk_journey::error::ActionError;
use image::{imageops, RgbImage};

/// Centres of the six card columns of the Resonance Heroes grid.
const COLUMNS: [u32; 6] = [134, 294, 458, 624, 790, 956];
const CARD_HALF_WIDTH: u32 = 76;
/// Centres of the five Hands of Resonance cards.
const HANDS: [u32; 5] = [134, 334, 536, 736, 936];
const HAND_HALF_WIDTH: u32 = 92;
/// Hands cards, measured from the "Hands of Resonance" title.
const HANDS_TOP: u32 = 32;
const HANDS_BOTTOM: u32 = 302;
/// The grid is visible down to the Artifacts / Equipment bar.
const GRID_BOTTOM: u32 = 1580;
/// A pixel row is a gap between card rows when this share of it is the
/// light background.
const GAP_SHARE: f32 = 0.6;
/// Card rows are about 230 px apart; anything much shorter is no full row.
const MIN_ROW: u32 = 180;
/// Mean brightness difference below which two cards are the same.
const SAME_CARD: f32 = 14.0;

fn background(pixel: &image::Rgb<u8>) -> bool {
    let [r, g, b] = pixel.0;
    r > 215 && g > 205 && b > 185
}

/// Centres of the light gaps between card rows from `top` to `bottom`.
pub fn row_gaps(screen: &RgbImage, top: u32, bottom: u32) -> Vec<u32> {
    let bottom = bottom.min(screen.height());
    let width = screen.width().min(1040);
    let mut gaps: Vec<(u32, u32)> = Vec::new();
    for y in top..bottom {
        let light = (40..width)
            .step_by(2)
            .filter(|&x| background(screen.get_pixel(x, y)))
            .count();
        let total = (40..width).step_by(2).count().max(1);
        if light as f32 / total as f32 > GAP_SHARE {
            match gaps.last_mut() {
                Some(gap) if y <= gap.1 + 2 => gap.1 = y,
                _ => gaps.push((y, y)),
            }
        }
    }
    gaps.into_iter()
        .map(|(start, end)| (start + end) / 2)
        .collect()
}

/// One card, or `None` when the slot is empty (end of the grid).
fn card(screen: &RgbImage, centre: u32, half: u32, top: u32, bottom: u32) -> Option<RgbImage> {
    let left = centre.saturating_sub(half);
    let crop = imageops::crop_imm(screen, left, top, half * 2, bottom - top).to_image();
    let light = crop.pixels().filter(|pixel| background(pixel)).count();
    (light * 10 < crop.pixels().len() * 6).then_some(crop)
}

/// Full card rows between gaps below `top`.
pub fn card_rows(screen: &RgbImage, top: u32) -> Vec<Vec<RgbImage>> {
    let gaps = row_gaps(screen, top, GRID_BOTTOM);
    gaps.windows(2)
        .filter(|pair| pair[1] - pair[0] >= MIN_ROW)
        .map(|pair| {
            COLUMNS
                .iter()
                .filter_map(|&centre| {
                    card(screen, centre, CARD_HALF_WIDTH, pair[0] + 3, pair[1] - 3)
                })
                .collect()
        })
        .filter(|row: &Vec<RgbImage>| !row.is_empty())
        .collect()
}

/// The five Hands of Resonance below their title at `title_y`.
pub fn hand_cards(screen: &RgbImage, title_y: u32) -> Vec<RgbImage> {
    let (top, bottom) = (title_y + HANDS_TOP, title_y + HANDS_BOTTOM);
    if bottom > screen.height() {
        return Vec::new();
    }
    HANDS
        .iter()
        .filter_map(|&centre| card(screen, centre, HAND_HALF_WIDTH, top, bottom))
        .collect()
}

fn mean_difference(a: &RgbImage, b: &RgbImage) -> f32 {
    let b = imageops::resize(b, a.width(), a.height(), imageops::FilterType::Triangle);
    let total: u64 = a
        .pixels()
        .zip(b.pixels())
        .map(|(p, q)| {
            p.0.iter()
                .zip(q.0.iter())
                .map(|(x, y)| u64::from(x.abs_diff(*y)))
                .sum::<u64>()
        })
        .sum();
    total as f32 / (a.pixels().len() * 3).max(1) as f32
}

/// The same row of cards, seen again after scrolling.
pub fn same_row(a: &[RgbImage], b: &[RgbImage]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| mean_difference(x, y) < SAME_CARD)
}

/// Adds the rows of a new screen that were not seen yet. Rows already taken
/// show up again at the top after a short scroll.
pub fn add_rows(taken: &mut Vec<Vec<RgbImage>>, rows: Vec<Vec<RgbImage>>) -> usize {
    let mut added = 0;
    for row in rows {
        let seen = taken.iter().rev().take(4).any(|old| same_row(old, &row));
        if !seen {
            taken.push(row);
            added += 1;
        }
    }
    added
}

fn title_y(screen: &Screen, key: &str) -> Option<u32> {
    screen
        .line(0..700, 0..1700, |text| text == key)
        .map(|line| line.center().1)
}

fn swipe(
    driver: &Driver<'_>,
    size: (u32, u32),
    from: u32,
    to: u32,
    ms: u32,
) -> Result<(), ActionError> {
    crate::adb::swipe_game_points(driver.serial, (540, from), (540, to), size, ms)
        .map_err(|error| error.to_string())?;
    crate::afk_journey::vision::settle_cancellable(driver.cancelled)
}

/// Every card of the Resonating Hall in profile order. Leaves the hall
/// scrolled to the top again, so the first hero can be opened as before.
pub(super) fn collect_cards(
    driver: &Driver<'_>,
    log: &mut impl FnMut(String),
) -> Result<Vec<RgbImage>, ActionError> {
    let mut screen = hall::open_hall(driver, log)?;
    for _ in 0..4 {
        swipe(driver, screen.size, 900, 1600, 250)?;
    }
    screen = driver.read()?;
    let mut hands = Vec::new();
    let mut rows: Vec<Vec<RgbImage>> = Vec::new();
    let mut quiet = 0;
    for _ in 0..80 {
        if hands.is_empty() {
            if let (Some(hands_y), Some(grid_y)) = (
                title_y(&screen, "handsofresonance"),
                title_y(&screen, "resonanceheroes"),
            ) {
                if grid_y > hands_y + HANDS_BOTTOM {
                    hands = hand_cards(&screen.color, hands_y);
                }
            }
        }
        let added = match title_y(&screen, "resonanceheroes") {
            Some(grid_y) => add_rows(&mut rows, card_rows(&screen.color, grid_y + 15)),
            None => 0,
        };
        // A short row is the end of the grid.
        if rows.last().is_some_and(|row| row.len() < COLUMNS.len()) {
            break;
        }
        quiet = if added == 0 { quiet + 1 } else { 0 };
        if quiet >= 3 {
            break;
        }
        swipe(driver, screen.size, 1450, 1150, 900)?;
        screen = driver.read()?;
    }
    for _ in 0..12 {
        swipe(driver, screen.size, 900, 1600, 250)?;
    }
    let mut cards = hands;
    cards.extend(rows.into_iter().flatten());
    Ok(cards)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(name: &str) -> RgbImage {
        let folder = std::env::var_os("ADBHELPER_TEST_HERO_DB").expect("capture folder");
        image::open(std::path::Path::new(&folder).join(name))
            .unwrap()
            .to_rgb8()
    }

    #[test]
    fn gaps_are_light_rows_across_the_grid() {
        let mut screen = RgbImage::from_pixel(1080, 1920, image::Rgb([60, 40, 30]));
        for y in [1000, 1230] {
            for x in 0..1080 {
                for dy in 0..6 {
                    screen.put_pixel(x, y + dy, image::Rgb([240, 232, 220]));
                }
            }
        }
        assert_eq!(row_gaps(&screen, 760, 1580), [1002, 1232]);
        let rows = card_rows(&screen, 760);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 6);
    }

    #[test]
    fn rows_seen_again_are_not_added_twice() {
        let row =
            |shade: u8| vec![RgbImage::from_pixel(10, 10, image::Rgb([shade, shade, shade])); 6];
        let mut taken = vec![row(10), row(80)];
        assert_eq!(add_rows(&mut taken, vec![row(80), row(150)]), 1);
        assert_eq!(taken.len(), 3);
    }

    #[test]
    #[ignore = "needs private Resonating Hall captures (ADBHELPER_TEST_HERO_DB)"]
    fn captured_hall_rows_follow_each_other() {
        let first = capture("hall-g0.png");
        let rows = card_rows(&first, 746);
        assert_eq!(rows.len(), 3, "three full rows under the title");
        assert!(rows.iter().all(|row| row.len() == 6));
        let hands = hand_cards(&first, 340);
        assert_eq!(hands.len(), 5);
        let mut taken = rows;
        let next = card_rows(&capture("hall-g1.png"), 746);
        let added = add_rows(&mut taken, next);
        assert!((1..=3).contains(&added), "added {added}");
        let end = card_rows(&capture("hall-g12.png"), 746);
        println!(
            "end rows: {:?}",
            end.iter().map(Vec::len).collect::<Vec<_>>()
        );
    }
}
