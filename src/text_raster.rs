//! Text and simple shapes on images for the Hero Box picture. Glyph outlines
//! come from the font egui already ships (Ubuntu Light, via skrifa, both
//! already part of the app); filling is a small anti-aliased scanline fill.

use image::{Rgba, RgbaImage};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

/// Sub-samples per pixel in each direction.
const SAMPLES: usize = 4;

fn font() -> Option<FontRef<'static>> {
    FontRef::new(epaint_default_fonts::UBUNTU_LIGHT).ok()
}

/// Closed polygons in image coordinates (y down).
#[derive(Default)]
struct Shapes {
    contours: Vec<Vec<(f32, f32)>>,
    origin: (f32, f32),
}

impl Shapes {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin.0 + x, self.origin.1 - y)
    }
    fn last(&self) -> (f32, f32) {
        self.contours
            .last()
            .and_then(|contour| contour.last())
            .copied()
            .unwrap_or(self.origin)
    }
    fn push(&mut self, point: (f32, f32)) {
        if let Some(contour) = self.contours.last_mut() {
            contour.push(point);
        }
    }
}

impl OutlinePen for Shapes {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.contours.push(vec![point]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.push(point);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (start, control, end) = (self.last(), self.point(cx0, cy0), self.point(x, y));
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            let u = 1.0 - t;
            self.push((
                u * u * start.0 + 2.0 * u * t * control.0 + t * t * end.0,
                u * u * start.1 + 2.0 * u * t * control.1 + t * t * end.1,
            ));
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let start = self.last();
        let (c0, c1, end) = (self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
        for step in 1..=10 {
            let t = step as f32 / 10.0;
            let u = 1.0 - t;
            let blend = |a: f32, b: f32, c: f32, d: f32| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            self.push((
                blend(start.0, c0.0, c1.0, end.0),
                blend(start.1, c0.1, c1.1, end.1),
            ));
        }
    }
    fn close(&mut self) {}
}

/// Fills closed polygons (non-zero winding) with anti-aliasing.
fn fill(image: &mut RgbaImage, contours: &[Vec<(f32, f32)>], color: Rgba<u8>) {
    let points = contours.iter().flatten();
    let (Some(top), Some(bottom)) = (
        points.clone().map(|p| p.1).reduce(f32::min),
        points.map(|p| p.1).reduce(f32::max),
    ) else {
        return;
    };
    let first_row = top.floor().max(0.0) as u32;
    let last_row = (bottom.ceil().max(0.0) as u32).min(image.height());
    let width = image.width() as usize;
    for row in first_row..last_row {
        let mut coverage = vec![0.0f32; width];
        for sub in 0..SAMPLES {
            let y = row as f32 + (sub as f32 + 0.5) / SAMPLES as f32;
            let mut crossings: Vec<(f32, i32)> = Vec::new();
            for contour in contours {
                for index in 0..contour.len() {
                    let a = contour[index];
                    let b = contour[(index + 1) % contour.len()];
                    if (a.1 <= y) != (b.1 <= y) {
                        let x = a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0);
                        crossings.push((x, if b.1 > a.1 { 1 } else { -1 }));
                    }
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding == 0 {
                    continue;
                }
                let (start, end) = (pair[0].0.max(0.0), pair[1].0.min(width as f32));
                let mut x = start;
                while x < end {
                    let pixel = x.floor() as usize;
                    let next = ((pixel + 1) as f32).min(end);
                    if pixel < width {
                        coverage[pixel] += (next - x) / SAMPLES as f32;
                    }
                    x = next;
                }
            }
        }
        for (x, amount) in coverage.into_iter().enumerate() {
            let amount = amount.clamp(0.0, 1.0);
            if amount > 0.0 {
                let pixel = image.get_pixel_mut(x as u32, row);
                for channel in 0..3 {
                    let old = f32::from(pixel.0[channel]);
                    let new = f32::from(color.0[channel]);
                    pixel.0[channel] = (old + (new - old) * amount).round() as u8;
                }
            }
        }
    }
}

/// Width of `text` in pixels at `size`.
pub fn text_width(text: &str, size: f32) -> f32 {
    let Some(font) = font() else {
        return 0.0;
    };
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(Size::new(size), LocationRef::default());
    text.chars()
        .filter_map(|c| charmap.map(c))
        .filter_map(|glyph| metrics.advance_width(glyph))
        .sum()
}

/// Draws `text` with its baseline at (`x`, `baseline`).
pub fn draw_text(
    image: &mut RgbaImage,
    x: f32,
    baseline: f32,
    size: f32,
    text: &str,
    color: Rgba<u8>,
) {
    let Some(font) = font() else {
        return;
    };
    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    let metrics = font.glyph_metrics(Size::new(size), LocationRef::default());
    let mut pen_x = x;
    for c in text.chars() {
        let Some(glyph) = charmap.map(c) else {
            continue;
        };
        if let Some(outline) = outlines.get(glyph) {
            let mut shapes = Shapes {
                origin: (pen_x, baseline),
                ..Shapes::default()
            };
            let settings = DrawSettings::unhinted(Size::new(size), LocationRef::default());
            if outline.draw(settings, &mut shapes).is_ok() {
                fill(image, &shapes.contours, color);
            }
        }
        pen_x += metrics.advance_width(glyph).unwrap_or(size * 0.5);
    }
}

/// A filled five-pointed star centred at (`x`, `y`).
pub fn draw_star(image: &mut RgbaImage, x: f32, y: f32, radius: f32, color: Rgba<u8>) {
    let star: Vec<(f32, f32)> = (0..10)
        .map(|index| {
            let r = if index % 2 == 0 {
                radius
            } else {
                radius * 0.45
            };
            let angle = std::f32::consts::PI * (index as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            (x + r * angle.cos(), y + r * angle.sin())
        })
        .collect();
    fill(image, &[star], color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(image: &RgbaImage) -> usize {
        image.pixels().filter(|pixel| pixel.0[0] > 128).count()
    }

    #[test]
    fn text_leaves_ink_and_longer_text_is_wider() {
        let mut image = RgbaImage::from_pixel(200, 40, Rgba([0, 0, 0, 255]));
        draw_text(
            &mut image,
            2.0,
            30.0,
            18.0,
            "Lyca",
            Rgba([255, 255, 255, 255]),
        );
        assert!(ink(&image) > 40);
        assert!(text_width("Lyca", 18.0) > text_width("Ly", 18.0));
        assert_eq!(text_width("", 18.0), 0.0);
    }

    #[test]
    fn a_star_is_filled() {
        let mut image = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        draw_star(&mut image, 20.0, 20.0, 15.0, Rgba([255, 255, 255, 255]));
        assert_eq!(*image.get_pixel(20, 20), Rgba([255, 255, 255, 255]));
        assert_eq!(*image.get_pixel(2, 38), Rgba([0, 0, 0, 255]));
    }
}
