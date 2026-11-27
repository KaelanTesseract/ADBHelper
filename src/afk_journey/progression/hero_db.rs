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
}

impl Default for HeroDatabase {
    fn default() -> Self {
        Self {
            version: 1,
            updated: String::new(),
            heroes: BTreeMap::new(),
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
    pub progress: Option<Progress>,
    pub stats: Option<Stats>,
    pub skills: Vec<Skill>,
    /// Where each part came from: "list", "facts", "stats", "skills".
    pub sources: BTreeMap<String, String>,
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
    fn old_files_load() {
        let db: HeroDatabase =
            serde_json::from_str(r#"{"heroes":{"frieren":{"name":"Frieren"}}}"#).unwrap();
        assert_eq!(db.version, 1);
        assert_eq!(db.heroes["frieren"].name, "Frieren");
        assert!(db.heroes["frieren"].skills.is_empty());
    }
}
