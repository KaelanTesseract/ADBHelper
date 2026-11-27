//! From a hero's profile: the stars under the EX weapon badge, and a picture
//! cut from the hero art for heroes without a portrait or hall card.

use image::{imageops, RgbImage};

/// Upper body of the hero art, below the name and left of the side icons,
/// in the shape of a hall card.
const PORTRAIT_AREA: (u32, u32, u32, u32) = (300, 230, 560, 800);
/// Size of a hall card, which the profile cut is scaled to.
pub const PORTRAIT_SIZE: (u32, u32) = (152, 228);

/// Area around the EX weapon badge at the lower left of the profile. The
/// stars sit on an arc under it; the outer ones reach up beside the badge.
const STAR_AREA_X: std::ops::Range<u32> = 20..260;
const STAR_AREA_Y: std::ops::Range<u32> = 1605..1710;
/// The colour mask splits a star into a few pieces; pieces whose centres
/// are this close belong to one star (neighbouring stars are 25+ px apart).
const SAME_STAR: f32 = 14.0;
/// A whole star is about 20–24 px wide and high; the light blue weapon orb
/// is much larger and is not a star.
const STAR_SIZE: std::ops::RangeInclusive<u32> = 12..=36;
const PIECE_AREA: usize = 20;
const STAR_AREA_MIN: usize = 60;

/// The light blue of the stars (measured about 180, 225, 250).
fn star_pixel(image::Rgb([r, g, b]): image::Rgb<u8>) -> bool {
    b > 220 && g > 200 && r < 230 && b.saturating_sub(r) > 15
}

/// Bounding box (left, top, right, bottom) and area of a piece or a star.
type Mark = (u32, u32, u32, u32, usize);

/// Connected pieces of star colour in the area around the badge.
fn pieces(screen: &RgbImage) -> Vec<Mark> {
    let right = STAR_AREA_X.end.min(screen.width());
    let bottom = STAR_AREA_Y.end.min(screen.height());
    let (left, top) = (STAR_AREA_X.start, STAR_AREA_Y.start);
    if right <= left || bottom <= top {
        return Vec::new();
    }
    let width = (right - left) as usize;
    let index = |x: u32, y: u32| (y - top) as usize * width + (x - left) as usize;
    let mut seen = vec![false; width * (bottom - top) as usize];
    let mut found = Vec::new();
    for y in top..bottom {
        for x in left..right {
            if seen[index(x, y)] || !star_pixel(*screen.get_pixel(x, y)) {
                continue;
            }
            seen[index(x, y)] = true;
            let mut stack = vec![(x, y)];
            let mut mark: Mark = (x, y, x, y, 0);
            while let Some((px, py)) = stack.pop() {
                mark = (
                    mark.0.min(px),
                    mark.1.min(py),
                    mark.2.max(px),
                    mark.3.max(py),
                    mark.4 + 1,
                );
                for (dx, dy) in [
                    (-1i32, 0i32),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                ] {
                    let (nx, ny) = (px as i32 + dx, py as i32 + dy);
                    if nx < left as i32
                        || ny < top as i32
                        || nx >= right as i32
                        || ny >= bottom as i32
                    {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    if !seen[index(nx, ny)] && star_pixel(*screen.get_pixel(nx, ny)) {
                        seen[index(nx, ny)] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            found.push(mark);
        }
    }
    found
}

fn centre(mark: &Mark) -> (f32, f32) {
    (
        (mark.0 + mark.2) as f32 / 2.0,
        (mark.1 + mark.3) as f32 / 2.0,
    )
}

/// Stars under the EX weapon badge on the profile; `None` without a badge.
pub fn count_ex_stars(screen: &RgbImage) -> Option<u32> {
    let too_big = |mark: &Mark| {
        mark.2 - mark.0 + 1 > *STAR_SIZE.end() || mark.3 - mark.1 + 1 > *STAR_SIZE.end()
    };
    let mut stars: Vec<Mark> = Vec::new();
    for piece in pieces(screen)
        .into_iter()
        .filter(|piece| piece.4 >= PIECE_AREA && !too_big(piece))
    {
        let (px, py) = centre(&piece);
        let near = stars.iter_mut().find(|star| {
            let (sx, sy) = centre(star);
            (sx - px).abs() <= SAME_STAR && (sy - py).abs() <= SAME_STAR
        });
        match near {
            Some(star) => {
                *star = (
                    star.0.min(piece.0),
                    star.1.min(piece.1),
                    star.2.max(piece.2),
                    star.3.max(piece.3),
                    star.4 + piece.4,
                );
            }
            None => stars.push(piece),
        }
    }
    let count = stars
        .iter()
        .filter(|star| {
            star.4 >= STAR_AREA_MIN
                && STAR_SIZE.contains(&(star.2 - star.0 + 1))
                && STAR_SIZE.contains(&(star.3 - star.1 + 1))
        })
        .count() as u32;
    (count > 0).then_some(count)
}

/// A picture cut from the hero art on the profile, for heroes whose hall
/// card is not known.
pub fn portrait(screen: &RgbImage) -> RgbImage {
    let (x, y, width, height) = PORTRAIT_AREA;
    let width = width.min(screen.width().saturating_sub(x)).max(1);
    let height = height.min(screen.height().saturating_sub(y)).max(1);
    let area = imageops::crop_imm(screen, x, y, width, height).to_image();
    imageops::resize(
        &area,
        PORTRAIT_SIZE.0,
        PORTRAIT_SIZE.1,
        imageops::FilterType::Triangle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band_with_stars(columns: &[u32]) -> RgbImage {
        let mut image = RgbImage::from_pixel(1080, 1920, image::Rgb([40, 90, 40]));
        for &left in columns {
            for x in left..left + 20 {
                for y in 1670..1690 {
                    image.put_pixel(x, y, image::Rgb([180, 225, 250]));
                }
            }
        }
        image
    }

    #[test]
    fn ex_weapon_stars_are_counted_as_separate_marks() {
        assert_eq!(count_ex_stars(&band_with_stars(&[85, 117])), Some(2));
        assert_eq!(count_ex_stars(&band_with_stars(&[69, 100, 132])), Some(3));
        assert_eq!(count_ex_stars(&band_with_stars(&[])), None);
    }

    #[test]
    fn a_thin_line_is_no_star() {
        let mut image = band_with_stars(&[85]);
        for y in 1670..1690 {
            image.put_pixel(150, y, image::Rgb([180, 225, 250]));
        }
        assert_eq!(count_ex_stars(&image), Some(1));
    }

    #[test]
    #[ignore = "needs private profile captures (ADBHELPER_TEST_HERO_DB)"]
    fn captured_profile_ex_stars() {
        let folder = std::env::var_os("ADBHELPER_TEST_HERO_DB").expect("capture folder");
        for (file, stars) in [
            ("profile-2-stars.png", 2),
            ("profile-3-stars.png", 3),
            ("profile-3-stars-orb.png", 3),
            ("profile-6-stars-rolan.png", 6),
            ("profile-5-stars.png", 5),
            ("profile-6-stars.png", 6),
        ] {
            let image = image::open(std::path::Path::new(&folder).join(file))
                .unwrap()
                .to_rgb8();
            assert_eq!(count_ex_stars(&image), Some(stars), "{file}");
        }
    }

    #[test]
    fn the_profile_cut_has_the_shape_of_a_card() {
        let picture = portrait(&RgbImage::from_pixel(1080, 1920, image::Rgb([1, 2, 3])));
        assert_eq!(picture.dimensions(), PORTRAIT_SIZE);
    }
}
