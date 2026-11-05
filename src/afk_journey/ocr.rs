//! Local Windows OCR; no server, Python runtime or downloaded model is used.
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
        format!("Windows-OCR nicht verfügbar: {error}. Automatischer Sweep bleibt gesperrt.")
    })
}

#[cfg(not(windows))]
pub(super) fn recognize(_: &GrayImage) -> Result<Vec<TextLine>, String> {
    Err("Für den sicheren Sweep wird die lokale Windows-OCR benötigt.".into())
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
