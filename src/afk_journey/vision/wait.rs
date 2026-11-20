//! Polling for screens, pauses that scale with the device speed, and cancellation.

use super::{get_match, screenshot, Match, Rect, MATCH_THRESHOLD};
use crate::afk_journey::error::ActionError;
use crate::localization::tr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub(in crate::afk_journey) fn wait_for_match(
    serial: &str,
    paths: &[&str],
    roi: Rect,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<(String, Match, (u32, u32)), ActionError> {
    poll_for_match(serial, paths, roi, timeout, cancelled)?
        .ok_or_else(|| {
            tr!(
                "vision-wait-timed-out-waiting-for-or",
                or = paths.join(tr!("vision-wait-or"))
            )
        })
        .map_err(ActionError::from)
}

pub(in crate::afk_journey) fn poll_for_match(
    serial: &str,
    paths: &[&str],
    roi: Rect,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Option<(String, Match, super::ScreenSize)>, ActionError> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        ensure_not_cancelled(cancelled)?;
        let (screen, size) = screenshot(serial)?;
        if let Some((path, found)) = get_match(&screen, paths, roi, MATCH_THRESHOLD)? {
            crate::timing::note_success();
            return Ok(Some((path, found, size)));
        }
        thread::sleep(crate::timing::millis(700));
    }
    ensure_not_cancelled(cancelled)?;
    // No timing penalty here: optional checks (popups, banners) end this way by design.
    // Only a recovery restart counts as "the device was too slow".
    Ok(None)
}

pub(crate) fn ensure_not_cancelled(cancelled: &AtomicBool) -> Result<(), ActionError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ActionError::Cancelled)
    } else {
        Ok(())
    }
}

pub(in crate::afk_journey) fn settle_cancellable(
    cancelled: &AtomicBool,
) -> Result<(), ActionError> {
    // 900 ms on the reference setup, longer on slow devices; checked every 100 ms.
    let deadline = Instant::now() + crate::timing::millis(900);
    loop {
        ensure_not_cancelled(cancelled)?;
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        thread::sleep(left.min(Duration::from_millis(100)));
    }
}
