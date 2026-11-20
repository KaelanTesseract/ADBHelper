//! Device screenshots, normalized to the 1080 x 1920 base resolution, and taps.

use super::Match;
use crate::adb;
use crate::afk_journey::error::ActionError;
use crate::afk_journey::BASE_RESOLUTION;
use crate::localization::tr;
use image::{imageops::FilterType, GrayImage};

pub(in crate::afk_journey) fn normalize_screenshot(
    bytes: &[u8],
) -> Result<(GrayImage, (u32, u32)), ActionError> {
    let screenshot = image::load_from_memory(bytes)
        .map_err(|error| {
            tr!(
                "vision-screen-could-not-decode-device-screenshot-error",
                error = error
            )
        })?
        .to_luma8();
    let actual_size = screenshot.dimensions();
    if actual_size.0 == 0 || actual_size.1 == 0 {
        return Err(tr!("vision-screen-device-returned-an-empty-screenshot")
            .to_owned()
            .into());
    }
    let expected_ratio = BASE_RESOLUTION.0 as f32 / BASE_RESOLUTION.1 as f32;
    let actual_ratio = actual_size.0 as f32 / actual_size.1 as f32;
    if (actual_ratio - expected_ratio).abs() > 0.04 {
        return Err(tr!(
            "vision-screen-adbhelper-expects-a-9-16-portrait",
            actual_size = actual_size.0,
            actual_size2 = actual_size.1
        )
        .into());
    }

    let normalized = if actual_size == BASE_RESOLUTION {
        screenshot
    } else {
        image::imageops::resize(
            &screenshot,
            BASE_RESOLUTION.0,
            BASE_RESOLUTION.1,
            FilterType::Lanczos3,
        )
    };
    Ok((normalized, actual_size))
}

pub(in crate::afk_journey) fn screenshot(
    serial: &str,
) -> Result<(GrayImage, (u32, u32)), ActionError> {
    let bytes = adb::screenshot(serial).map_err(|error| error.to_string())?;
    normalize_screenshot(&bytes)
}

pub(in crate::afk_journey) fn tap_match(
    serial: &str,
    found: Match,
    size: (u32, u32),
) -> Result<(), ActionError> {
    let (x, y) = found.center();
    adb::tap_game_point(serial, x, y, size)
        .map_err(|error| error.to_string())
        .map_err(ActionError::from)
}
