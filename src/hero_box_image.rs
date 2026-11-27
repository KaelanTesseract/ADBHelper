//! The Hero Box picture for Discord: every scanned hero with portrait in a
//! uniform frame, and below it ascension | name | EX weapon level.

use crate::text_raster::{draw_sword, draw_text, text_width};
use image::{imageops, Rgba, RgbaImage};

pub const COLUMNS: u32 = 8;
pub const TILE: u32 = 128;
/// Portraits are about 1.4 times as tall as wide (180×248).
pub const TILE_HEIGHT: u32 = 176;
/// The line with ascension, name and EX weapon level under each portrait.
pub const LABEL: u32 = 24;
pub const GAP: u32 = 10;
pub const MARGIN: u32 = 16;
const FRAME: u32 = 2;
const BACKGROUND: Rgba<u8> = Rgba([24, 26, 30, 255]);
const PLACEHOLDER: Rgba<u8> = Rgba([58, 62, 70, 255]);
pub const FRAME_COLOR: Rgba<u8> = Rgba([92, 98, 112, 255]);
const GOLD: Rgba<u8> = Rgba([212, 175, 55, 255]);
const LIGHT: Rgba<u8> = Rgba([225, 225, 225, 255]);
const BLADE: Rgba<u8> = Rgba([215, 222, 235, 255]);
const GRIP: Rgba<u8> = Rgba([196, 140, 70, 255]);
const TEXT_SIZE: f32 = 14.0;

/// One hero on the picture.
pub struct BoxHero {
    pub name: String,
    pub portrait: Option<RgbaImage>,
    /// Supreme+ or higher; the ascension is written in gold then.
    pub supreme_plus: bool,
    /// Ascension such as "S+", "P3", "M+".
    pub badge: Option<&'static str>,
    /// EX weapon level ("+25").
    pub ex_level: Option<u32>,
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    for py in y..(y + height).min(image.height()) {
        for px in x..(x + width).min(image.width()) {
            image.put_pixel(px, py, color);
        }
    }
}

/// `name` shortened with "…" until it fits `room` pixels.
fn fitted(name: &str, room: f32) -> String {
    if text_width(name, TEXT_SIZE) <= room {
        return name.to_owned();
    }
    let mut text: String = name.to_owned();
    while !text.is_empty() {
        text.pop();
        let candidate = format!("{}…", text.trim_end());
        if text_width(&candidate, TEXT_SIZE) <= room {
            return candidate;
        }
    }
    String::new()
}

