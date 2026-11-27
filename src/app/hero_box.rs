//! The Hero Box page: every hero from heroes.json with portrait, S+ frame
//! from the Resonating Hall cards, the hero database scan, and the picture
//! for Discord.

use super::*;
use crate::afk_journey::progression::hero_db::{
    is_supreme_plus, portrait_folder, tier_badge, HeroDatabase, HeroRecord,
};
use crate::afk_journey::progression::hero_portraits;
use crate::hero_box_image;
use std::collections::HashMap;

const TILE: f32 = 112.0;
/// Portraits are about 1.4 times as tall as wide (180×248).
const TILE_HEIGHT: f32 = 154.0;
const STAR: egui::Color32 = egui::Color32::from_rgb(255, 214, 90);
const GOLD: egui::Color32 = egui::Color32::from_rgb(212, 175, 55);

/// What the Hero Box page shows; loaded when the page opens and after a scan.
#[derive(Default)]
pub(super) struct HeroBox {
    loaded: bool,
    heroes: Vec<HeroRecord>,
    textures: HashMap<String, egui::TextureHandle>,
    message: Option<String>,
    filter: HeroFilter,
    /// Result of a portrait download running in the background.
    download: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
}

/// Faction and class shown in the Hero Box; `None` shows all.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct HeroFilter {
    pub faction: Option<String>,
    pub class: Option<String>,
}

impl HeroFilter {
    pub(super) fn matches(&self, hero: &HeroRecord) -> bool {
        self.faction
            .as_ref()
            .is_none_or(|faction| &hero.faction == faction)
            && self.class.as_ref().is_none_or(|class| &hero.class == class)
    }
}

/// The distinct non-empty values of one field, sorted.
fn choices(heroes: &[HeroRecord], field: impl Fn(&HeroRecord) -> &String) -> Vec<String> {
    let mut values: Vec<String> = heroes
        .iter()
        .map(|hero| field(hero).clone())
        .filter(|value| !value.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values
}

/// A drop-down with "All" and the given values.
fn filter_combo(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    all: &str,
    values: &[String],
    selected: &mut Option<String>,
) {
    ui.label(label);
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_deref().unwrap_or(all))
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, all);
            for value in values {
                ui.selectable_value(selected, Some(value.clone()), value);
            }
        });
}

impl HeroBox {
    /// Loads heroes.json again on the next frame of the page.
    pub(super) fn reload(&mut self) {
        self.loaded = false;
    }
}

/// A small filled star: five spike triangles around a pentagon.
fn paint_star(painter: &egui::Painter, center: egui::Pos2, radius: f32) {
    let point = |index: usize, r: f32| {
        let angle = std::f32::consts::PI * (index as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
        center + egui::vec2(r * angle.cos(), r * angle.sin())
    };
    let inner = radius * 0.45;
    let pentagon: Vec<egui::Pos2> = (0..5).map(|i| point(i * 2 + 1, inner)).collect();
    painter.add(egui::Shape::convex_polygon(
        pentagon,
        STAR,
        egui::Stroke::NONE,
    ));
    for i in 0..5 {
        let tip = point(i * 2, radius);
        let left = point((i * 2 + 9) % 10, inner);
        let right = point(i * 2 + 1, inner);
        painter.add(egui::Shape::convex_polygon(
            vec![tip, right, left],
            STAR,
            egui::Stroke::NONE,
        ));
    }
}

/// EX weapon stars: from the stored EX level when there is one (reliable),
/// else as counted by the scan.
fn stars_of(hero: &HeroRecord) -> Option<u32> {
    hero.progress
        .as_ref()
        .and_then(|progress| progress.ex_weapon)
        .and_then(crate::afk_journey::progression::hero_card::stars_for_ex_level)
        .or(hero.stars)
}

/// "S+", "P4", ... for the hero's ascension.
fn badge_of(hero: &HeroRecord) -> Option<&'static str> {
    hero.progress
        .as_ref()
        .and_then(|progress| progress.ascension.as_deref())
        .and_then(tier_badge)
}

fn is_s_plus(hero: &HeroRecord) -> bool {
    hero.progress
        .as_ref()
        .and_then(|progress| progress.ascension.as_deref())
        .is_some_and(is_supreme_plus)
}

