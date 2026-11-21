//! Pure route logic for searching the world map for fishing spots: which part
//! of the map to look at next, when a view shows no map, which map tabs exist
//! and which spots are given up for this run. Nothing here touches the device.

use super::{Point, Rect};
use crate::afk_journey::{ocr::TextLine, text_match::ascii_key};
use image::GrayImage;
use std::collections::{HashMap, HashSet};

/// The free map area between the title, the zoom slider and the tab strip.
pub(super) const MAP_VIEW: Rect = Rect {
    x: 40,
    y: 115,
    width: 930,
    height: 1310,
};
/// Buttons inside `MAP_VIEW` that are not map: the side buttons top left and
/// the Homestead button bottom left.
const MAP_BUTTONS: [Rect; 2] = [
    Rect {
        x: 0,
        y: 0,
        width: 430,
        height: 560,
    },
    Rect {
        x: 0,
        y: 1340,
        width: 120,
        height: 580,
    },
];
/// A view with fewer bright pixels than this shows no map, only the dark
/// background beyond the map's edge (measured 0.001–0.003 empty, 0.009 and
/// more with map).
const EMPTY_VIEW: f32 = 0.005;
const BRIGHT: u8 = 60;
/// The strip of map tabs below the map.
pub(super) const TAB_STRIP: Rect = Rect {
    x: 0,
    y: 1540,
    width: 1080,
    height: 110,
};
/// Finger travel of one grid step; the map moves about 550 px.
const STEP: u32 = 600;
/// Swipe duration; 500 ms swipes were sometimes ignored by the game.
pub(super) const SWIPE_MS: u32 = 700;
/// Empty views in a row that are still crossed, so water between islands does
/// not end the search.
const MAX_GAP: u8 = 2;
pub(super) const MAX_VIEWS_PER_MAP: usize = 80;
pub(super) const MAX_VISITS_PER_MAP: u32 = 20;

/// Direction the view moves on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Heading {
    Up,
    Down,
    Left,
    Right,
}

/// The finger path for one grid step. Looking further right means dragging the
/// map to the left.
pub(super) fn swipe_for(heading: Heading) -> (Point, Point) {
    let (cx, cy) = (540, 800);
    let half = STEP / 2;
    match heading {
        Heading::Right => ((cx + half, cy), (cx - half, cy)),
        Heading::Left => ((cx - half, cy), (cx + half, cy)),
        Heading::Down => ((cx, cy + half), (cx, cy - half)),
        Heading::Up => ((cx, cy - half), (cx, cy + half)),
    }
}

/// True when the view shows only the dark background beyond the map.
pub(super) fn is_empty_view(frame: &GrayImage) -> bool {
    let (mut bright, mut total) = (0u32, 0u32);
    for y in (MAP_VIEW.y..MAP_VIEW.y + MAP_VIEW.height).step_by(4) {
        for x in (MAP_VIEW.x..MAP_VIEW.x + MAP_VIEW.width).step_by(4) {
            if x >= frame.width() || y >= frame.height() || on_button(x, y) {
                continue;
            }
            total += 1;
            bright += u32::from(frame.get_pixel(x, y).0[0] > BRIGHT);
        }
    }
    total == 0 || (bright as f32) < EMPTY_VIEW * total as f32
}

fn on_button(x: u32, y: u32) -> bool {
    MAP_BUTTONS
        .iter()
        .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
}

type Cell = (i32, i32);

/// Flood fill over grid views, starting where the map opened. Every view with
/// map is scanned once; its neighbours are visited, crossing up to
/// `MAX_GAP - 1` empty views.
pub(super) struct Explorer {
    at: Cell,
    max_views: usize,
    /// Gap (empty views since the last map) of every visited cell.
    seen: HashMap<Cell, u8>,
    queue: Vec<Cell>,
    queued: HashSet<Cell>,
}

impl Explorer {
    pub(super) fn new(max_views: usize) -> Self {
        Self {
            at: (0, 0),
            max_views,
            seen: HashMap::new(),
            queue: Vec::new(),
            queued: HashSet::new(),
        }
    }

