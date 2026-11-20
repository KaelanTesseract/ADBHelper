//! Normalized cross-correlation template search on 1080 x 1920 grayscale screens.

use super::{load_template, Match, Rect};
use crate::afk_journey::BASE_RESOLUTION;
use image::GrayImage;

#[derive(Clone, Copy)]
pub(in crate::afk_journey) struct Sample {
    x: u32,
    y: u32,
    value: f64,
}

pub(in crate::afk_journey) fn template_samples(template: &GrayImage, stride: u32) -> Vec<Sample> {
    let mut samples = Vec::new();
    for y in (0..template.height()).step_by(stride as usize) {
        for x in (0..template.width()).step_by(stride as usize) {
            samples.push(Sample {
                x,
                y,
                value: f64::from(template.get_pixel(x, y)[0]),
            });
        }
    }
    samples
}

pub(in crate::afk_journey) fn normalized_correlation(
    image: &GrayImage,
    origin: (u32, u32),
    samples: &[Sample],
) -> f32 {
    let count = samples.len() as f64;
    let (mut template_sum, mut template_square_sum) = (0.0, 0.0);
    let (mut image_sum, mut image_square_sum, mut product_sum) = (0.0, 0.0, 0.0);
    for sample in samples {
        let image_value = f64::from(image.get_pixel(origin.0 + sample.x, origin.1 + sample.y)[0]);
        template_sum += sample.value;
        template_square_sum += sample.value * sample.value;
        image_sum += image_value;
        image_square_sum += image_value * image_value;
        product_sum += sample.value * image_value;
    }
    let covariance = product_sum - template_sum * image_sum / count;
    let template_variance = template_square_sum - template_sum * template_sum / count;
    let image_variance = image_square_sum - image_sum * image_sum / count;
    let divisor = (template_variance * image_variance).sqrt();
    if divisor <= f64::EPSILON {
        -1.0
    } else {
        (covariance / divisor) as f32
    }
}

pub(in crate::afk_journey) fn find_template(
    screenshot: &GrayImage,
    relative_path: &str,
    roi: Rect,
    threshold: f32,
) -> Result<Option<Match>, String> {
    let template = load_template(relative_path)?;
    let right = roi.x.saturating_add(roi.width).min(screenshot.width());
    let bottom = roi.y.saturating_add(roi.height).min(screenshot.height());
    if roi.x >= right
        || roi.y >= bottom
        || template.width() > right - roi.x
        || template.height() > bottom - roi.y
    {
        return Ok(None);
    }

    let coarse_samples = template_samples(&template, 4);
    // Small high-contrast text/edges can lose their true peak on a four-pixel
    // grid. Records must also match between grid positions after floor loading.
    let search_step = if matches!(
        relative_path,
        "afk_stages/season_battle.png" | "battle/records.png"
    ) {
        1
    } else if template.height() <= 40 {
        // Thin labels (including Union's orange Skip) need a finer grid.
        2
    } else {
        4
    };
    let x_end = right - template.width();
    let y_end = bottom - template.height();
    let mut candidates = Vec::<(f32, u32, u32)>::new();
    for y in (roi.y..=y_end).step_by(search_step) {
        for x in (roi.x..=x_end).step_by(search_step) {
            let confidence = normalized_correlation(screenshot, (x, y), &coarse_samples);
            if candidates.len() < 5 || confidence > candidates[0].0 {
                candidates.push((confidence, x, y));
                candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
                if candidates.len() > 5 {
                    candidates.remove(0);
                }
            }
        }
    }

    let fine_samples = template_samples(&template, 2);
    let mut best = None::<(f32, u32, u32)>;
    for (_, coarse_x, coarse_y) in candidates {
        let start_x = coarse_x.saturating_sub(3).max(roi.x);
        let start_y = coarse_y.saturating_sub(3).max(roi.y);
        let end_x = (coarse_x + 3).min(x_end);
        let end_y = (coarse_y + 3).min(y_end);
        for y in start_y..=end_y {
            for x in start_x..=end_x {
                let confidence = normalized_correlation(screenshot, (x, y), &fine_samples);
                if best.is_none_or(|(best_confidence, _, _)| confidence > best_confidence) {
                    best = Some((confidence, x, y));
                }
            }
        }
    }

    Ok(best.and_then(|(confidence, x, y)| {
        (confidence >= threshold).then_some(Match {
            x,
            y,
            width: template.width(),
            height: template.height(),
            confidence,
        })
    }))
}

pub(in crate::afk_journey) fn full_screen() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: BASE_RESOLUTION.0,
        height: BASE_RESOLUTION.1,
    }
}

pub(in crate::afk_journey) fn lower_screen() -> Rect {
    Rect {
        x: 0,
        y: BASE_RESOLUTION.1 * 2 / 5,
        width: BASE_RESOLUTION.0,
        height: BASE_RESOLUTION.1 * 3 / 5,
    }
}

pub(in crate::afk_journey) fn get_match(
    screenshot: &GrayImage,
    paths: &[&str],
    roi: Rect,
    threshold: f32,
) -> Result<Option<(String, Match)>, String> {
    let mut best = None::<(&str, Match)>;
    for path in paths {
        if let Some(found) = find_template(screenshot, path, roi, threshold)? {
            if best.is_none_or(|(_, current)| found.confidence > current.confidence) {
                best = Some((path, found));
            }
        }
    }
    Ok(best.map(|(path, found)| (path.to_owned(), found)))
}
