//! The Hero Box picture: every scanned hero with portrait, S+ frame and
//! stars, for pasting into Discord.

use image::{imageops, Rgba, RgbaImage};

pub const COLUMNS: u32 = 8;
pub const TILE: u32 = 128;
/// Hall cards are half again as tall as wide.
pub const TILE_HEIGHT: u32 = 192;
pub const GAP: u32 = 8;
pub const MARGIN: u32 = 16;
const FRAME: u32 = 4;
const BACKGROUND: Rgba<u8> = Rgba([24, 26, 30, 255]);
const PLACEHOLDER: Rgba<u8> = Rgba([58, 62, 70, 255]);
pub const GOLD: Rgba<u8> = Rgba([212, 175, 55, 255]);
const BADGE: Rgba<u8> = Rgba([20, 20, 20, 255]);

/// One hero on the picture.
pub struct BoxHero {
    pub portrait: Option<RgbaImage>,
    pub supreme_plus: bool,
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    for py in y..(y + height).min(image.height()) {
        for px in x..(x + width).min(image.width()) {
            image.put_pixel(px, py, color);
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

/// Every hero in rows of `COLUMNS`: the hall card (or a grey tile) and a
/// gold frame for S+. The card itself shows the stars.
pub fn compose(heroes: &[BoxHero]) -> RgbaImage {
    let rows = (heroes.len() as u32).div_ceil(COLUMNS).max(1);
    let columns = (heroes.len() as u32).clamp(1, COLUMNS);
    let width = MARGIN * 2 + columns * TILE + (columns - 1) * GAP;
    let height = MARGIN * 2 + rows * TILE_HEIGHT + (rows - 1) * GAP;
    let mut image = RgbaImage::from_pixel(width, height, BACKGROUND);
    for (index, hero) in heroes.iter().enumerate() {
        let index = index as u32;
        let x = MARGIN + (index % COLUMNS) * (TILE + GAP);
        let y = MARGIN + (index / COLUMNS) * (TILE_HEIGHT + GAP);
        match &hero.portrait {
            Some(portrait) => {
                let tile =
                    imageops::resize(portrait, TILE, TILE_HEIGHT, imageops::FilterType::Triangle);
                imageops::replace(&mut image, &tile, i64::from(x), i64::from(y));
            }
            None => fill(&mut image, x, y, TILE, TILE_HEIGHT, PLACEHOLDER),
        }
        if hero.supreme_plus {
            fill(&mut image, x, y, TILE, FRAME, GOLD);
            fill(&mut image, x, y + TILE_HEIGHT - FRAME, TILE, FRAME, GOLD);
            fill(&mut image, x, y, FRAME, TILE_HEIGHT, GOLD);
            fill(&mut image, x + TILE - FRAME, y, FRAME, TILE_HEIGHT, GOLD);
            s_plus(&mut image, x + FRAME + 2, y + FRAME + 2, 2);
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

    fn hero(supreme_plus: bool) -> BoxHero {
        BoxHero {
            portrait: Some(RgbaImage::from_pixel(152, 228, Rgba([10, 120, 200, 255]))),
            supreme_plus,
        }
    }

    #[test]
    fn compose_places_heroes_in_rows() {
        let heroes: Vec<BoxHero> = (0..9).map(|_| hero(false)).collect();
        let image = compose(&heroes);
        assert_eq!(
            image.width(),
            MARGIN * 2 + COLUMNS * TILE + (COLUMNS - 1) * GAP
        );
        assert_eq!(image.height(), MARGIN * 2 + 2 * TILE_HEIGHT + GAP);
    }

    #[test]
    fn a_short_row_is_only_as_wide_as_its_heroes() {
        let heroes: Vec<BoxHero> = (0..3).map(|_| hero(false)).collect();
        assert_eq!(compose(&heroes).width(), MARGIN * 2 + 3 * TILE + 2 * GAP);
    }

    #[test]
    fn supreme_plus_gets_a_gold_frame() {
        let image = compose(&[hero(true), hero(false)]);
        assert_eq!(*image.get_pixel(MARGIN + 1, MARGIN + TILE_HEIGHT / 2), GOLD);
        let second = MARGIN + TILE + GAP;
        assert_ne!(*image.get_pixel(second + 1, MARGIN + TILE_HEIGHT / 2), GOLD);
    }

    #[test]
    fn the_card_fills_its_tile() {
        let image = compose(&[hero(false)]);
        assert_eq!(
            *image.get_pixel(MARGIN + TILE / 2, MARGIN + TILE_HEIGHT - 2),
            Rgba([10, 120, 200, 255])
        );
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
