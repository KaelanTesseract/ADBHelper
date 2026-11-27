//! Facts from github.com/arnecls/afjk-characters. The repository has no
//! license, so only game facts are taken (faction, class, damage type,
//! range, cooldowns, unlock levels), never its texts. Fetched at run time.

use super::hero_db::{HeroFacts, SkillFacts};
use crate::afk_journey::error::ActionError;
use crate::localization::tr;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

const BASE: &str = "https://raw.githubusercontent.com/arnecls/afjk-characters/main/data/";

/// The database slot for a skill section of afjk-characters.
fn slot(section: &str) -> String {
    let key = crate::afk_journey::text_match::ascii_key(section);
    match key.as_str() {
        "ultimate" => "ult".into(),
        "skill1" | "skilli" => "s1".into(),
        "skill2" | "skillii" => "s2".into(),
        "exskill" | "exclusiveweapon" => "ex".into(),
        _ if key.contains("legendary") => "focus".into(),
        _ if key.contains("supreme") => "enhance".into(),
        _ => key,
    }
}

/// "15s" → 15.0.
fn seconds(text: &str) -> Option<f64> {
    text.trim().trim_end_matches('s').trim().parse().ok()
}

/// Facts of one hero from its `data/heroes/<id>/source.json`.
pub fn parse_facts(id: &str, file: &Value) -> Option<HeroFacts> {
    let source = file.get("source")?;
    let text = |key: &str| match source.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    let skills = source
        .get("skills")
        .and_then(Value::as_array)
        .map(|skills| {
            skills
                .iter()
                .filter_map(|skill| {
                    let section = skill.get("section")?.as_str()?;
                    Some(SkillFacts {
                        slot: slot(section),
                        cooldown_s: skill
                            .get("meta")
                            .and_then(|meta| meta.get("Cooldown"))
                            .and_then(Value::as_str)
                            .and_then(seconds),
                        unlock: skill
                            .get("unlock")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(HeroFacts {
        id: id.to_owned(),
        faction: text("faction"),
        class: text("class"),
        damage_type: text("damage_type"),
        range: text("range"),
        skills,
    })
}

fn download(path: &str) -> Result<Value, String> {
    super::scanner::client()
        .get(format!("{BASE}{path}"))
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(2 * 1024 * 1024)
                .read_json()
        })
        .map_err(|error| error.to_string())
}

/// Facts for `ids`, one download each. A failed hero is counted and
/// skipped; stopping the scan ends the downloads.
fn collect_facts(
    ids: &[&str],
    cancelled: &AtomicBool,
    mut download: impl FnMut(&str) -> Result<Value, String>,
) -> Result<(Vec<HeroFacts>, usize), ActionError> {
    let mut facts = Vec::new();
    let mut failed = 0;
    for id in ids {
        crate::afk_journey::vision::ensure_not_cancelled(cancelled)?;
        match download(id) {
            Ok(file) => facts.extend(parse_facts(id, &file)),
            Err(_) => failed += 1,
        }
    }
    Ok((facts, failed))
}

/// Facts for every hero in the roster. Missing facts are logged once; the
/// stored ones stay because merging only fills empty fields.
pub fn fetch_facts(
    log: &mut impl FnMut(String),
    cancelled: &AtomicBool,
) -> Result<Vec<HeroFacts>, ActionError> {
    let roster = match download("roster.json") {
        Ok(roster) => roster,
        Err(error) => {
            log(tr!("hero-db-facts-unavailable", error = error));
            return Ok(Vec::new());
        }
    };
    let ids: Vec<&str> = roster
        .get("heroes")
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| hero.get("id")?.as_str())
                .collect()
        })
        .unwrap_or_default();
    let (facts, failed) = collect_facts(&ids, cancelled, |id| {
        download(&format!("heroes/{id}/source.json"))
    })?;
    if failed > 0 || ids.is_empty() {
        log(tr!(
            "hero-db-facts-unavailable",
            error = format!("{failed}/{}", ids.len())
        ));
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn thoran() -> serde_json::Value {
        json!({
            "id": "thoran",
            "source": {
                "faction": "Graveborn", "class": "Tank", "damage_type": "Physical",
                "range": 1,
                "description": "TEXT THAT MUST NOT BE TAKEN",
                "skills": [
                    {"section": "Ultimate", "name": "Soul Retaliation",
                     "unlock": "Unlocks at Level 1",
                     "meta": {"Cooldown": "1s", "Initial Energy": "200"},
                     "description": {"raw": "TEXT THAT MUST NOT BE TAKEN"}},
                    {"section": "Skill1", "unlock": "Unlocks at Level 11",
                     "meta": {"Cooldown": "15s"}},
                    {"section": "Skill2", "unlock": "Unlocks at Level 31", "meta": {}},
                    {"section": "Unlocks at Legendary+", "unlock": "Unlocks at Legendary+"},
                    {"section": "Ex. Skill", "unlock": "Unlocks at Mythic+"},
                    {"section": "Unlocks at Supreme+", "unlock": "Unlocks at Supreme+"}
                ]
            }
        })
    }

    #[test]
    fn facts_take_numbers_not_texts() {
        let facts = parse_facts("thoran", &thoran()).unwrap();
        assert_eq!(facts.faction, "Graveborn");
        assert_eq!(facts.class, "Tank");
        assert_eq!(facts.damage_type, "Physical");
        assert_eq!(facts.range, "1");
        let slots: Vec<&str> = facts.skills.iter().map(|s| s.slot.as_str()).collect();
        assert_eq!(slots, ["ult", "s1", "s2", "focus", "ex", "enhance"]);
        assert_eq!(facts.skills[0].cooldown_s, Some(1.0));
        assert_eq!(facts.skills[1].cooldown_s, Some(15.0));
        assert_eq!(facts.skills[2].cooldown_s, None);
        assert_eq!(
            facts.skills[1].unlock.as_deref(),
            Some("Unlocks at Level 11")
        );
        assert!(!format!("{facts:?}").contains("MUST NOT"));
    }

    #[test]
    #[ignore = "downloads afjk-characters over the network"]
    fn facts_download() {
        let facts = fetch_facts(
            &mut |line| println!("{line}"),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        println!("{} heroes with facts", facts.len());
        assert!(facts.len() >= 130);
    }

    #[test]
    fn one_failed_hero_does_not_stop_the_others() {
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let (facts, failed) = collect_facts(&["thoran", "missing", "thoran2"], &cancelled, |id| {
            if id == "missing" {
                Err("404".into())
            } else {
                Ok(thoran())
            }
        })
        .unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(failed, 1);
    }

    #[test]
    fn stopping_ends_the_download() {
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        assert!(collect_facts(&["thoran"], &cancelled, |_| Ok(thoran())).is_err());
    }

    #[test]
    fn a_file_without_source_is_skipped() {
        assert!(parse_facts("x", &json!({"id": "x"})).is_none());
    }
}
