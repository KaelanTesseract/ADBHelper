//! Local Windows OCR; no server, Python runtime or downloaded model is used.
#[cfg(windows)]
use crate::localization::format_text;
#[cfg(not(windows))]
use crate::localization::text;
use image::GrayImage;

#[derive(Clone, Debug)]
pub(super) struct TextLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl TextLine {
    pub fn center(&self) -> (u32, u32) {
        (
            (self.x + self.width / 2.0) as u32,
            (self.y + self.height / 2.0) as u32,
        )
    }
}

#[cfg(windows)]
pub(super) fn recognize(image: &GrayImage) -> Result<Vec<TextLine>, String> {
    use windows::{
        core::HSTRING,
        Globalization::Language,
        Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };
    fn read(image: &GrayImage) -> windows::core::Result<Vec<TextLine>> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        struct Runtime;
        impl Drop for Runtime {
            fn drop(&mut self) {
                unsafe {
                    RoUninitialize();
                }
            }
        }
        let _runtime = Runtime;
        let engine = Language::CreateLanguage(&HSTRING::from("en-US"))
            .and_then(|language| OcrEngine::TryCreateFromLanguage(&language))
            .or_else(|_| OcrEngine::TryCreateFromUserProfileLanguages())?;
        let mut pixels = Vec::with_capacity(image.len() * 4);
        for &gray in image.as_raw() {
            pixels.extend_from_slice(&[gray, gray, gray, 255]);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&pixels)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &writer.DetachBuffer()?,
            BitmapPixelFormat::Bgra8,
            image.width() as i32,
            image.height() as i32,
        )?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let mut lines = Vec::new();
        for line in result.Lines()? {
            let mut left = f32::MAX;
            let mut top = f32::MAX;
            let mut right = 0.0f32;
            let mut bottom = 0.0f32;
            for word in line.Words()? {
                let rect = word.BoundingRect()?;
                left = left.min(rect.X);
                top = top.min(rect.Y);
                right = right.max(rect.X + rect.Width);
                bottom = bottom.max(rect.Y + rect.Height);
            }
            if left.is_finite() && top.is_finite() && right > left && bottom > top {
                lines.push(TextLine {
                    text: line.Text()?.to_string(),
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                });
            }
        }
        Ok(lines)
    }
    read(image).map_err(|error| {
        format_text!(
            "The action stopped because local Windows OCR is unavailable: {error}.",
            "Lokale Windows-Texterkennung nicht verfügbar: {error}. Die Aktion wurde angehalten."
        )
    })
}

#[cfg(not(windows))]
pub(super) fn recognize(_: &GrayImage) -> Result<Vec<TextLine>, String> {
    Err(text(
        "This action requires local Windows OCR.",
        "Für diese Aktion wird die lokale Windows-Texterkennung benötigt.",
    )
    .into())
}

pub(super) fn repeated_counter(text: &str) -> Option<u32> {
    let text = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if !text.bytes().all(|c| c.is_ascii_digit()) || ![3, 6].contains(&text.len()) {
        return None;
    }
    let len = text.len() / 3;
    let first = &text[..len];
    (first == &text[len..len * 2] && first == &text[len * 2..])
        .then(|| first.parse().ok())
        .flatten()
}

pub(super) fn repeated_numeric_patch(
    gray: &image::GrayImage,
    roi: super::afk_stages::Rect,
) -> Result<Vec<TextLine>, String> {
    // Windows OCR omits some isolated one-digit counters. Repeat the same
    // captured glyph three times as a single word; accept only three identical
    // readings. No digit is generated, guessed or taken from the Sweep price.
    let mut left = roi.width;
    let mut top = roi.height;
    let (mut right, mut bottom) = (0, 0);
    for y in 0..roi.height {
        for x in 0..roi.width {
            if gray.get_pixel(roi.x + x, roi.y + y).0[0] > 180 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    if right <= left
        || bottom <= top
        || !(6..=65).contains(&(right - left))
        || !(20..=45).contains(&(bottom - top))
    {
        return Ok(Vec::new());
    }
    let (width, height) = (right - left, bottom - top);
    let mut word = image::GrayImage::from_pixel(width * 3 + 64, height + 60, image::Luma([255]));
    for copy in 0..3 {
        for y in 0..height {
            for x in 0..width {
                let value = gray.get_pixel(roi.x + left + x, roi.y + top + y).0[0];
                word.put_pixel(
                    30 + copy * (width + 2) + x,
                    30 + y,
                    image::Luma([if value > 180 { 0 } else { 255 }]),
                );
            }
        }
    }
    let word = image::imageops::resize(
        &word,
        word.width() * 4,
        word.height() * 4,
        image::imageops::FilterType::CatmullRom,
    );
    recognize(&word)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "reads an explicitly supplied screenshot using installed Windows OCR"]
    fn local_ocr_diagnostic() {
        let path = std::env::var("ADBHELPER_OCR_IMAGE").expect("set ADBHELPER_OCR_IMAGE");
        let image = image::open(path).unwrap().to_luma8();
        for line in recognize(&image).unwrap() {
            println!(
                "{:.0},{:.0} {:.0}x{:.0}: {}",
                line.x, line.y, line.width, line.height, line.text
            );
        }
    }
}