    /// Records what the current view shows. Returns true when the view is new
    /// and shows map, so it should be searched for markers.
    pub(super) fn record(&mut self, empty: bool) -> bool {
        if self.seen.contains_key(&self.at) {
            return false;
        }
        let gap = if empty {
            1 + self
                .neighbours(self.at)
                .iter()
                .filter_map(|c| self.seen.get(c))
                .min()
                .copied()
                .unwrap_or(0)
        } else {
            0
        };
        self.seen.insert(self.at, gap);
        if gap < MAX_GAP {
            for cell in self.neighbours(self.at) {
                if !self.seen.contains_key(&cell) && self.queued.insert(cell) {
                    self.queue.push(cell);
                }
            }
        }
        !empty
    }

    /// The next grid step towards the nearest view still to visit; `None` when
    /// every reachable view was seen or the view limit is reached.
    pub(super) fn next_move(&mut self) -> Option<Heading> {
        self.queue.retain(|c| !self.seen.contains_key(c));
        if self.seen.len() >= self.max_views {
            return None;
        }
        let at = self.at;
        let target = *self
            .queue
            .iter()
            .min_by_key(|c| ((c.0 - at.0).abs() + (c.1 - at.1).abs(), c.1, c.0))?;
        let heading = if target.0 > at.0 {
            Heading::Right
        } else if target.0 < at.0 {
            Heading::Left
        } else if target.1 > at.1 {
            Heading::Down
        } else {
            Heading::Up
        };
        self.at = match heading {
            Heading::Right => (at.0 + 1, at.1),
            Heading::Left => (at.0 - 1, at.1),
            Heading::Down => (at.0, at.1 + 1),
            Heading::Up => (at.0, at.1 - 1),
        };
        Some(heading)
    }

    fn neighbours(&self, (x, y): Cell) -> [Cell; 4] {
        [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
    }
}

/// A map tab in the strip below the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tab {
    pub name: String,
    pub at: Point,
}

/// Unlocked map tabs from the OCR lines of the screen, left to right. A tab
/// with a lock icon (`locks`, centres) is left out; clipped names such as
/// "Arcane Affa…" are kept as read.
pub(super) fn map_tabs(lines: &[TextLine], locks: &[Point]) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = lines
        .iter()
        .filter_map(|line| {
            let at = line.center();
            let inside = at.1 >= TAB_STRIP.y && at.1 < TAB_STRIP.y + TAB_STRIP.height;
            let name = ascii_key(&line.text);
            let locked = locks.iter().any(|lock| lock.0.abs_diff(at.0) <= 120);
            // Percentages such as "45%" share the tab but are not names.
            let is_name = name.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3;
            (inside && is_name && !locked).then_some(Tab { name, at })
        })
        .collect();
    tabs.sort_by_key(|tab| tab.at.0);
    tabs
}

/// Spots given up for this run and how often each map sent the character
/// walking, so no spot or map can keep the run busy forever.
pub(super) struct Visits {
    cap_per_map: u32,
    walks: HashMap<String, u32>,
    blocked: HashSet<(String, String)>,
}

impl Visits {
    pub(super) fn new(cap_per_map: u32) -> Self {
        Self {
            cap_per_map,
            walks: HashMap::new(),
            blocked: HashSet::new(),
        }
    }

    /// Counts one walk on `map`; false once the map reached its limit.
    pub(super) fn visit(&mut self, map: &str) -> bool {
        let walks = self.walks.entry(map.to_owned()).or_default();
        if *walks >= self.cap_per_map {
            return false;
        }
        *walks += 1;
        true
    }

    pub(super) fn block(&mut self, map: &str, spot: &str) {
        self.blocked.insert((map.to_owned(), spot.to_owned()));
    }

