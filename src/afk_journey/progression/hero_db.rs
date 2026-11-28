//! Local hero database `heroes.json`: every hero from the AFKJ Tracker roster,
//! facts from afjk-characters (numbers only, no texts), and what the hero
//! database scan read in the game.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const FILE: &str = "heroes.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeroDatabase {
    pub version: u32,
    /// Local date and time of the last write, "2026-11-27 15:00:00".
    pub updated: String,
    pub heroes: BTreeMap<String, HeroRecord>,
    /// Heroes a scan read but could not name for certain. The Hero Box shows
    /// them until the user picks the right hero.
    pub unrecognized: Vec<Unrecognized>,
}

impl Default for HeroDatabase {
    fn default() -> Self {
        Self {
            version: 1,
            updated: String::new(),
            heroes: BTreeMap::new(),
            unrecognized: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeroRecord {
    pub name: String,
    pub faction: String,
    pub class: String,
    pub rarity: String,
    pub damage_type: String,
    pub range: String,
    pub owned: bool,
    /// Stars on the hero's card in the Resonating Hall.
    pub stars: Option<u32>,
    /// File name of the portrait cut from the user's own game, in
    /// `heroes/portraits` next to `heroes.json`.
    pub portrait: Option<String>,
    pub progress: Option<Progress>,
    pub stats: Option<Stats>,
    pub skills: Vec<Skill>,
    /// Where each part came from: "list", "facts", "stats", "skills", and
    /// "user" for a hero the user added in the Hero Box.
    pub sources: BTreeMap<String, String>,
    /// The title above the hero's name on its profile ("Misty Scorpion");
    /// every hero has its own, so it identifies the profile.
    pub epithet: String,
    /// Misread names the user assigned to this hero (name keys).
    pub read_names: Vec<String>,
}

/// A hero profile the scan could not name for certain.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Unrecognized {
    /// The name as the OCR read it.
    pub name_read: String,
    /// The title above the name as read (name key); may be empty.
    pub epithet: String,
    pub stars: Option<u32>,
    pub progress: Option<Progress>,
    pub stats: Option<Stats>,
    /// Profile cut and the header with title and name, files in the
    /// portraits folder.
    pub portrait: Option<String>,
    pub header: Option<String>,
    /// "unknown": no hero fits; "twice": two profiles of one scan were read
    /// as the same hero.
    pub reason: String,
}

/// Who an unrecognized hero is, as the user picked it in the Hero Box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Assignment {
    /// A hero of the database, by id.
    Hero(String),
    /// A hero no list knows yet, by name.
    New(String),
}

/// True for heroes from the roster or added by the user; only these can be
/// found by a scan. Records from misread names are not.
pub fn is_known(record: &HeroRecord) -> bool {
    record.sources.contains_key("list") || record.sources.contains_key("user")
}

/// True when `read` is at least as new as `stored` (dates "YYYY-MM-DD").
fn newer(read: Option<&str>, stored: Option<&str>) -> bool {
    match (read, stored) {
        (Some(read), Some(stored)) => read >= stored,
        (Some(_), None) => true,
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub level: Option<u32>,
    pub ascension: Option<String>,
    pub ex_weapon: Option<u32>,
    pub scanned: String,
}

/// Current values from the game's "Hero Stats", keyed by the label's
/// `ascii_key` ("atk", "physdef", "crit", ...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub values: BTreeMap<String, f64>,
    pub scanned: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Skill {
    /// "ult", "s1", "s2", ..., "ex", "enhance".
    pub slot: String,
    pub name: String,
    pub level: Option<u32>,
    pub cooldown_s: Option<f64>,
    pub range: Option<String>,
    pub energy: Option<u32>,
    pub unlock: Option<String>,
    pub levels: Vec<SkillLevel>,
    /// Text read in the user's own game; never copied from other sources.
    pub text: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillLevel {
    pub level: u32,
    /// The numbers in the order they appear in the skill text.
    pub values: Vec<f64>,
}

/// Facts about one hero from afjk-characters: numbers and categories only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeroFacts {
    pub id: String,
    pub faction: String,
    pub class: String,
    pub damage_type: String,
    pub range: String,
    pub skills: Vec<SkillFacts>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillFacts {
    pub slot: String,
    pub cooldown_s: Option<f64>,
    pub unlock: Option<String>,
}

/// The short badge of an ascension as shown in the Hero Box: "S+", "P3",
/// "M+", ... `None` for a value the scan does not know.
pub fn tier_badge(ascension: &str) -> Option<&'static str> {
    Some(match ascension {
        "Paragon 4" => "P4",
        "Paragon 3" => "P3",
        "Paragon 2" => "P2",
        "Paragon 1" => "P1",
        "Supreme+" => "S+",
        "Supreme" => "S",
        "Mythic+" => "M+",
        "Mythic" => "M",
        "Legendary+" => "L+",
        "Legendary" => "L",
        "Epic+" | "Elite+" => "E+",
        "Epic" | "Elite" => "E",
        _ => return None,
    })
}

/// Supreme+ or higher (Paragon 1–4); these badges are drawn in gold.
pub fn is_supreme_plus(ascension: &str) -> bool {
    ascension == "Supreme+" || ascension.starts_with("Paragon")
}

/// Folder of the portraits, next to `heroes.json`.
pub fn portrait_folder() -> std::io::Result<std::path::PathBuf> {
    crate::settings::file("heroes").map(|folder| folder.join("portraits"))
}

/// The tracker's hero id: lowercase, spaces become "-".
pub fn hero_id(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn fill(field: &mut String, value: &str) {
    if !value.is_empty() {
        value.clone_into(field);
    }
}

fn stamp() -> String {
    let now = crate::logfile::Stamp::now();
    format!("{} {}", now.date(), now.time())
}

/// Today's local date, "2026-11-27".
pub fn today() -> String {
    crate::logfile::Stamp::now().date()
}

impl HeroDatabase {
    /// The stored database. A file that cannot be read as JSON is moved to
    /// `heroes.json.broken`, and the warning says so; a fresh database starts.
    pub fn load() -> Result<(Self, Option<String>), String> {
        Self::load_from(&crate::settings::file(FILE).map_err(|e| e.to_string())?)
    }

    /// When `heroes.json` was last written. Only reads: a broken file stays
    /// where it is, the scan sets it aside and says so.
    pub fn updated() -> Option<String> {
        Self::updated_at(&crate::settings::file(FILE).ok()?)
    }

    /// The stored database for display; a missing or broken file gives an
    /// empty one and stays where it is.
    pub fn load_quietly() -> Self {
        crate::settings::file(FILE)
            .ok()
            .and_then(|path| Self::read_only(&path))
            .unwrap_or_default()
    }

    fn read_only(path: &std::path::Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    fn updated_at(path: &std::path::Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<Self>(&bytes)
            .ok()
            .map(|db| db.updated)
            .filter(|updated| !updated.is_empty())
    }

    fn load_from(path: &std::path::Path) -> Result<(Self, Option<String>), String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), None))
            }
            Err(error) => return Err(error.to_string()),
        };
        match serde_json::from_slice(&bytes) {
            Ok(db) => Ok((db, None)),
            Err(error) => {
                let broken = path.with_extension("json.broken");
                std::fs::rename(path, &broken).map_err(|e| e.to_string())?;
                Ok((
                    Self::default(),
                    Some(format!("{}: {error}", broken.display())),
                ))
            }
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.updated = stamp();
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::settings::write(FILE, &bytes).map_err(|e| e.to_string())
    }

    /// The record for `name`, created when the hero is not known yet.
    pub fn record(&mut self, name: &str) -> &mut HeroRecord {
        let record = self.heroes.entry(hero_id(name)).or_default();
        if record.name.is_empty() {
            name.trim().clone_into(&mut record.name);
        }
        record
    }

    /// Adds every hero of the AFKJ Tracker template (`{"heroes": [...]}`) and
    /// updates name, class, faction and rarity; scanned data stays.
    pub fn merge_roster(&mut self, template: &Value) {
        let Some(heroes) = template.get("heroes").and_then(Value::as_array) else {
            return;
        };
        for hero in heroes {
            let text = |key: &str| hero.get(key).and_then(Value::as_str).unwrap_or_default();
            let name = text("name");
            if name.is_empty() {
                continue;
            }
            let id = Some(text("id"))
                .filter(|id| !id.is_empty())
                .map_or_else(|| hero_id(name), str::to_owned);
            if !self.heroes.contains_key(&id) {
                // A hero scanned before the tracker listed it sits under its
                // name key; it moves to the tracker id.
                let key = crate::afk_journey::text_match::ascii_key(name);
                let earlier = self
                    .heroes
                    .iter()
                    .find(|(_, record)| {
                        crate::afk_journey::text_match::ascii_key(&record.name) == key
                    })
                    .map(|(old, _)| old.clone());
                if let Some(record) = earlier.and_then(|old| self.heroes.remove(&old)) {
                    self.heroes.insert(id.clone(), record);
                }
            }
            let record = self.heroes.entry(id).or_default();
            fill(&mut record.name, name);
            fill(&mut record.class, text("class"));
            fill(&mut record.faction, text("faction"));
            fill(&mut record.rarity, text("rarity"));
            record.sources.insert("list".into(), "afkj-tracker".into());
        }
    }

    /// Adds facts from afjk-characters. Categories fill empty fields only;
    /// skill cooldowns and unlocks are added to the matching slot.
    pub fn merge_facts(&mut self, facts: &[HeroFacts]) {
        for fact in facts {
            let Some(record) = self.heroes.get_mut(&fact.id) else {
                continue;
            };
            for (field, value) in [
                (&mut record.faction, &fact.faction),
                (&mut record.class, &fact.class),
                (&mut record.damage_type, &fact.damage_type),
                (&mut record.range, &fact.range),
            ] {
                if field.is_empty() {
                    fill(field, value);
                }
            }
            for skill in &fact.skills {
                let index = match record.skills.iter().position(|s| s.slot == skill.slot) {
                    Some(index) => index,
                    None => {
                        record.skills.push(Skill {
                            slot: skill.slot.clone(),
                            source: "afjk-characters".into(),
                            ..Skill::default()
                        });
                        record.skills.len() - 1
                    }
                };
                let target = &mut record.skills[index];
                target.cooldown_s = target.cooldown_s.or(skill.cooldown_s);
                if target.unlock.is_none() {
                    target.unlock.clone_from(&skill.unlock);
                }
            }
            record
                .sources
                .insert("facts".into(), "afjk-characters".into());
        }
    }

    /// Adds titles from afjk-characters: `(names, title)` per hero, where
    /// `names` are the hero's names and aliases there ("Natsu", "Twins",
    /// "Elijah & Lailah"). A hero is found by a name, or by the start of its
    /// longer name ("Natsu" for "Natsu Dragneel"), and only when that is clear.
    pub fn merge_epithets(&mut self, titles: &[(Vec<String>, String)]) {
        use crate::afk_journey::text_match::ascii_key;
        for (names, title) in titles {
            let keys: Vec<String> = names
                .iter()
                .flat_map(|name| [ascii_key(name), ascii_key(&name.replace('&', "and"))])
                .filter(|key| key.len() >= 3)
                .collect();
            let fits = |exact: bool| -> Vec<String> {
                self.heroes
                    .iter()
                    .filter(|(_, record)| is_known(record))
                    .filter(|(_, record)| {
                        let name = ascii_key(&record.name);
                        keys.iter().any(|key| {
                            if exact {
                                &name == key
                            } else {
                                key.len() >= 4 && name.starts_with(key.as_str())
                            }
                        })
                    })
                    .map(|(id, _)| id.clone())
                    .collect()
            };
            let mut found = fits(true);
            if found.is_empty() {
                found = fits(false);
            }
            if let [id] = found.as_slice() {
                let record = self.heroes.get_mut(id).expect("id was just found");
                title.trim().clone_into(&mut record.epithet);
            }
        }
    }

    /// Keeps an unrecognized hero; a later reading of the same profile (same
    /// title, or same name when no title was read) replaces the earlier one.
    pub fn add_unrecognized(&mut self, entry: Unrecognized) {
        let same = |old: &Unrecognized| {
            if entry.epithet.is_empty() {
                old.epithet.is_empty() && old.name_read == entry.name_read
            } else {
                old.epithet == entry.epithet
            }
        };
        self.unrecognized.retain(|old| !same(old));
        self.unrecognized.push(entry);
    }

    /// Records left from misread names in older versions become
    /// unrecognized heroes for the user to assign.
    pub fn move_strays(&mut self) {
        let strays: Vec<String> = self
            .heroes
            .iter()
            .filter(|(_, record)| !is_known(record))
            .map(|(id, _)| id.clone())
            .collect();
        for id in strays {
            let Some(record) = self.heroes.remove(&id) else {
                continue;
            };
            if !record.owned {
                continue;
            }
            self.add_unrecognized(Unrecognized {
                name_read: record.name,
                epithet: String::new(),
                stars: record.stars,
                progress: record.progress,
                stats: record.stats,
                portrait: record.portrait,
                header: None,
                reason: "unknown".into(),
            });
        }
    }

    /// Puts the unrecognized hero at `index` where the user said it belongs
    /// and remembers its title and misread name for later scans. Returns the
    /// hero's id.
    pub fn assign(&mut self, index: usize, to: Assignment) -> Result<String, String> {
        use crate::afk_journey::text_match::ascii_key;
        if index >= self.unrecognized.len() {
            return Err(format!("no unrecognized hero {index}"));
        }
        let id = match &to {
            Assignment::Hero(id) if self.heroes.contains_key(id) => id.clone(),
            Assignment::Hero(id) => return Err(format!("unknown hero {id}")),
            Assignment::New(name) if ascii_key(name).len() >= 2 => {
                let id = hero_id(name);
                let record = self.record(name);
                record.sources.insert("user".into(), "hero-box".into());
                id
            }
            Assignment::New(name) => return Err(format!("invalid name {name:?}")),
        };
        let entry = self.unrecognized.remove(index);
        let title_taken = !entry.epithet.is_empty()
            && self.heroes.iter().any(|(other, record)| {
                other != &id && ascii_key(&record.epithet) == ascii_key(&entry.epithet)
            });
        let record = self.heroes.get_mut(&id).expect("id exists");
        record.owned = true;
        let scanned = |progress: &Option<Progress>| progress.as_ref().map(|p| p.scanned.clone());
        if newer(
            scanned(&entry.progress).as_deref(),
            scanned(&record.progress).as_deref(),
        ) {
            record.progress = entry.progress;
            record.stars = entry.stars.or(record.stars);
        }
        let stats_day = |stats: &Option<Stats>| stats.as_ref().map(|s| s.scanned.clone());
        if newer(
            stats_day(&entry.stats).as_deref(),
            stats_day(&record.stats).as_deref(),
        ) {
            record.stats = entry.stats;
        }
        if record.epithet.is_empty() && !entry.epithet.is_empty() && !title_taken {
            record.epithet = entry.epithet;
        }
        let read = ascii_key(&entry.name_read);
        if read.len() >= 2 && read != ascii_key(&record.name) && !record.read_names.contains(&read)
        {
            record.read_names.push(read);
        }
        if record.portrait.is_none() {
            record.portrait = entry.portrait;
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hero_ids_match_the_tracker() {
        assert_eq!(hero_id("Elijah & Lailah"), "elijah-&-lailah");
        assert_eq!(hero_id("Frieren"), "frieren");
    }

    #[test]
    fn roster_adds_every_hero_and_keeps_scanned_data() {
        let mut db = HeroDatabase::default();
        db.record("Frieren").stats = Some(Stats {
            values: [("atk".to_owned(), 5.0)].into(),
            scanned: "2026-11-27".into(),
        });
        db.merge_roster(&json!({"heroes": [
            {"id": "frieren", "name": "Frieren", "class": "Mage",
             "faction": "Celestial", "rarity": "Cele/Hypo"},
            {"id": "thoran", "name": "Thoran", "class": "Tank",
             "faction": "Graveborn", "rarity": "S-Level"}
        ]}));
        let frieren = &db.heroes["frieren"];
        assert_eq!(frieren.class, "Mage");
        assert_eq!(frieren.faction, "Celestial");
        assert_eq!(frieren.rarity, "Cele/Hypo");
        assert_eq!(frieren.stats.as_ref().unwrap().values["atk"], 5.0);
        assert!(!frieren.owned);
        assert_eq!(db.heroes["thoran"].name, "Thoran");
        assert_eq!(db.heroes["thoran"].sources["list"], "afkj-tracker");
    }

    #[test]
    fn facts_fill_only_empty_fields_and_never_texts() {
        let mut db = HeroDatabase::default();
        db.record("Frieren").skills.push(Skill {
            slot: "ult".into(),
            name: "Zoltraak".into(),
            text: "read in the game".into(),
            source: "game".into(),
            ..Skill::default()
        });
        db.merge_facts(&[HeroFacts {
            id: "frieren".into(),
            faction: "Celestial".into(),
            class: "Mage".into(),
            damage_type: "Magic".into(),
            range: "Ranged".into(),
            skills: vec![
                SkillFacts {
                    slot: "ult".into(),
                    cooldown_s: Some(0.0),
                    unlock: None,
                },
                SkillFacts {
                    slot: "s1".into(),
                    cooldown_s: Some(15.0),
                    unlock: Some("Lv. 11".into()),
                },
            ],
        }]);
        let frieren = &db.heroes["frieren"];
        assert_eq!(frieren.damage_type, "Magic");
        assert_eq!(frieren.range, "Ranged");
        let ult = frieren.skills.iter().find(|s| s.slot == "ult").unwrap();
        assert_eq!(ult.text, "read in the game");
        assert_eq!(ult.cooldown_s, Some(0.0));
        let s1 = frieren.skills.iter().find(|s| s.slot == "s1").unwrap();
        assert_eq!(s1.unlock.as_deref(), Some("Lv. 11"));
        assert!(s1.text.is_empty());
        assert_eq!(s1.source, "afjk-characters");
    }

    #[test]
    fn an_unknown_hero_gets_its_own_record() {
        let mut db = HeroDatabase::default();
        db.record("Thane").owned = true;
        assert_eq!(db.heroes["thane"].name, "Thane");
        assert!(db.heroes["thane"].owned);
    }

    #[test]
    fn a_broken_file_is_kept_aside_not_overwritten() {
        let folder = std::env::temp_dir().join(format!("hero-db-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("heroes.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let (db, warning) = HeroDatabase::load_from(&path).unwrap();
        assert!(db.heroes.is_empty());
        assert!(warning.unwrap().contains("heroes.json.broken"));
        assert_eq!(
            std::fs::read(folder.join("heroes.json.broken")).unwrap(),
            b"{ not json"
        );
        assert!(!path.exists());
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn reading_the_date_never_moves_a_broken_file() {
        let folder = std::env::temp_dir().join(format!("hero-db-peek-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("heroes.json");
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(HeroDatabase::updated_at(&path), None);
        assert!(HeroDatabase::read_only(&path).is_none());
        assert!(path.exists(), "only the scan may set a broken file aside");
        std::fs::write(&path, br#"{"updated":"2026-11-27 15:00:00"}"#).unwrap();
        assert_eq!(
            HeroDatabase::updated_at(&path).as_deref(),
            Some("2026-11-27 15:00:00")
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn every_ascension_has_a_short_badge() {
        assert_eq!(tier_badge("Supreme+"), Some("S+"));
        assert_eq!(tier_badge("Supreme"), Some("S"));
        assert_eq!(tier_badge("Paragon 1"), Some("P1"));
        assert_eq!(tier_badge("Paragon 4"), Some("P4"));
        assert_eq!(tier_badge("Mythic+"), Some("M+"));
        assert_eq!(tier_badge("Legendary+"), Some("L+"));
        assert_eq!(tier_badge("Epic"), Some("E"));
        assert_eq!(tier_badge("Elite+"), Some("E+"));
        assert_eq!(tier_badge("something"), None);
    }

    #[test]
    fn supreme_plus_includes_paragon() {
        assert!(is_supreme_plus("Supreme+"));
        assert!(is_supreme_plus("Paragon 1"));
        assert!(is_supreme_plus("Paragon 4"));
        assert!(!is_supreme_plus("Supreme"));
        assert!(!is_supreme_plus("Mythic+"));
    }

    #[test]
    fn old_files_without_stars_load() {
        let db: HeroDatabase =
            serde_json::from_str(r#"{"heroes":{"frieren":{"name":"Frieren"}}}"#).unwrap();
        assert_eq!(db.heroes["frieren"].stars, None);
        assert_eq!(db.heroes["frieren"].portrait, None);
    }

    #[test]
    fn old_files_load() {
        let db: HeroDatabase =
            serde_json::from_str(r#"{"heroes":{"frieren":{"name":"Frieren"}}}"#).unwrap();
        assert_eq!(db.version, 1);
        assert_eq!(db.heroes["frieren"].name, "Frieren");
        assert!(db.heroes["frieren"].skills.is_empty());
    }

    fn listed(names: &[&str]) -> HeroDatabase {
        let heroes: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
        let mut db = HeroDatabase::default();
        db.merge_roster(&json!({ "heroes": heroes }));
        db
    }

    fn progress(ascension: &str, ex_weapon: u32, scanned: &str) -> Option<Progress> {
        Some(Progress {
            level: None,
            ascension: Some(ascension.into()),
            ex_weapon: Some(ex_weapon),
            scanned: scanned.into(),
        })
    }

    #[test]
    fn titles_reach_heroes_under_every_name() {
        let mut db = listed(&[
            "Phraesto",
            "Natsu Dragneel",
            "Elijah & Lailah",
            "Smokey and Meerky",
            "Lucius",
            "Lucca",
        ]);
        db.merge_epithets(&[
            (vec!["Phraesto".into()], "Misty Scorpion".into()),
            (vec!["Natsu".into()], "Fire Dragon Slayer Mage".into()),
            (
                vec!["Elijah & Lailah".into(), "Twins".into()],
                "Celestial Twins".into(),
            ),
            (
                vec!["Smokey & Meerky".into()],
                "Wasteland Apothecary".into(),
            ),
            // "Luc" is too short to point at one hero.
            (vec!["Luc".into()], "Nobody".into()),
        ]);
        assert_eq!(db.heroes["phraesto"].epithet, "Misty Scorpion");
        assert_eq!(
            db.heroes["natsu-dragneel"].epithet,
            "Fire Dragon Slayer Mage"
        );
        assert_eq!(db.heroes["elijah-&-lailah"].epithet, "Celestial Twins");
        assert_eq!(
            db.heroes["smokey-and-meerky"].epithet,
            "Wasteland Apothecary"
        );
        assert_eq!(db.heroes["lucius"].epithet, "");
        assert_eq!(db.heroes["lucca"].epithet, "");
    }

    #[test]
    fn records_from_misread_names_wait_for_the_user() {
        let mut db = listed(&["Solise"]);
        let stray = db.record("lige");
        stray.owned = true;
        stray.progress = progress("Paragon 4", 10, "2026-11-28");
        stray.portrait = Some("lige.png".into());
        db.record("never-owned");
        db.move_strays();
        assert_eq!(db.heroes.keys().collect::<Vec<_>>(), ["solise"]);
        assert_eq!(db.unrecognized.len(), 1);
        let waiting = &db.unrecognized[0];
        assert_eq!(waiting.name_read, "lige");
        assert_eq!(waiting.portrait.as_deref(), Some("lige.png"));
        assert_eq!(waiting.progress, progress("Paragon 4", 10, "2026-11-28"));
    }

    #[test]
    fn the_same_profile_is_kept_once() {
        let mut db = HeroDatabase::default();
        for ex_weapon in [5, 10] {
            db.add_unrecognized(Unrecognized {
                name_read: format!("read{ex_weapon}"),
                epithet: "palevotary".into(),
                progress: progress("Supreme", ex_weapon, "2026-11-28"),
                ..Unrecognized::default()
            });
        }
        db.add_unrecognized(Unrecognized {
            name_read: "lige".into(),
            ..Unrecognized::default()
        });
        assert_eq!(db.unrecognized.len(), 2);
        assert_eq!(
            db.unrecognized[0].progress,
            progress("Supreme", 10, "2026-11-28")
        );
    }

    #[test]
    fn an_assigned_hero_takes_the_reading_and_remembers_it() {
        let mut db = listed(&["Tephesia", "Temesia"]);
        db.heroes.get_mut("temesia").unwrap().epithet = "Lightsavior".into();
        db.heroes.get_mut("tephesia").unwrap().progress = progress("Supreme", 5, "2026-11-20");
        db.add_unrecognized(Unrecognized {
            name_read: "lige".into(),
            epithet: "palevotary".into(),
            stars: Some(3),
            progress: progress("Paragon 4", 10, "2026-11-28"),
            portrait: Some("unrecognized-palevotary.png".into()),
            reason: "unknown".into(),
            ..Unrecognized::default()
        });
        assert_eq!(
            db.assign(0, Assignment::Hero("tephesia".into())),
            Ok("tephesia".into())
        );
        assert!(db.unrecognized.is_empty());
        let tephesia = &db.heroes["tephesia"];
        assert!(tephesia.owned);
        assert_eq!(tephesia.progress, progress("Paragon 4", 10, "2026-11-28"));
        assert_eq!(tephesia.stars, Some(3));
        assert_eq!(tephesia.epithet, "palevotary");
        assert_eq!(tephesia.read_names, ["lige"]);
        assert_eq!(
            tephesia.portrait.as_deref(),
            Some("unrecognized-palevotary.png")
        );
    }

    #[test]
    fn an_assignment_never_takes_another_heros_title_or_older_progress() {
        let mut db = listed(&["Tephesia", "Temesia"]);
        db.heroes.get_mut("temesia").unwrap().epithet = "Lightsavior".into();
        db.heroes.get_mut("tephesia").unwrap().progress = progress("Paragon 1", 10, "2026-11-28");
        db.add_unrecognized(Unrecognized {
            name_read: "tephesia".into(),
            epithet: "lightsavior".into(),
            progress: progress("Supreme", 5, "2026-11-20"),
            ..Unrecognized::default()
        });
        db.assign(0, Assignment::Hero("tephesia".into())).unwrap();
        let tephesia = &db.heroes["tephesia"];
        assert_eq!(tephesia.epithet, "");
        assert_eq!(tephesia.progress, progress("Paragon 1", 10, "2026-11-28"));
        assert!(
            tephesia.read_names.is_empty(),
            "her own name is no misreading"
        );
    }

    #[test]
    fn a_new_hero_can_be_added_by_name() {
        let mut db = listed(&["Solise"]);
        db.add_unrecognized(Unrecognized {
            name_read: "lige".into(),
            epithet: "wanderingfox".into(),
            ..Unrecognized::default()
        });
        assert!(db.assign(0, Assignment::New(" ".into())).is_err());
        assert!(db.assign(0, Assignment::Hero("nobody".into())).is_err());
        assert_eq!(
            db.unrecognized.len(),
            1,
            "a failed assignment keeps the hero"
        );
        assert_eq!(
            db.assign(0, Assignment::New("Liga".into())),
            Ok("liga".into())
        );
        let liga = &db.heroes["liga"];
        assert!(is_known(liga));
        assert!(liga.owned);
        assert_eq!(liga.epithet, "wanderingfox");
    }
}
