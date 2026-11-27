//! From a hero's profile: the stars under the EX weapon badge, and a picture
//! cut from the hero art for heroes without a portrait or hall card.

use image::{imageops, RgbImage};

/// Upper body of the hero art, below the name and left of the side icons,
/// in the shape of a hall card.
const PORTRAIT_AREA: (u32, u32, u32, u32) = (300, 230, 560, 800);
/// Size of a hall card, which the profile cut is scaled to.
pub const PORTRAIT_SIZE: (u32, u32) = (152, 228);

/// Band under the EX weapon badge at the lower left of the profile.
const STAR_BAND_X: std::ops::Range<u32> = 40..220;
const STAR_BAND_Y: std::ops::Range<u32> = 1650..1705;
/// A star is at least this many pixel columns wide (measured 21–22).
const STAR_WIDTH: u32 = 12;

/// The light blue of the stars (measured about 180, 225, 250).
fn star_pixel(image::Rgb([r, g, b]): image::Rgb<u8>) -> bool {
    b > 220 && g > 200 && r < 230 && b.saturating_sub(r) > 15
}

/// Stars under the EX weapon badge on the profile; `None` without a badge.
pub fn count_ex_stars(screen: &RgbImage) -> Option<u32> {
    let mut stars = 0;
    let mut run = 0;
    for x in STAR_BAND_X.chain(std::iter::once(STAR_BAND_X.end)) {
        let lit = x < STAR_BAND_X.end
            && x < screen.width()
            && STAR_BAND_Y
                .clone()
                .filter(|&y| y < screen.height())
                .any(|y| star_pixel(*screen.get_pixel(x, y)));
        if lit {
            run += 1;
        } else {
            if run >= STAR_WIDTH {
                stars += 1;
            }
            run = 0;
        }
    }
    (stars > 0).then_some(stars)
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
        for (file, stars) in [("profile-2-stars.png", 2), ("profile-3-stars.png", 3)] {
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