/// Every hero in rows of `COLUMNS`: portrait (or a grey tile) in a uniform
/// frame, then ascension, name and EX weapon level on one line.
pub fn compose(heroes: &[BoxHero]) -> RgbaImage {
    let rows = (heroes.len() as u32).div_ceil(COLUMNS).max(1);
    let columns = (heroes.len() as u32).clamp(1, COLUMNS);
    let width = MARGIN * 2 + columns * TILE + (columns - 1) * GAP;
    let height = MARGIN * 2 + rows * (TILE_HEIGHT + LABEL) + (rows - 1) * GAP;
    let mut image = RgbaImage::from_pixel(width, height, BACKGROUND);
    for (index, hero) in heroes.iter().enumerate() {
        let index = index as u32;
        let x = MARGIN + (index % COLUMNS) * (TILE + GAP);
        let y = MARGIN + (index / COLUMNS) * (TILE_HEIGHT + LABEL + GAP);
        fill(&mut image, x, y, TILE, TILE_HEIGHT, PLACEHOLDER);
        if let Some(portrait) = &hero.portrait {
            let tile =
                imageops::resize(portrait, TILE, TILE_HEIGHT, imageops::FilterType::Triangle);
            imageops::overlay(&mut image, &tile, i64::from(x), i64::from(y));
        }
        fill(&mut image, x, y, TILE, FRAME, FRAME_COLOR);
        fill(
            &mut image,
            x,
            y + TILE_HEIGHT - FRAME,
            TILE,
            FRAME,
            FRAME_COLOR,
        );
        fill(&mut image, x, y, FRAME, TILE_HEIGHT, FRAME_COLOR);
        fill(
            &mut image,
            x + TILE - FRAME,
            y,
            FRAME,
            TILE_HEIGHT,
            FRAME_COLOR,
        );
        let baseline = (y + TILE_HEIGHT + LABEL - 6) as f32;
        let mut left = x as f32;
        if let Some(text) = hero.badge {
            let color = if hero.supreme_plus { GOLD } else { LIGHT };
            draw_text(&mut image, left, baseline, TEXT_SIZE, text, color);
            left += text_width(text, TEXT_SIZE) + 5.0;
        }
        let mut right = (x + TILE) as f32;
        if let Some(level) = hero.ex_level.filter(|&level| level > 0) {
            let text = format!("+{level}");
            right -= text_width(&text, TEXT_SIZE);
            draw_text(&mut image, right, baseline, TEXT_SIZE, &text, LIGHT);
            right -= 14.0;
            draw_sword(&mut image, right, baseline + 1.0, 13.0, BLADE, GRIP);
            right -= 3.0;
        }
        let name = fitted(&hero.name, (right - left).max(0.0));
        let name_x = left + ((right - left) - text_width(&name, TEXT_SIZE)).max(0.0) / 2.0;
        draw_text(&mut image, name_x, baseline, TEXT_SIZE, &name, LIGHT);
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

    fn hero(name: &str, badge: Option<&'static str>, ex_level: Option<u32>) -> BoxHero {
        BoxHero {
            name: name.into(),
            portrait: Some(RgbaImage::from_pixel(180, 248, Rgba([10, 120, 200, 255]))),
            supreme_plus: badge == Some("P4"),
            badge,
            ex_level,
        }
    }

    fn label_area(image: &RgbaImage) -> Vec<Rgba<u8>> {
        (MARGIN..MARGIN + TILE)
            .flat_map(|x| (MARGIN + TILE_HEIGHT..MARGIN + TILE_HEIGHT + LABEL).map(move |y| (x, y)))
            .map(|(x, y)| *image.get_pixel(x, y))
            .collect()
    }

    #[test]
    fn compose_places_heroes_in_rows() {
        let heroes: Vec<BoxHero> = (0..9).map(|_| hero("Lyca", None, None)).collect();
        let image = compose(&heroes);
        assert_eq!(
            image.width(),
            MARGIN * 2 + COLUMNS * TILE + (COLUMNS - 1) * GAP
        );
        assert_eq!(image.height(), MARGIN * 2 + 2 * (TILE_HEIGHT + LABEL) + GAP);
    }

    #[test]
    fn a_short_row_is_only_as_wide_as_its_heroes() {
        let heroes: Vec<BoxHero> = (0..3).map(|_| hero("Lyca", None, None)).collect();
        assert_eq!(compose(&heroes).width(), MARGIN * 2 + 3 * TILE + 2 * GAP);
    }

    #[test]
    fn every_hero_has_the_same_frame() {
        let image = compose(&[
            hero("Lyca", Some("P4"), None),
            hero("Odie", Some("M+"), None),
        ]);
        for x in [MARGIN, MARGIN + TILE + GAP] {
            assert_eq!(*image.get_pixel(x, MARGIN + TILE_HEIGHT / 2), FRAME_COLOR);
        }
    }

    #[test]
    fn the_label_shows_ascension_name_and_ex_level() {
        let plain = label_area(&compose(&[hero("Lyca", None, None)]));
        let with_badge = label_area(&compose(&[hero("Lyca", Some("P4"), None)]));
        let with_level = label_area(&compose(&[hero("Lyca", None, Some(25))]));
        let other_level = label_area(&compose(&[hero("Lyca", None, Some(10))]));
        let other_name = label_area(&compose(&[hero("Odie", None, None)]));
        assert_ne!(plain, with_badge);
        assert_ne!(plain, with_level);
        assert_ne!(with_level, other_level);
        assert_ne!(plain, other_name);
        assert!(
            plain.iter().any(|pixel| *pixel != BACKGROUND),
            "the name is drawn"
        );
    }

    #[test]
    #[ignore = "writes a sample picture to ADBHELPER_SAMPLE_PICTURE"]
    fn sample_picture() {
        let path = std::env::var_os("ADBHELPER_SAMPLE_PICTURE").expect("output path");
        let heroes = [
            hero("Elijah & Lailah", Some("P4"), Some(25)),
            hero("Odie", Some("M+"), Some(10)),
            hero("Smokey and Meerky", Some("S+"), Some(5)),
        ];
        compose(&heroes).save(path).unwrap();
    }

    #[test]
    fn long_names_are_shortened() {
        let long = "Smokey and Meerky and Friends";
        assert!(fitted(long, 60.0).ends_with('…'));
        assert!(text_width(&fitted(long, 60.0), TEXT_SIZE) <= 60.0);
        assert_eq!(fitted("Lyca", 60.0), "Lyca");
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
