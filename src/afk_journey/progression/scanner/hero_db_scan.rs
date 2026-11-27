//! The hero database scan: every owned hero's progress, current stats and
//! skills into `heroes.json`, plus the full roster and facts.

use super::hero::{ascension, exclusive_level, reference};
use super::{tr, ActivityEvent, Driver, GameMode};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::progression::hero_db::portrait_folder;
use crate::afk_journey::progression::hero_db::{today, HeroDatabase, HeroRecord, Progress, Skill};
use crate::afk_journey::progression::{
    hall, hero_card, hero_facts, hero_portraits, hero_skills, hero_stats,
};
use crate::afk_journey::text_match::ascii_key;
use crate::afk_journey::vision;
use std::collections::BTreeMap;

/// True when the hero has no stats yet, or they are older than
/// `max_age_days`. `None` scans every hero.
pub(super) fn needs_scan(record: &HeroRecord, today: &str, max_age_days: Option<u32>) -> bool {
    let Some(max_age) = max_age_days else {
        return true;
    };
    let Some(stats) = &record.stats else {
        return true;
    };
    match (
        crate::logfile::day_number(&stats.scanned),
        crate::logfile::day_number(today),
    ) {
        (Some(scanned), Some(now)) => now - scanned > i64::from(max_age),
        _ => true,
    }
}

/// The record whose name matches the profile's name key ("frieren",
/// "elijahlailah"); a hero the roster does not know gets a new record.
pub(super) fn record_for<'a>(db: &'a mut HeroDatabase, key: &str) -> &'a mut HeroRecord {
    let known = db
        .heroes
        .iter()
        .find(|(_, record)| ascii_key(&record.name) == key)
        .map(|(id, _)| id.clone());
    match known {
        Some(id) => db.heroes.get_mut(&id).expect("id was just found"),
        None => db.record(key),
    }
}

/// Like `record_for`, but an OCR variant listed in the tracker's
/// `hero_synonyms.json` (alias key → name key) finds the real hero.
pub(super) fn record_for_alias<'a>(
    db: &'a mut HeroDatabase,
    key: &str,
    aliases: &BTreeMap<String, String>,
) -> &'a mut HeroRecord {
    // Only roster heroes count as known: a record from an earlier misread
    // must not catch later reads of the real hero.
    let known = |db: &HeroDatabase, key: &str| {
        db.heroes
            .values()
            .any(|record| ascii_key(&record.name) == key && record.sources.contains_key("list"))
    };
    if !known(db, key) {
        if let Some(real) = aliases.get(key).filter(|real| known(db, real)) {
            return record_for(db, real);
        }
        if let Some(id) = closest_hero(db, key) {
            return db.heroes.get_mut(&id).expect("closest hero exists");
        }
    }
    record_for(db, key)
}

/// The tracker's aliases as alias key → name key; empty when unavailable.
fn alias_keys() -> BTreeMap<String, String> {
    reference("hero_synonyms.json")
        .ok()
        .and_then(|json| serde_json::from_value::<BTreeMap<String, String>>(json).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(alias, name)| (ascii_key(&alias), ascii_key(&name)))
        .collect()
}

/// Weapon levels exist from Mythic+ on, as in the AFKJ Tracker scan.
pub(super) fn has_weapon(ascension: &str) -> bool {
    matches!(ascension, "Mythic+" | "Supreme" | "Supreme+") || ascension.starts_with("Paragon")
}

/// New progress; a value that could not be read keeps the earlier one.
pub(super) fn merged_progress(
    old: Option<&Progress>,
    ascension: Option<String>,
    ex_weapon: Option<u32>,
    day: &str,
) -> Progress {
    Progress {
        level: old.and_then(|old| old.level),
        ascension: ascension.or_else(|| old.and_then(|old| old.ascension.clone())),
        ex_weapon: ex_weapon.or_else(|| old.and_then(|old| old.ex_weapon)),
        scanned: day.to_owned(),
    }
}

