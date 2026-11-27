//! The Hero Box picture: every scanned hero with portrait, S+ frame and
//! stars, for pasting into Discord.

use image::{imageops, Rgba, RgbaImage};

pub const COLUMNS: u32 = 8;
pub const TILE: u32 = 128;
pub const STAR_ROW: u32 = 24;
pub const GAP: u32 = 8;
pub const MARGIN: u32 = 16;
const FRAME: u32 = 4;
const BACKGROUND: Rgba<u8> = Rgba([24, 26, 30, 255]);
const PLACEHOLDER: Rgba<u8> = Rgba([58, 62, 70, 255]);
pub const GOLD: Rgba<u8> = Rgba([212, 175, 55, 255]);
pub const STAR: Rgba<u8> = Rgba([255, 214, 90, 255]);
const BADGE: Rgba<u8> = Rgba([20, 20, 20, 255]);

/// One hero on the picture.
pub struct BoxHero {
    pub portrait: Option<RgbaImage>,
    pub supreme_plus: bool,
    pub stars: Option<u32>,
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    for py in y..(y + height).min(image.height()) {
        for px in x..(x + width).min(image.width()) {
            image.put_pixel(px, py, color);
        }
    }
}

/// A filled five-pointed star of `size` pixels with its top left at (x, y).
fn star(image: &mut RgbaImage, x: u32, y: u32, size: u32) {
    let center = (size as f32 / 2.0, size as f32 / 2.0);
    let outer = size as f32 / 2.0;
    let inner = outer * 0.45;
    let corners: Vec<(f32, f32)> = (0..10)
        .map(|index| {
            let radius = if index % 2 == 0 { outer } else { inner };
            let angle = std::f32::consts::PI * (index as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect();
    for py in 0..size {
        for px in 0..size {
            let point = (px as f32 + 0.5, py as f32 + 0.5);
            let mut inside = false;
            for index in 0..corners.len() {
                let a = corners[index];
                let b = corners[(index + 1) % corners.len()];
                if (a.1 > point.1) != (b.1 > point.1)
                    && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
                {
                    inside = !inside;
                }
            }
            if inside && x + px < image.width() && y + py < image.height() {
                image.put_pixel(x + px, y + py, STAR);
            }
        }
    }
}

/// "S+" in a 5×7 pixel font, each dot `scale` pixels.
fn s_plus(image: &mut RgbaImage, x: u32, y: u32, scale: u32) {
    const S: [&str; 7] = [
        ".###.", "#...#", "#....", ".###.", "....#", "#...#", ".###.",
    ];
    const PLUS: [&str; 7] = [
        ".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....",
    ];
    fill(image, x, y, scale * 13, scale * 9, BADGE);
    for (offset, glyph) in [(1, S), (7, PLUS)] {
        for (row, line) in glyph.iter().enumerate() {
            for (column, dot) in line.chars().enumerate() {
                if dot == '#' {
                    fill(
                        image,
                        x + (offset + column as u32) * scale,
                        y + (1 + row as u32) * scale,
                        scale,
                        scale,
                        GOLD,
                    );
                }
            }
        }
    }
}

/// Every hero in rows of `COLUMNS`: portrait (or a grey tile), a gold frame
/// for S+, and the stars below.
pub fn compose(heroes: &[BoxHero]) -> RgbaImage {
    let rows = (heroes.len() as u32).div_ceil(COLUMNS).max(1);
    let columns = (heroes.len() as u32).clamp(1, COLUMNS);
    let width = MARGIN * 2 + columns * TILE + (columns - 1) * GAP;
    let height = MARGIN * 2 + rows * (TILE + STAR_ROW) + (rows - 1) * GAP;
    let mut image = RgbaImage::from_pixel(width, height, BACKGROUND);
    for (index, hero) in heroes.iter().enumerate() {
        let index = index as u32;
        let x = MARGIN + (index % COLUMNS) * (TILE + GAP);
        let y = MARGIN + (index / COLUMNS) * (TILE + STAR_ROW + GAP);
        match &hero.portrait {
            Some(portrait) => {
                let tile = imageops::resize(portrait, TILE, TILE, imageops::FilterType::Triangle);
                imageops::replace(&mut image, &tile, i64::from(x), i64::from(y));
            }
            None => fill(&mut image, x, y, TILE, TILE, PLACEHOLDER),
        }
        if hero.supreme_plus {
            fill(&mut image, x, y, TILE, FRAME, GOLD);
            fill(&mut image, x, y + TILE - FRAME, TILE, FRAME, GOLD);
            fill(&mut image, x, y, FRAME, TILE, GOLD);
            fill(&mut image, x + TILE - FRAME, y, FRAME, TILE, GOLD);
            s_plus(&mut image, x + FRAME + 2, y + FRAME + 2, 2);
        }
        if let Some(stars) = hero.stars.filter(|&stars| stars > 0) {
            let size = STAR_ROW - 4;
            let stars = stars.min(TILE / size);
            let start = x + (TILE - stars * size) / 2;
            for number in 0..stars {
                star(&mut image, start + number * size, y + TILE + 2, size);
            }
        }
    }
    image
}

/// The picture as a Windows device-independent bitmap (CF_DIB): a 40-byte
/// BITMAPINFOHEADER followed by BGRA rows from the bottom up.
pub fn dib(image: &RgbaImage) -> Vec<u8> {
    let (width, height) = image.dimensions();
    let mut bytes = Vec::with_capacity(40 + (width * height * 4) as usize);
    bytes.extend(40u32.to_le_bytes());
    bytes.extend((width as i32).to_le_bytes());
    bytes.extend((height as i32).to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(32u16.to_le_bytes());
    bytes.extend(0u32.to_le_bytes()); // BI_RGB
    bytes.extend((width * height * 4).to_le_bytes());
    bytes.extend([0u8; 16]);
    for y in (0..height).rev() {
        for x in 0..width {
            let [r, g, b, a] = image.get_pixel(x, y).0;
            bytes.extend([b, g, r, a]);
        }
    }
    bytes
}

/// Puts the picture on the clipboard so it can be pasted into Discord.
#[cfg(windows)]
pub fn copy_to_clipboard(image: &RgbaImage) -> Result<(), String> {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_DIB;
    let bytes = dib(image);
    // SAFETY: plain Win32 clipboard calls; the global memory is handed to
    // the clipboard on success and freed here on failure.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if memory.is_null() {
            return Err("GlobalAlloc failed".into());
        }
        let target = GlobalLock(memory).cast::<u8>();
        if target.is_null() {
            GlobalFree(memory);
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        GlobalUnlock(memory);
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            GlobalFree(memory);
            return Err("the clipboard is in use".into());
        }
        EmptyClipboard();
        let placed = !SetClipboardData(u32::from(CF_DIB), memory).is_null();
        CloseClipboard();
        if !placed {
            GlobalFree(memory);
            return Err("SetClipboardData failed".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn copy_to_clipboard(_: &RgbaImage) -> Result<(), String> {
    Err("unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(supreme_plus: bool, stars: Option<u32>) -> BoxHero {
        BoxHero {
            portrait: Some(RgbaImage::from_pixel(64, 64, Rgba([10, 120, 200, 255]))),
            supreme_plus,
            stars,
        }
    }

    #[test]
    fn compose_places_heroes_in_rows() {
        let heroes: Vec<BoxHero> = (0..9).map(|_| hero(false, Some(3))).collect();
        let image = compose(&heroes);
        assert_eq!(
            image.width(),
            MARGIN * 2 + COLUMNS * TILE + (COLUMNS - 1) * GAP
        );
        assert_eq!(image.height(), MARGIN * 2 + 2 * (TILE + STAR_ROW) + GAP);
    }

    #[test]
    fn a_short_row_is_only_as_wide_as_its_heroes() {
        let heroes: Vec<BoxHero> = (0..3).map(|_| hero(false, None)).collect();
        assert_eq!(compose(&heroes).width(), MARGIN * 2 + 3 * TILE + 2 * GAP);
    }

    #[test]
    fn supreme_plus_gets_a_gold_frame() {
        let image = compose(&[hero(true, None), hero(false, None)]);
        assert_eq!(*image.get_pixel(MARGIN + 1, MARGIN + TILE / 2), GOLD);
        let second = MARGIN + TILE + GAP;
        assert_ne!(*image.get_pixel(second + 1, MARGIN + TILE / 2), GOLD);
    }

    #[test]
    fn stars_are_drawn_below_the_portrait() {
        let none = compose(&[hero(false, None)]);
        let five = compose(&[hero(false, Some(5))]);
        let row = |image: &RgbaImage| {
            (MARGIN..MARGIN + TILE)
                .flat_map(|x| (MARGIN + TILE..MARGIN + TILE + STAR_ROW).map(move |y| (x, y)))
                .filter(|&(x, y)| *image.get_pixel(x, y) == STAR)
                .count()
        };
        assert_eq!(row(&none), 0);
        assert!(row(&five) > 50);
    }

    #[test]
    fn dib_is_bottom_up_bgra() {
        let mut image = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        image.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
        let bytes = dib(&image);
        assert_eq!(bytes.len(), 40 + 2 * 2 * 4);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[14..16].try_into().unwrap()), 32);
        assert_eq!(&bytes[40..44], &[255, 0, 0, 255], "bottom-left pixel, BGRA");
    }
}
