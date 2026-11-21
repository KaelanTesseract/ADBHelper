//! Shared screen reading for all AFK Journey modes: template images, matching,
//! device screenshots, waiting, and cancellation.

mod matching;
mod screen;
mod stream;
mod templates;
mod wait;

pub(in crate::afk_journey) use matching::*;
pub(in crate::afk_journey) use screen::*;
pub(in crate::afk_journey) use stream::ScreenStream;
pub(in crate::afk_journey) use templates::*;
pub(crate) use wait::ensure_not_cancelled;
pub(in crate::afk_journey) use wait::*;

/// A pixel position on the 1080 x 1920 base screen.
pub(in crate::afk_journey) type Point = (u32, u32);
/// Width and height of the device screenshot in pixels.
pub(in crate::afk_journey) type ScreenSize = (u32, u32);

pub(in crate::afk_journey) const MATCH_THRESHOLD: f32 = 0.78;
#[derive(Clone, Copy)]
pub(in crate::afk_journey) struct Rect {
    pub(in crate::afk_journey) x: u32,
    pub(in crate::afk_journey) y: u32,
    pub(in crate::afk_journey) width: u32,
    pub(in crate::afk_journey) height: u32,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::afk_journey) struct Match {
    pub(in crate::afk_journey) x: u32,
    pub(in crate::afk_journey) y: u32,
    pub(in crate::afk_journey) width: u32,
    pub(in crate::afk_journey) height: u32,
    pub(in crate::afk_journey) confidence: f32,
}

impl Match {
    pub(in crate::afk_journey) fn rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
    pub(in crate::afk_journey) fn center(self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}
