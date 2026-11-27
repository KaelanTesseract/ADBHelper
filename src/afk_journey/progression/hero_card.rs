//! A picture cut from a hero's profile, used when the hero's Resonating Hall
//! card is not known.

use image::{imageops, RgbImage};

/// Upper body of the hero art, below the name and left of the side icons,
/// in the shape of a hall card.
const PORTRAIT_AREA: (u32, u32, u32, u32) = (300, 230, 560, 800);
/// Size of a hall card, which the profile cut is scaled to.
pub const PORTRAIT_SIZE: (u32, u32) = (152, 228);

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

    #[test]
    fn the_profile_cut_has_the_shape_of_a_card() {
        let picture = portrait(&RgbImage::from_pixel(1080, 1920, image::Rgb([1, 2, 3])));
        assert_eq!(picture.dimensions(), PORTRAIT_SIZE);
    }
}
