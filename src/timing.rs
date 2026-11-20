//! Automatic wait-time scaling for slow emulators and PCs.
//!
//! Users never configure anything. ADBHelper measures how long each device
//! screenshot takes and stretches waits, timeouts, and pauses when the device
//! is slower than the reference setup. A wait that times out raises the factor
//! a little further, so the next attempt (for example after a recovery restart)
//! is more patient. Successful waits let that extra patience fade again.

use crate::localization::tr;
use std::sync::Mutex;
use std::time::Duration;

/// Screenshot time of the setup the built-in waits were tuned on (MuMuPlayer, 1080 × 1920).
const REFERENCE_SCREENSHOT_MS: f64 = 600.0;
/// Waits never get shorter than tuned and never longer than this multiple.
const MAX_FACTOR: f64 = 3.0;
const MAX_SPEED_FACTOR: f64 = 2.5;
const TIMEOUT_BOOST: f64 = 0.25;
const MAX_BOOST: f64 = 1.0;
// About 50 successful screen matches undo one timeout boost.
const SUCCESS_DECAY: f64 = 0.005;
/// Weight of the newest screenshot in the moving average.
const SMOOTHING: f64 = 0.2;
/// Single samples above this are treated as hiccups (ADB restart, emulator stall).
const OUTLIER_MS: f64 = 15_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    average_ms: Option<f64>,
    boost: f64,
}

impl Timing {
    pub const fn new() -> Self {
        Self {
            average_ms: None,
            boost: 0.0,
        }
    }

    pub fn observe_screenshot(&mut self, elapsed: Duration) {
        let sample = elapsed.as_secs_f64() * 1000.0;
        if !sample.is_finite() || sample <= 0.0 || sample > OUTLIER_MS {
            return;
        }
        self.average_ms = Some(match self.average_ms {
            None => sample,
            Some(average) => average + SMOOTHING * (sample - average),
        });
    }

    pub fn note_timeout(&mut self) {
        self.boost = (self.boost + TIMEOUT_BOOST).min(MAX_BOOST);
    }

    pub fn note_success(&mut self) {
        self.boost = (self.boost - SUCCESS_DECAY).max(0.0);
    }

    pub fn speed_factor(&self) -> f64 {
        self.average_ms.map_or(1.0, |average| {
            (average / REFERENCE_SCREENSHOT_MS).clamp(1.0, MAX_SPEED_FACTOR)
        })
    }

    pub fn factor(&self) -> f64 {
        (self.speed_factor() + self.boost).min(MAX_FACTOR)
    }

    pub fn scale(&self, duration: Duration) -> Duration {
        duration.mul_f64(self.factor())
    }

    pub fn summary(&self) -> String {
        let factor = self.factor();
        match self.average_ms {
            Some(average) => tr!(
                "timing-wait-times-factor-screenshot-average-ms",
                factor = format!("{:.2}", factor),
                average = format!("{:.0}", average)
            ),
            None => tr!(
                "timing-wait-times-factor",
                factor = format!("{:.2}", factor)
            ),
        }
    }
}

static GLOBAL: Mutex<Timing> = Mutex::new(Timing::new());

fn with<T>(apply: impl FnOnce(&mut Timing) -> T) -> T {
    let mut timing = GLOBAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    apply(&mut timing)
}

pub fn observe_screenshot(elapsed: Duration) {
    with(|timing| timing.observe_screenshot(elapsed));
}

pub fn note_timeout() {
    with(Timing::note_timeout);
}

pub fn note_success() {
    with(Timing::note_success);
}

pub fn summary() -> String {
    with(|timing| timing.summary())
}

/// A built-in wait, stretched for the current device.
pub fn scaled(duration: Duration) -> Duration {
    with(|timing| timing.scale(duration))
}

pub fn secs(seconds: u64) -> Duration {
    scaled(Duration::from_secs(seconds))
}

pub fn millis(milliseconds: u64) -> Duration {
    scaled(Duration::from_millis(milliseconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn fast_and_reference_devices_keep_the_tuned_waits() {
        let mut timing = Timing::new();
        assert_eq!(timing.scale(ms(1000)), ms(1000));
        for _ in 0..20 {
            timing.observe_screenshot(ms(250));
        }
        assert_eq!(timing.factor(), 1.0);
        timing.observe_screenshot(ms(600));
        assert_eq!(timing.factor(), 1.0);
    }

    #[test]
    fn slow_devices_stretch_waits_up_to_the_cap() {
        let mut timing = Timing::new();
        timing.observe_screenshot(ms(1200));
        assert!((timing.factor() - 2.0).abs() < 1e-9);
        assert_eq!(
            timing.scale(Duration::from_secs(20)),
            Duration::from_secs(40)
        );
        for _ in 0..50 {
            timing.observe_screenshot(ms(9000));
        }
        assert_eq!(timing.speed_factor(), MAX_SPEED_FACTOR);
    }

    #[test]
    fn the_average_follows_gradually_and_ignores_stalls() {
        let mut timing = Timing::new();
        timing.observe_screenshot(ms(600));
        timing.observe_screenshot(ms(1600));
        assert!((timing.average_ms.unwrap() - 800.0).abs() < 1e-9);
        timing.observe_screenshot(ms(60_000));
        timing.observe_screenshot(Duration::ZERO);
        assert!((timing.average_ms.unwrap() - 800.0).abs() < 1e-9);
    }

    #[test]
    fn timeouts_add_patience_that_fades_after_successes() {
        let mut timing = Timing::new();
        timing.note_timeout();
        timing.note_timeout();
        assert!((timing.factor() - 1.5).abs() < 1e-9);
        for _ in 0..10 {
            timing.note_timeout();
        }
        assert!((timing.factor() - 2.0).abs() < 1e-9);
        for _ in 0..400 {
            timing.note_success();
        }
        assert_eq!(timing.factor(), 1.0);
    }

    #[test]
    fn total_factor_never_exceeds_the_maximum() {
        let mut timing = Timing::new();
        timing.observe_screenshot(ms(10_000));
        for _ in 0..10 {
            timing.note_timeout();
        }
        assert_eq!(timing.factor(), MAX_FACTOR);
    }
}