/// Puts the skills read in the game in place of earlier ones of the same
/// slot, keeping the facts (cooldown, unlock) those had.
pub(super) fn replace_game_skills(record: &mut HeroRecord, skills: Vec<Skill>) {
    for mut skill in skills {
        if let Some(index) = record.skills.iter().position(|old| old.slot == skill.slot) {
            let old = record.skills.remove(index);
            skill.cooldown_s = skill.cooldown_s.or(old.cooldown_s);
            if skill.unlock.is_none() {
                skill.unlock = old.unlock;
            }
        }
        record.skills.push(skill);
    }
}

/// Edit distance between two name keys.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(previous + usize::from(ca != *cb));
            previous = current;
        }
    }
    row[b.len()]
}

/// The roster hero whose name is closest to a misread profile name key
/// ("lvca" → Lyca, "natsu" → Natsu Dragneel). Only a clear single best match
/// counts; a name that fits no hero gives `None`.
pub(super) fn closest_hero(db: &HeroDatabase, key: &str) -> Option<String> {
    if key.len() < 3 {
        return None;
    }
    let mut scored: Vec<(usize, &String)> = db
        .heroes
        .iter()
        .filter(|(_, record)| record.sources.contains_key("list"))
        .map(|(id, record)| {
            let name = ascii_key(&record.name);
            let score = if key.len() >= 4 && name.starts_with(key) {
                1
            } else {
                distance(key, &name)
            };
            let allowed = (name.len().max(key.len()) / 3).max(1);
            (if score <= allowed { score } else { usize::MAX }, id)
        })
        .filter(|(score, _)| *score != usize::MAX)
        .collect();
    scored.sort();
    match scored.as_slice() {
        [(best, id), rest @ ..] if rest.first().is_none_or(|(next, _)| next > best) => {
            Some((*id).clone())
        }
        _ => None,
    }
}

/// Moves what was stored under a misread name to the hero it belongs to,
/// when that hero has no scanned data of its own.
pub(super) fn merge_strays(db: &mut HeroDatabase) {
    let strays: Vec<String> = db
        .heroes
        .iter()
        .filter(|(_, record)| record.owned && !record.sources.contains_key("list"))
        .map(|(id, _)| id.clone())
        .collect();
    for stray in strays {
        let key = ascii_key(&db.heroes[&stray].name);
        let Some(target) = closest_hero(db, &key) else {
            continue;
        };
        let Some(record) = db.heroes.remove(&stray) else {
            continue;
        };
        let hero = db.heroes.get_mut(&target).expect("target exists");
        hero.owned = true;
        // The later reading wins; on the same day the misread one was the
        // last write, because the real hero could not be found then.
        let newer = match (&record.progress, &hero.progress) {
            (Some(read), Some(stored)) => read.scanned >= stored.scanned,
            (Some(_), None) => true,
            _ => false,
        };
        if newer {
            hero.progress = record.progress;
        }
        hero.stars = record.stars.or(hero.stars);
        hero.stats = record.stats.or(hero.stats.take());
        if record.skills.iter().any(|skill| skill.source == "game") {
            replace_game_skills(hero, record.skills);
        }
        for (part, source) in record.sources {
            hero.sources.entry(part).or_insert(source);
        }
    }
}

/// What the AFKJ Tracker scan read: roster name, ascension, EX weapon level.
pub(super) type ScannedProgress = (String, String, Option<u32>);

/// Roster and progress from the AFKJ Tracker scan; stats and skills stay.
pub(super) fn apply_progress(
    db: &mut HeroDatabase,
    template: &serde_json::Value,
    scanned: &[ScannedProgress],
    day: &str,
) {
    db.merge_roster(template);
    for (name, ascension, ex_weapon) in scanned {
        let record = record_for(db, &ascii_key(name));
        record.owned = true;
        record.progress = Some(Progress {
            level: None,
            ascension: Some(ascension.clone()),
            ex_weapon: *ex_weapon,
            scanned: day.to_owned(),
        });
    }
}