/// Owned heroes first, then by name.
fn sorted(db: HeroDatabase) -> Vec<HeroRecord> {
    let mut heroes: Vec<HeroRecord> = db.heroes.into_values().collect();
    heroes.sort_by(|a, b| b.owned.cmp(&a.owned).then_with(|| a.name.cmp(&b.name)));
    heroes
}

/// The downloaded portrait, else the hall card or profile cut from the scan.
fn portrait_image(hero: &HeroRecord) -> Option<image::RgbaImage> {
    let file = hero_portraits::web_portrait(&hero.name)
        .or_else(|| Some(portrait_folder().ok()?.join(hero.portrait.as_ref()?)))?;
    image::open(file).ok().map(|image| image.to_rgba8())
}

impl PlayerApp {
    fn load_hero_box(&mut self, context: &egui::Context) {
        let db = HeroDatabase::load_quietly();
        self.hero_box.heroes = sorted(db);
        self.hero_box.textures.clear();
        for hero in &self.hero_box.heroes {
            let Some(image) = portrait_image(hero) else {
                continue;
            };
            let size = [image.width() as usize, image.height() as usize];
            let texture = context.load_texture(
                format!("hero-portrait-{}", hero.name),
                egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            self.hero_box.textures.insert(hero.name.clone(), texture);
        }
        self.hero_box.loaded = true;
    }

    /// The picture of every scanned hero, saved next to heroes.json and put
    /// on the clipboard.
    fn make_hero_picture(&mut self) {
        let heroes: Vec<hero_box_image::BoxHero> = self
            .hero_box
            .heroes
            .iter()
            .filter(|hero| hero.owned && self.hero_box.filter.matches(hero))
            .map(|hero| hero_box_image::BoxHero {
                name: hero.name.clone(),
                stars: stars_of(hero),
                portrait: portrait_image(hero),
                supreme_plus: is_s_plus(hero),
                badge: badge_of(hero),
            })
            .collect();
        if heroes.is_empty() {
            self.hero_box.message = Some(tr!("app-hero-box-nothing-scanned").into());
            return;
        }
        let picture = hero_box_image::compose(&heroes);
        let saved = crate::settings::file("hero-box.png")
            .map_err(|error| error.to_string())
            .and_then(|path| {
                picture
                    .save(&path)
                    .map(|()| path)
                    .map_err(|error| error.to_string())
            });
        let copied = hero_box_image::copy_to_clipboard(&picture);
        self.hero_box.message = Some(match (saved, copied) {
            (_, Ok(())) => tr!("app-hero-box-picture-copied", count = heroes.len()),
            (Ok(path), Err(_)) => tr!(
                "app-hero-box-picture-saved",
                path = path.display().to_string()
            ),
            (Err(error), Err(_)) => tr!("app-hero-box-picture-failed", error = error),
        });
    }

    /// Start and stop of the quick scan (ascension and EX weapon stars).
    fn hero_box_scan_controls(&mut self, ui: &mut egui::Ui) {
        ui.small(tr!("app-hero-box-scan-hint"));
        ui.horizontal(|ui| {
            let can_start = !self.busy && self.selected_serial.is_some();
            if ui
                .add_enabled(can_start, egui::Button::new(tr!("app-hero-box-scan")))
                .clicked()
            {
                self.start_action(StageAction::HeroBoxScan);
            }
            if self.busy && self.active_action == Some(StageAction::HeroBoxScan) {
                if let Some(cancel) = &self.stage_cancel {
                    if ui.button(tr!("app-settings-hero-db-stop")).clicked() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            match &self.hero_db_updated {
                Some(updated) => ui.small(tr!(
                    "app-settings-hero-db-last-scan",
                    updated = updated.clone()
                )),
                None => ui.small(tr!("app-settings-hero-db-never")),
            };
        });
        if let Some((StageAction::HeroBoxScan, error)) = &self.action_error {
            ui.add(
                egui::Label::new(egui::RichText::new(error).color(ColorTokens::DANGER_HOVER))
                    .wrap(),
            );
        }
    }

    /// Downloads missing portraits on a background thread.
    fn load_portraits(&mut self, context: &egui::Context) {
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.hero_box.download = Some(result.clone());
        let context = context.clone();
        thread::spawn(move || {
            let mut lines = Vec::new();
            let added = hero_portraits::fetch_missing(
                &mut |line| lines.push(line),
                &AtomicBool::new(false),
            );
            let message = match added {
                Ok(added) if lines.is_empty() => {
                    tr!("hero-db-portraits-added", count = added).to_owned()
                }
                Ok(_) => lines.join(" "),
                Err(error) => error.to_string(),
            };
            if let Ok(mut slot) = result.lock() {
                *slot = Some(message);
            }
            context.request_repaint();
        });
    }

    pub(super) fn show_hero_box(&mut self, ui: &mut egui::Ui) {
        let finished = self
            .hero_box
            .download
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|mut slot| slot.take()));
        if let Some(message) = finished {
            self.hero_box.download = None;
            self.hero_box.message = Some(message);
            self.hero_box.reload();
        }
        if !self.hero_box.loaded {
            self.load_hero_box(ui.ctx());
        }
        ui.heading(egui::RichText::new("Hero Box").color(ColorTokens::TEXT_PRIMARY));
        ui.label(tr!("app-hero-box-explain"));
        ui.add_space(SpaceTokens::MD);
        card(ui, |ui| {
            self.hero_box_scan_controls(ui);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.hero_box.heroes.is_empty(),
                        egui::Button::new(tr!("app-hero-box-make-picture")),
                    )
                    .clicked()
                {
                    self.make_hero_picture();
                }
                let downloading = self.hero_box.download.is_some();
                if ui
                    .add_enabled(
                        !downloading,
                        egui::Button::new(tr!("app-hero-box-load-portraits")),
                    )
                    .clicked()
                {
                    self.load_portraits(ui.ctx());
                }
                if downloading {
                    ui.spinner();
                }
                if let Some(message) = &self.hero_box.message {
                    ui.label(message.as_str());
                }
            });
        });
        ui.add_space(SpaceTokens::MD);
        let owned = self
            .hero_box
            .heroes
            .iter()
            .filter(|hero| hero.owned)
            .count();
        ui.label(tr!(
            "app-hero-box-count",
            owned = owned,
            total = self.hero_box.heroes.len()
        ));
        if self.hero_box.heroes.is_empty() {
            ui.label(tr!("app-hero-box-empty"));
            return;
        }
        let factions = choices(&self.hero_box.heroes, |hero| &hero.faction);
        let classes = choices(&self.hero_box.heroes, |hero| &hero.class);
        let all = tr!("app-hero-box-all");
        ui.horizontal(|ui| {
            filter_combo(
                ui,
                "hero-box-faction",
                tr!("app-hero-box-faction"),
                all,
                &factions,
                &mut self.hero_box.filter.faction,
            );
            ui.add_space(SpaceTokens::MD);
            filter_combo(
                ui,
                "hero-box-class",
                tr!("app-hero-box-class"),
                all,
                &classes,
                &mut self.hero_box.filter.class,
            );
        });
        ui.add_space(SpaceTokens::SM);
        let shown: Vec<&HeroRecord> = self
            .hero_box
            .heroes
            .iter()
            .filter(|hero| self.hero_box.filter.matches(hero))
            .collect();
        let columns = ((ui.available_width() + 8.0) / (TILE + 8.0))
            .floor()
            .max(1.0) as usize;
        egui::Grid::new("hero-box-grid")
            .spacing(egui::vec2(8.0, 8.0))
            .show(ui, |ui| {
                for (index, hero) in shown.iter().enumerate() {
                    self.hero_tile(ui, hero);
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn hero_tile(&self, ui: &mut egui::Ui, hero: &HeroRecord) {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(TILE, TILE_HEIGHT + 20.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let portrait = egui::Rect::from_min_size(rect.min, egui::vec2(TILE, TILE_HEIGHT));
        match self.hero_box.textures.get(&hero.name) {
            Some(texture) => {
                painter.image(
                    texture.id(),
                    portrait,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    if hero.owned {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(90)
                    },
                );
            }
            None => {
                painter.rect_filled(portrait, 6.0, egui::Color32::from_rgb(58, 62, 70));
                painter.text(
                    portrait.center(),
                    egui::Align2::CENTER_CENTER,
                    &hero.name,
                    egui::FontId::proportional(12.0),
                    if hero.owned {
                        ColorTokens::TEXT_PRIMARY
                    } else {
                        egui::Color32::from_gray(120)
                    },
                );
            }
        }
        painter.rect_stroke(
            portrait.shrink(0.5),
            6.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(92, 98, 112)),
            egui::StrokeKind::Inside,
        );
        // One line under the picture: ascension | name | EX weapon stars.
        let line_y = portrait.bottom() + 10.0;
        let small = egui::FontId::proportional(11.0);
        let mut left = rect.left();
        if let Some(text) = badge_of(hero) {
            let color = if is_s_plus(hero) {
                GOLD
            } else {
                egui::Color32::from_gray(235)
            };
            let galley = painter.layout_no_wrap(text.to_owned(), small.clone(), color);
            painter.galley(
                egui::pos2(left, line_y - galley.size().y / 2.0),
                galley.clone(),
                color,
            );
            left += galley.size().x + 4.0;
        }
        let mut right = rect.right();
        if let Some(stars) = stars_of(hero).filter(|&stars| stars > 0) {
            let galley = painter.layout_no_wrap(stars.to_string(), small.clone(), STAR);
            right -= galley.size().x;
            painter.galley(
                egui::pos2(right, line_y - galley.size().y / 2.0),
                galley,
                STAR,
            );
            right -= 7.0;
            paint_star(&painter, egui::pos2(right, line_y), 5.5);
            right -= 8.0;
        }
        let name_color = egui::Color32::from_gray(if hero.owned { 200 } else { 130 });
        let name = painter.layout_no_wrap(hero.name.clone(), small.clone(), name_color);
        let room = (right - left).max(0.0);
        let shown = if name.size().x <= room {
            name
        } else {
            let mut text: String = hero.name.clone();
            loop {
                text.pop();
                let candidate = painter.layout_no_wrap(
                    format!("{}…", text.trim_end()),
                    small.clone(),
                    name_color,
                );
                if candidate.size().x <= room || text.is_empty() {
                    break candidate;
                }
            }
        };
        let x = left + (room - shown.size().x).max(0.0) / 2.0;
        painter.galley(
            egui::pos2(x, line_y - shown.size().y / 2.0),
            shown,
            name_color,
        );
        response.on_hover_text(&hero.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(name: &str, faction: &str, class: &str) -> HeroRecord {
        HeroRecord {
            name: name.into(),
            faction: faction.into(),
            class: class.into(),
            ..HeroRecord::default()
        }
    }

    #[test]
    fn heroes_are_filtered_by_faction_and_class() {
        let heroes = [
            hero("Thoran", "Graveborn", "Tank"),
            hero("Frieren", "Celestial", "Mage"),
            hero("Odie", "Lightbearer", "Marksman"),
            hero("Vala", "Wilder", "Rogue"),
        ];
        let names = |filter: &HeroFilter| -> Vec<&str> {
            heroes
                .iter()
                .filter(|hero| filter.matches(hero))
                .map(|hero| hero.name.as_str())
                .collect()
        };
        assert_eq!(names(&HeroFilter::default()).len(), 4);
        let celestial = HeroFilter {
            faction: Some("Celestial".into()),
            class: None,
        };
        assert_eq!(names(&celestial), ["Frieren"]);
        let tank = HeroFilter {
            faction: None,
            class: Some("Tank".into()),
        };
        assert_eq!(names(&tank), ["Thoran"]);
        let none = HeroFilter {
            faction: Some("Celestial".into()),
            class: Some("Tank".into()),
        };
        assert!(names(&none).is_empty());
    }

    #[test]
    fn filter_choices_come_from_the_heroes() {
        let heroes = [
            hero("Thoran", "Graveborn", "Tank"),
            hero("Smokey", "Graveborn", "Support"),
            hero("Unknown", "", ""),
        ];
        assert_eq!(choices(&heroes, |hero| &hero.faction), ["Graveborn"]);
        assert_eq!(choices(&heroes, |hero| &hero.class), ["Support", "Tank"]);
    }
}
