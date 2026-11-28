//! The hero database scan: every owned hero's progress, current stats and
//! skills into `heroes.json`, plus the full roster and facts.

use super::hero::{ascension, exclusive_level, reference};
use super::{tr, ActivityEvent, Driver, GameMode};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::progression::hero_db::portrait_folder;
use crate::afk_journey::progression::hero_db::{
    today, HeroDatabase, HeroRecord, Progress, Skill, Unrecognized,
};
use crate::afk_journey::progression::hero_identity::{identify, learn_title};
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

/// True when the profile should be read once more: the ascension is
/// missing, or a hero with an EX weapon shows no level. Badges still
/// animate right after the profile opens.
pub(super) fn read_again(ascension: Option<&str>, ex_weapon: Option<u32>) -> bool {
    match ascension {
        None => true,
        Some(read) => has_weapon(read) && ex_weapon.is_none(),
    }
}

/// Ascension and EX weapon level of the open profile, read up to four
/// times while something is missing. `profile` becomes the last screen read.
fn read_progress(
    driver: &Driver<'_>,
    profile: &mut super::Screen,
) -> Result<(Option<String>, Option<u32>), ActionError> {
    let mut found_ascension = None;
    let mut found_level = None;
    for attempt in 0..4 {
        if attempt > 0 {
            vision::settle_cancellable(driver.cancelled)?;
            *profile = driver.read()?;
        }
        if found_ascension.is_none() {
            found_ascension = ascension(profile)?;
        }
        if found_level.is_none() && found_ascension.is_none_or(has_weapon) {
            found_level = exclusive_level(profile)?;
        }
        if !read_again(found_ascension, found_level) {
            break;
        }
    }
    let level = found_level.filter(|_| found_ascension.is_some_and(has_weapon));
    Ok((found_ascension.map(str::to_owned), level))
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

/// What one profile showed, for a hero the scan could not name for certain.
fn reading(
    names: &[String],
    epithet: Option<&str>,
    progress: Progress,
    stars: Option<u32>,
    reason: &str,
) -> Unrecognized {
    Unrecognized {
        name_read: names.first().cloned().unwrap_or_default(),
        epithet: epithet.unwrap_or_default().to_owned(),
        stars,
        progress: Some(progress),
        stats: None,
        portrait: None,
        header: None,
        reason: reason.into(),
    }
}

/// The reading with the profile cut and the header saved, so the user can
/// recognize the hero.
fn with_pictures(
    mut entry: Unrecognized,
    profile: &super::Screen,
    log: &mut impl FnMut(String),
) -> Unrecognized {
    let key = Some(entry.epithet.clone())
        .filter(|title| !title.is_empty())
        .or_else(|| Some(entry.name_read.clone()).filter(|name| !name.is_empty()))
        .unwrap_or_else(|| "hero".into());
    entry.portrait = save_portrait(
        &format!("unrecognized-{key}"),
        &hero_card::portrait(&profile.color),
        log,
    );
    entry.header = save_portrait(
        &format!("unrecognized-{key}-header"),
        &hall::header_picture(profile),
        log,
    );
    entry
}

/// The hero a profile shows (if certain), its name readings and its title.
type Identified = (Option<String>, Vec<String>, Option<String>);

/// The hero the open profile shows, read again on up to two later frames
/// while it is unclear. Returns the hero (if certain) and the readings of
/// the last frame. `profile` becomes the last screen read.
fn identify_profile(
    driver: &Driver<'_>,
    profile: &mut super::Screen,
    db: &HeroDatabase,
    aliases: &BTreeMap<String, String>,
) -> Result<Identified, ActionError> {
    let mut names = hall::name_reads(profile);
    let mut epithet = hall::hero_epithet(profile);
    for attempt in 0..3 {
        if attempt > 0 {
            vision::settle_cancellable(driver.cancelled)?;
            *profile = driver.read()?;
            names = hall::name_reads(profile);
            epithet = hall::hero_epithet(profile);
        }
        if let Some(id) = identify(db, &names, epithet.as_deref(), aliases) {
            return Ok((Some(id), names, epithet));
        }
    }
    Ok((None, names, epithet))
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
    db.move_strays();
    db.merge_epithets(&hero_facts::fetch_titles(log));
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
    // Heroes named in this scan: their record before the scan and what the
    // profile showed, in case a second profile is read as the same hero.
    let mut named: BTreeMap<String, Option<(HeroRecord, Unrecognized)>> = BTreeMap::new();
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
        if !name.is_empty() || hall::hero_epithet(&profile).is_some() {
            let (ascension, ex_weapon) = read_progress(driver, &mut profile)?;
            let (hero, names, epithet) = identify_profile(driver, &mut profile, &db, &aliases)?;
            let weapon = ascension.as_deref().is_some_and(has_weapon);
            // The EX level gives the stars; counting them is only the fallback.
            let stars = ex_weapon
                .and_then(hero_card::stars_for_ex_level)
                .or_else(|| hero_card::count_ex_stars(&profile.color));
            let stars = if weapon { stars } else { None };
            let read = merged_progress(None, ascension.clone(), ex_weapon, &day);
            let fresh = match hero {
                Some(id) if !named.contains_key(&id) => {
                    let before = db.heroes[&id].clone();
                    learn_title(&mut db, &id, epithet.as_deref());
                    let record = db.heroes.get_mut(&id).expect("identified hero exists");
                    // A profile once left unrecognized is now named.
                    let title = ascii_key(&record.epithet);
                    db.unrecognized
                        .retain(|old| old.epithet.is_empty() || ascii_key(&old.epithet) != title);
                    let record = db.heroes.get_mut(&id).expect("identified hero exists");
                    record.owned = true;
                    // Stars under the EX weapon; none before Mythic+.
                    record.stars = if weapon { stars.or(record.stars) } else { None };
                    if record.portrait.is_none()
                        && hero_portraits::web_portrait(&record.name).is_none()
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
                    let fresh = match depth {
                        Depth::Full { max_age_days } => needs_scan(record, &day, max_age_days),
                        Depth::Quick => false,
                    };
                    let shown = reading(&names, epithet.as_deref(), read, stars, "twice");
                    named.insert(id.clone(), Some((before, shown)));
                    fresh.then_some(id)
                }
                Some(id) => {
                    // Two profiles read as one hero: one of them is wrong, so
                    // neither counts and the user decides.
                    log(tr!(
                        "hero-db-read-twice",
                        hero = db.heroes[&id].name.clone()
                    ));
                    if let Some((before, first_seen)) = named.insert(id.clone(), None).flatten() {
                        db.heroes.insert(id, before);
                        db.add_unrecognized(first_seen);
                    }
                    let entry = with_pictures(
                        reading(&names, epithet.as_deref(), read, stars, "twice"),
                        &profile,
                        log,
                    );
                    db.add_unrecognized(entry);
                    None
                }
                None => {
                    log(tr!(
                        "hero-db-not-recognized",
                        name = names.first().cloned().unwrap_or_default(),
                        title = epithet.clone().unwrap_or_default()
                    ));
                    let entry = with_pictures(
                        reading(&names, epithet.as_deref(), read, stars, "unknown"),
                        &profile,
                        log,
                    );
                    db.add_unrecognized(entry);
                    None
                }
            };
            if let Some(id) = fresh {
                let stats = hero_stats::read_stats(driver, &profile)?;
                profile = back_on_profile(driver)?;
                let skills = hero_skills::read_skills(driver, &profile)?;
                profile = back_on_profile(driver)?;
                let record = db.heroes.get_mut(&id).expect("identified hero exists");
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
    if !db.unrecognized.is_empty() {
        log(tr!(
            "hero-db-unrecognized-left",
            count = db.unrecognized.len()
        ));
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
    fn missing_ascension_or_ex_level_is_read_again() {
        assert!(read_again(None, None));
        assert!(
            read_again(Some("Supreme"), None),
            "Supreme has an EX weapon"
        );
        assert!(!read_again(Some("Supreme"), Some(13)));
        assert!(!read_again(Some("Legendary+"), None), "no EX weapon yet");
        assert!(!read_again(Some("Paragon 4"), Some(25)));
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

    #[test]
    #[ignore = "needs the user's heroes.json, the titles online and private profile captures"]
    fn stored_heroes_and_captured_profiles_are_identified() {
        let (mut db, _) = HeroDatabase::load().unwrap();
        merge_tracker_roster(&mut db, &mut |line| println!("{line}"));
        db.move_strays();
        db.merge_epithets(&hero_facts::fetch_titles(&mut |line| println!("{line}")));
        let untitled: Vec<&String> = db
            .heroes
            .values()
            .filter(|hero| hero.epithet.is_empty())
            .map(|hero| &hero.name)
            .collect();
        println!("heroes without title: {untitled:?}");
        println!("unrecognized: {:?}", db.unrecognized);
        let aliases = alias_keys();
        // Every hero's own name and title find it, and its title alone too.
        for (id, hero) in &db.heroes {
            let name = vec![ascii_key(&hero.name)];
            let title = Some(ascii_key(&hero.epithet)).filter(|title| !title.is_empty());
            assert_eq!(
                identify(&db, &name, title.as_deref(), &aliases).as_ref(),
                Some(id),
                "{id}"
            );
            if title.is_some() {
                assert_eq!(
                    identify(&db, &[], title.as_deref(), &aliases).as_ref(),
                    Some(id),
                    "{id}"
                );
            }
        }
        let folder = std::path::PathBuf::from(std::env::var_os("ADBHELPER_TEST_HERO_DB").unwrap());
        for (file, id) in [
            ("profile-2-stars.png", "thane"),
            ("profile-3-stars-orb.png", "marene"),
            ("profile-3-stars.png", "aurora"),
            ("profile-5-stars.png", "aliceth"),
            ("profile-6-stars-rolan.png", "rolan"),
            ("profile-6-stars.png", "alna"),
            ("profile-aneva-supreme-ex13.png", "aneva"),
            ("profile-aneva-walk.png", "aneva"),
            ("profile-athalia-supreme-ex10.png", "athalia"),
            ("profile-phraesto-paragon1.png", "phraesto"),
            ("profile-ravion-paragon1.png", "ravion"),
        ] {
            let profile =
                super::super::Screen::decode(&std::fs::read(folder.join(file)).unwrap()).unwrap();
            let names = hall::name_reads(&profile);
            let title = hall::hero_epithet(&profile);
            assert_eq!(
                identify(&db, &names, title.as_deref(), &aliases).as_deref(),
                Some(id),
                "{file}: {names:?} {title:?}"
            );
            // The title alone, as when the stylized name cannot be read at all.
            assert_eq!(
                identify(&db, &[], title.as_deref(), &aliases).as_deref(),
                Some(id),
                "{file}"
            );
            // A wild misreading of the name does not mislead the title.
            assert_eq!(
                identify(&db, &["lige".into()], title.as_deref(), &aliases).as_deref(),
                Some(id),
                "{file}"
            );
        }
    }
}