/// Stores what the AFKJ Tracker scan read; a failure is only logged, the
/// tracker export does not depend on it.
pub(super) fn store_progress(
    template: &serde_json::Value,
    scanned: &[ScannedProgress],
    log: &mut impl FnMut(String),
) {
    let result = HeroDatabase::load().and_then(|(mut db, warning)| {
        if let Some(warning) = warning {
            log(tr!("hero-db-broken-file-kept", file = warning));
        }
        apply_progress(&mut db, template, scanned, &today());
        db.save()
    });
    if let Err(error) = result {
        log(tr!("hero-db-not-saved", error = error));
    }
}

/// Adds the AFKJ Tracker roster; on failure the stored roster stays.
pub(super) fn merge_tracker_roster(db: &mut HeroDatabase, log: &mut impl FnMut(String)) {
    match reference("heroes-template.json") {
        Ok(template) => db.merge_roster(&template),
        Err(error) => log(tr!("hero-db-roster-unavailable", error = error.to_string())),
    }
}

/// Saves the hero's picture (its Resonating Hall card, or a cut from the
/// profile when no card is known); returns its file name.
fn save_portrait(
    name: &str,
    picture: &image::RgbImage,
    log: &mut impl FnMut(String),
) -> Option<String> {
    let file = format!("{}.png", ascii_key(name));
    let result = portrait_folder()
        .map_err(|error| error.to_string())
        .and_then(|folder| {
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            picture
                .save(folder.join(&file))
                .map_err(|error| error.to_string())
        });
    match result {
        Ok(()) => Some(file),
        Err(error) => {
            log(tr!("hero-db-not-saved", error = error));
            None
        }
    }
}

/// The hero profile again after Hero Stats or the skill tab closed.
fn back_on_profile(driver: &Driver<'_>) -> Result<super::Screen, ActionError> {
    driver
        .wait_for(hall::hero_profile)?
        .ok_or_else(|| tr!("hero-db-profile-lost").into())
}

fn save(db: &mut HeroDatabase) -> Result<(), ActionError> {
    db.save()
        .map_err(|error| ActionError::from(tr!("hero-db-not-saved", error = error)))
}

/// How much the hero database scan reads per hero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// Ascension, EX weapon, stars, current stats and skills (Settings).
    Full { max_age_days: Option<u32> },
    /// Only ascension, EX weapon and its stars (Hero Box).
    Quick,
}