    pub(super) fn is_blocked(&self, map: &str, spot: &str) -> bool {
        self.blocked.contains(&(map.to_owned(), spot.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the explorer over a map given as content cells and returns the
    /// cells it searched and how many views it looked at.
    fn explore(content: &[Cell], max_views: usize) -> (Vec<Cell>, usize) {
        let mut explorer = Explorer::new(max_views);
        let mut scanned = Vec::new();
        let mut views = 0;
        loop {
            views += 1;
            assert!(views < 1000, "explorer does not end");
            if explorer.record(!content.contains(&explorer.at)) {
                scanned.push(explorer.at);
            }
            if explorer.next_move().is_none() {
                break;
            }
        }
        (scanned, explorer.seen.len())
    }

    #[test]
    fn explorer_searches_every_part_of_an_irregular_map_once() {
        // An L-shaped main land and an island across one empty view.
        let content = [(0, 0), (1, 0), (2, 0), (0, 1), (0, 2), (2, 2)];
        let (mut scanned, _) = explore(&content, 200);
        scanned.sort();
        let mut expected = content.to_vec();
        expected.sort();
        assert_eq!(scanned, expected);
    }

    #[test]
    fn explorer_does_not_cross_two_empty_views() {
        let content = [(0, 0), (3, 0)];
        let (scanned, _) = explore(&content, 200);
        assert_eq!(scanned, vec![(0, 0)]);
    }

    #[test]
    fn explorer_stops_at_the_view_limit() {
        let content: Vec<Cell> = (0..20).flat_map(|x| (0..20).map(move |y| (x, y))).collect();
        let (_, views) = explore(&content, 15);
        assert_eq!(views, 15);
    }

    #[test]
    fn explorer_moves_one_grid_step_at_a_time() {
        let mut explorer = Explorer::new(50);
        explorer.record(false);
        let before = explorer.at;
        explorer.next_move().unwrap();
        let after = explorer.at;
        assert_eq!((before.0 - after.0).abs() + (before.1 - after.1).abs(), 1);
    }

    #[test]
    fn swipes_drag_the_map_against_the_heading() {
        let (from, to) = swipe_for(Heading::Right);
        assert!(from.0 > to.0 && from.1 == to.1);
        let (from, to) = swipe_for(Heading::Down);
        assert!(from.1 > to.1 && from.0 == to.0);
        assert_eq!(from.1 - to.1, STEP);
    }

    fn view(fill: u8, bright_patch: u32) -> GrayImage {
        let mut image = GrayImage::from_pixel(1080, 1920, image::Luma([fill]));
        for y in 800..800 + bright_patch {
            for x in 500..900 {
                image.put_pixel(x, y, image::Luma([200]));
            }
        }
        image
    }

    #[test]
    fn dark_background_is_empty_and_map_is_not() {
        assert!(is_empty_view(&view(20, 0)));
        // 400x60 bright pixels: about 2 % of the view, like a map edge.
        assert!(!is_empty_view(&view(20, 60)));
        // Bright buttons at the left do not count as map.
        let mut buttons = view(20, 0);
        for y in 150..550 {
            for x in 40..420 {
                buttons.put_pixel(x, y, image::Luma([230]));
            }
        }
        assert!(is_empty_view(&buttons));
    }

    fn line(text: &str, x: f32) -> TextLine {
        TextLine {
            text: text.into(),
            x: x - 60.0,
            y: 1580.0,
            width: 120.0,
            height: 30.0,
        }
    }

    #[test]
    fn tabs_keep_clipped_names_and_skip_locked_ones() {
        let lines = vec![
            line("Mortimer", 960.0),
            line("Arcane Affa", 120.0),
            line("45%", 330.0),
            line("Novaris", 400.0),
            line("Refinery", 680.0),
        ];
        let tabs = map_tabs(&lines, &[(940, 1594)]);
        let names: Vec<_> = tabs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["arcaneaffa", "novaris", "refinery"]);
        assert_eq!(tabs[1].at, (400, 1595));
    }

    #[test]
    fn text_outside_the_strip_is_no_tab() {
        let mut title = line("Novaris", 200.0);
        title.y = 40.0;
        assert!(map_tabs(&[title], &[]).is_empty());
    }

    #[test]
    fn visit_cap_stops_a_map_and_blocks_are_per_map() {
        let mut visits = Visits::new(2);
        assert!(visits.visit("novaris") && visits.visit("novaris"));
        assert!(!visits.visit("novaris"));
        assert!(visits.visit("refinery"));
        visits.block("novaris", "luckyspot");
        assert!(visits.is_blocked("novaris", "luckyspot"));
        assert!(!visits.is_blocked("refinery", "luckyspot"));
    }
}