pub(super) fn scan(
    driver: &Driver<'_>,
    depth: Depth,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
) -> Result<u32, ActionError> {
    let (mut db, warning) = HeroDatabase::load().map_err(ActionError::from)?;
    if let Some(warning) = warning {
        log(tr!("hero-db-broken-file-kept", file = warning));
    }
    merge_tracker_roster(&mut db, log);
    merge_strays(&mut db);
    if matches!(depth, Depth::Full { .. }) {
        db.merge_facts(&hero_facts::fetch_facts(log, driver.cancelled)?);
    }
    let added = hero_portraits::fetch_missing(log, driver.cancelled)?;
    if added > 0 {
        log(tr!("hero-db-portraits-added", count = added));
    }
    save(&mut db)?;
    let aliases = alias_keys();
    let roster_size = db.heroes.len().max(1);
    let day = today();
    let mut count = 0;
    hall::open_first_hero(driver, log)?;
    let (mut profile, mut nameplate) = hall::wait_for_affinity_profile(driver, None)?;
    let first = nameplate.clone();
    for position in 0..roster_size + 30 {
        vision::ensure_not_cancelled(driver.cancelled)?;
        let name = hall::hero_name(&profile).unwrap_or_default();
        if matches!(name.as_str(), "chippy" | "hammie")
            || profile
                .template("dailies/resonating_hall/chippy.png", 15..590, 20..330, 0.88)?
                .is_some()
            || (position > 0 && first.matches(&nameplate))
        {
            break;
        }
        if !name.is_empty() {
            let ascension = ascension(&profile)?.map(str::to_owned);
            let ex_weapon = match ascension.as_deref() {
                Some(read) if has_weapon(read) => exclusive_level(&profile)?,
                _ => None,
            };
            let weapon = ascension.as_deref().is_some_and(has_weapon);
            // The EX level gives the stars; counting them is only the fallback.
            let stars = ex_weapon
                .and_then(hero_card::stars_for_ex_level)
                .or_else(|| hero_card::count_ex_stars(&profile.color));
            let fresh = {
                let record = record_for_alias(&mut db, &name, &aliases);
                record.owned = true;
                // Stars under the EX weapon; none before Mythic+.
                record.stars = if weapon { stars.or(record.stars) } else { None };
                if record.portrait.is_none() && hero_portraits::web_portrait(&record.name).is_none()
                {
                    record.portrait =
                        save_portrait(&record.name, &hero_card::portrait(&profile.color), log);
                }
                record.progress = Some(merged_progress(
                    record.progress.as_ref(),
                    ascension,
                    ex_weapon,
                    &day,
                ));
                match depth {
                    Depth::Full { max_age_days } => needs_scan(record, &day, max_age_days),
                    Depth::Quick => false,
                }
            };
            if fresh {
                let stats = hero_stats::read_stats(driver, &profile)?;
                profile = back_on_profile(driver)?;
                let skills = hero_skills::read_skills(driver, &profile)?;
                profile = back_on_profile(driver)?;
                let record = record_for_alias(&mut db, &name, &aliases);
                if let Some(stats) = stats {
                    record.stats = Some(stats);
                    record.sources.insert("stats".into(), "game".into());
                }
                if !skills.is_empty() {
                    replace_game_skills(record, skills);
                    record.sources.insert("skills".into(), "game".into());
                }
            }
            save(&mut db)?;
            count += 1;
            report(ActivityEvent::Collected(GameMode::HeroDatabase, 1));
        }
        driver.tap(&profile, hall::NEXT_HERO)?;
        (profile, nameplate) = hall::wait_for_affinity_profile(driver, Some(&nameplate))?;
    }
    driver.back()?;
    if count == 0 {
        return Err(tr!("hero-db-no-hero-read").into());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::afk_journey::progression::hero_db::{HeroRecord, Skill, Stats};

    fn with_stats(scanned: &str) -> HeroRecord {
        HeroRecord {
            stats: Some(Stats {
                scanned: scanned.into(),
                ..Stats::default()
            }),
            ..HeroRecord::default()
        }
    }

    #[test]
    fn the_tracker_scan_records_progress_without_touching_stats() {
        let mut db = HeroDatabase::default();
        record_for(&mut db, "frieren").stats = Some(Stats {
            scanned: "2026-11-20".into(),
            ..Stats::default()
        });
        let template = serde_json::json!({"heroes": [
            {"id": "frieren", "name": "Frieren", "class": "Mage"}
        ]});
        apply_progress(
            &mut db,
            &template,
            &[("Frieren".into(), "Supreme+".into(), Some(25))],
            "2026-11-27",
        );
        let frieren = &db.heroes["frieren"];
        assert!(frieren.owned);
        let progress = frieren.progress.as_ref().unwrap();
        assert_eq!(progress.ascension.as_deref(), Some("Supreme+"));
        assert_eq!(progress.ex_weapon, Some(25));
        assert_eq!(progress.scanned, "2026-11-27");
        assert_eq!(frieren.stats.as_ref().unwrap().scanned, "2026-11-20");
        assert_eq!(db.heroes.len(), 1, "the roster name and the record match");
    }

    #[test]
    fn a_hero_scanned_before_the_tracker_knew_it_is_merged_later() {
        let mut db = HeroDatabase::default();
        record_for(&mut db, "ashwing").owned = true;
        db.merge_roster(&serde_json::json!({"heroes": [
            {"id": "ash-wing", "name": "Ash Wing", "class": "Mage"}
        ]}));
        assert_eq!(db.heroes.len(), 1, "{:?}", db.heroes.keys());
        assert!(db.heroes["ash-wing"].owned);
        assert_eq!(db.heroes["ash-wing"].name, "Ash Wing");
    }

    #[test]
    fn ocr_variants_use_the_tracker_aliases() {
        // Heroes come from the tracker roster, as in a scan.
        let mut db = roster(&["Elijah & Lailah"]);
        let aliases = [("elijahlilah".to_owned(), "elijahlailah".to_owned())].into();
        record_for_alias(&mut db, "elijahlilah", &aliases).owned = true;
        assert!(db.heroes["elijah-&-lailah"].owned);
        assert_eq!(db.heroes.len(), 1);
    }

    #[test]
    fn game_skills_keep_the_facts_of_their_slot() {
        let mut record = HeroRecord::default();
        record.skills.push(Skill {
            slot: "s1".into(),
            cooldown_s: Some(15.0),
            unlock: Some("Unlocks at Level 11".into()),
            source: "afjk-characters".into(),
            ..Skill::default()
        });
        replace_game_skills(
            &mut record,
            vec![Skill {
                slot: "s1".into(),
                name: "Soul Plunder".into(),
                source: "game".into(),
                ..Skill::default()
            }],
        );
        assert_eq!(record.skills.len(), 1);
        let s1 = &record.skills[0];
        assert_eq!(s1.name, "Soul Plunder");
        assert_eq!(s1.unlock.as_deref(), Some("Unlocks at Level 11"));
        assert_eq!(s1.cooldown_s, Some(15.0));
    }

    #[test]
    fn unreadable_progress_keeps_what_was_read_before() {
        let old = Progress {
            ascension: Some("Supreme+".into()),
            ex_weapon: Some(25),
            scanned: "2026-11-20".into(),
            ..Progress::default()
        };
        let new = merged_progress(Some(&old), None, None, "2026-11-27");
        assert_eq!(new.ascension.as_deref(), Some("Supreme+"));
        assert_eq!(new.ex_weapon, Some(25));
        assert_eq!(new.scanned, "2026-11-27");
        let read = merged_progress(Some(&old), Some("Paragon 1".into()), Some(30), "2026-11-27");
        assert_eq!(read.ascension.as_deref(), Some("Paragon 1"));
        assert_eq!(read.ex_weapon, Some(30));
    }

    #[test]
    fn heroes_below_mythic_plus_have_no_weapon_to_read() {
        assert!(!has_weapon("Legendary+"));
        assert!(has_weapon("Mythic+"));
        assert!(has_weapon("Supreme"));
        assert!(has_weapon("Paragon 2"));
    }

    fn roster(names: &[&str]) -> HeroDatabase {
        let heroes: Vec<serde_json::Value> = names
            .iter()
            .map(|name| serde_json::json!({"name": name}))
            .collect();
        let mut db = HeroDatabase::default();
        db.merge_roster(&serde_json::json!({ "heroes": heroes }));
        db
    }

    #[test]
    fn misread_names_find_the_closest_hero() {
        let db = roster(&[
            "Berial",
            "Odie",
            "Estrilda",
            "Fay",
            "Florabelle",
            "Hepler",
            "Lily May",
            "Lorsan",
            "Lyca",
            "Natsu Dragneel",
            "Smokey and Meerky",
            "Solise",
            "Faramor",
        ]);
        for (read, hero) in [
            ("beria", "berial"),
            ("die", "odie"),
            ("estrida", "estrilda"),
            ("fav", "fay"),
            ("florabee", "florabelle"),
            ("heper", "hepler"),
            ("lilvmav", "lily-may"),
            ("lorsana", "lorsan"),
            ("lvca", "lyca"),
            ("natsu", "natsu-dragneel"),
            ("smokeymeerky", "smokey-and-meerky"),
            ("soise", "solise"),
        ] {
            assert_eq!(closest_hero(&db, read).as_deref(), Some(hero), "{read}");
        }
        assert_eq!(closest_hero(&db, "zzzzzz"), None);
    }

    #[test]
    #[ignore = "walks the open hero profiles with next on an emulator and saves chosen ones"]
    fn connected_emulator_save_profiles() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let out = std::path::PathBuf::from(std::env::var("ADBHELPER_PROFILE_OUT").unwrap());
        let wanted: Vec<String> = std::env::var("ADBHELPER_PROFILE_NAMES")
            .unwrap()
            .split(',')
            .map(str::to_owned)
            .collect();
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let driver = Driver {
            serial: &serial,
            cancelled: &cancelled,
        };
        let (mut profile, mut nameplate) = hall::wait_for_affinity_profile(&driver, None).unwrap();
        for _ in 0..150 {
            let name = hall::hero_name(&profile).unwrap_or_default();
            println!(
                "{name}: {:?} {:?}",
                ascension(&profile).unwrap(),
                exclusive_level(&profile).unwrap()
            );
            if wanted.contains(&name) {
                profile
                    .color
                    .save(out.join(format!("walk-{name}.png")))
                    .unwrap();
            }
            driver.tap(&profile, hall::NEXT_HERO).unwrap();
            (profile, nameplate) =
                hall::wait_for_affinity_profile(&driver, Some(&nameplate)).unwrap();
        }
    }

    #[test]
    #[ignore = "prints how the stored misread names would be matched"]
    fn stored_strays_match() {
        let db = HeroDatabase::load_quietly();
        for (id, record) in &db.heroes {
            if record.owned && !record.sources.contains_key("list") {
                println!("{id} -> {:?}", closest_hero(&db, &ascii_key(&record.name)));
            }
        }
    }

    #[test]
    fn a_stray_never_catches_later_reads_of_a_known_hero() {
        let mut db = roster(&["Solise", "Soren"]);
        db.record("Solise").owned = true;
        let stray = db.record("soli");
        stray.owned = true;
        stray.progress = Some(Progress {
            ascension: Some("Paragon 4".into()),
            ex_weapon: Some(10),
            ..Progress::default()
        });
        merge_strays(&mut db);
        assert!(!db.heroes.contains_key("soli"));
        assert_eq!(
            db.heroes["solise"].progress.as_ref().unwrap().ex_weapon,
            Some(10)
        );
        let aliases = BTreeMap::new();
        record_for_alias(&mut db, "soli", &aliases).stars = Some(1);
        assert!(!db.heroes.contains_key("soli"));
        assert_eq!(db.heroes["solise"].stars, Some(1));
    }

    #[test]
    fn stray_records_from_misread_names_are_merged() {
        let mut db = roster(&["Lyca", "Odie"]);
        let stray = db.record("lvca");
        stray.owned = true;
        stray.progress = Some(Progress {
            ascension: Some("Paragon 4".into()),
            ..Progress::default()
        });
        stray.stars = Some(3);
        merge_strays(&mut db);
        assert!(!db.heroes.contains_key("lvca"));
        let lyca = &db.heroes["lyca"];
        assert!(lyca.owned);
        assert_eq!(lyca.stars, Some(3));
        assert_eq!(
            lyca.progress.as_ref().unwrap().ascension.as_deref(),
            Some("Paragon 4")
        );
    }

    #[test]
    fn profile_names_find_their_roster_record() {
        let mut db = HeroDatabase::default();
        db.record("Elijah & Lailah");
        record_for(&mut db, "elijahlailah").owned = true;
        assert!(db.heroes["elijah-&-lailah"].owned);
        record_for(&mut db, "newhero").owned = true;
        assert!(
            db.heroes["newhero"].owned,
            "an unknown hero gets its own record"
        );
        assert_eq!(db.heroes.len(), 2);
    }

    #[test]
    fn fresh_heroes_are_skipped() {
        let fresh = with_stats("2026-11-27");
        assert!(needs_scan(&fresh, "2026-11-30", None));
        assert!(!needs_scan(&fresh, "2026-11-30", Some(5)));
        assert!(needs_scan(&fresh, "2026-11-30", Some(2)));
        assert!(needs_scan(&HeroRecord::default(), "2026-11-30", Some(5)));
        assert!(needs_scan(&with_stats("broken"), "2026-11-30", Some(5)));
    }
}
