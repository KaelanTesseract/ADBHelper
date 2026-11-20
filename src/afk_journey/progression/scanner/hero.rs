use super::{client, crop, tr, ActivityEvent, Driver, GameMode, Screen};
use crate::afk_journey::error::ActionError;
use crate::afk_journey::progression::hall;
use crate::afk_journey::text_match::{ascii_key, ascii_key_with};
use crate::afk_journey::vision;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const TRACKER: &str = "https://afkj-tracker.vercel.app/data/";

fn reference(name: &str) -> Result<Value, ActionError> {
    client()
        .get(format!("{TRACKER}{name}"))
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(2 * 1024 * 1024)
                .read_json()
        })
        .map_err(|error| format!("AFKJ Tracker · {error}"))
        .map_err(ActionError::from)
}

fn ascension_label(label: &str) -> Option<&'static str> {
    let compact = ascii_key_with(label, &['+']);
    Some(match compact.as_str() {
        "elite" => "Elite",
        "elite+" => "Elite+",
        "epic" => "Epic",
        "epic+" => "Epic+",
        "legendary" => "Legendary",
        "legendary+" => "Legendary+",
        "mythic" => "Mythic",
        "mythic+" => "Mythic+",
        "supreme" => "Supreme",
        "supreme+" | "sypreme+" | "sureme+" => "Supreme+",
        "paragon1" => "Paragon 1",
        "paragon2" => "Paragon 2",
        "paragon3" => "Paragon 3",
        "paragon4" | "crown" => "Paragon 4",
        _ => return None,
    })
}

fn ascension(screen: &Screen) -> Result<Option<&'static str>, ActionError> {
    for (asset, label) in [
        ("progression/scan_crown.png", "Paragon 4"),
        ("progression/scan_paragon_three.png", "Paragon 3"),
    ] {
        if screen.template(asset, 89..130, 18..61, 0.88)?.is_some() {
            return Ok(Some(label));
        }
    }
    // A numbered Paragon badge can still be upgraded. The blue gem badge
    // on a maxed profile replaces the number when Ascend is unavailable.
    if screen
        .template("progression/scan_paragon_badge.png", 89..131, 18..61, 0.9)?
        .is_some()
        && screen
            .line(205..510, 1735..1850, |label| label.contains("ascend"))
            .is_none()
    {
        return Ok(Some("Paragon 4"));
    }
    let mut labels = BTreeSet::new();
    for line in crop(screen, 9, 20, 124, 46)? {
        labels.extend(ascension_label(&line.text));
    }
    for line in &screen.lines {
        if line.x < 35.0 && line.x + line.width <= 142.0 && (16.0..65.0).contains(&line.y) {
            labels.extend(ascension_label(&line.text));
        }
    }
    Ok((labels.len() == 1).then(|| *labels.first().unwrap()))
}

fn weapon_level(label: &str) -> Option<u32> {
    let digits = label.trim().strip_prefix('+')?.trim();
    digits.parse().ok().filter(|level| *level <= 40)
}

fn exclusive_level(screen: &Screen) -> Result<Option<u32>, ActionError> {
    // Only the weapon badge occupies this strip. The surrounding artwork,
    // upgrade arrow, and the neighboring level label can confuse OCR.
    let mut badge = image::imageops::crop_imm(&screen.gray, 82, 1514, 69, 31).to_image();
    image::imageops::invert(&mut badge);
    let mut padded = image::GrayImage::from_pixel(101, 63, image::Luma([255]));
    image::imageops::replace(&mut padded, &badge, 16, 16);
    let padded = image::imageops::resize(
        &padded,
        padded.width() * 3,
        padded.height() * 3,
        image::imageops::FilterType::CatmullRom,
    );
    if let Some(level) = super::super::ocr::recognize(&padded)?
        .first()
        .and_then(|line| weapon_level(&line.text))
    {
        return Ok(Some(level));
    }
    let mut values = BTreeSet::new();
    for line in crop(screen, 11, 1470, 163, 95)?
        .iter()
        .chain(crop(screen, 0, 1470, 215, 155)?.iter())
        .chain(screen.lines.iter().filter(|line| {
            let (x, y) = line.center();
            (25..200).contains(&x) && (1490..1610).contains(&y)
        }))
    {
        values.extend(weapon_level(&line.text));
    }
    Ok((values.len() == 1).then(|| *values.first().unwrap()))
}

pub(super) fn scan(
    driver: &Driver<'_>,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
    save: &mut impl FnMut(&str, &Value) -> Result<(), ActionError>,
) -> Result<u32, ActionError> {
    let mut backup = reference("heroes-template.json")?;
    let aliases: BTreeMap<String, String> = reference("hero_synonyms.json")
        .ok()
        .and_then(|json| serde_json::from_value(json).ok())
        .unwrap_or_default();
    let heroes = backup
        .get_mut("heroes")
        .and_then(Value::as_array_mut)
        .filter(|heroes| !heroes.is_empty())
        .ok_or("Invalid AFKJ Tracker hero template")?;
    let canonical: BTreeMap<_, _> = heroes
        .iter()
        .enumerate()
        .filter_map(|(index, hero)| Some((ascii_key(hero.get("name")?.as_str()?), index)))
        .collect();
    let aliases: BTreeMap<_, _> = aliases
        .iter()
        .filter_map(|(alias, name)| Some((ascii_key(alias), *canonical.get(&ascii_key(name))?)))
        .collect();
    let mut visited = BTreeSet::new();
    let mut count = 0;
    let mut unresolved = 0;
    let mut finished = false;
    let result = (|| {
        hall::open_first_hero(driver, log)?;
        let (mut profile, mut nameplate) = hall::wait_for_affinity_profile(driver, None)?;
        let first = nameplate.clone();
        for position in 0..heroes.len() + 30 {
            vision::ensure_not_cancelled(driver.cancelled)?;
            let name = hall::hero_name(&profile).unwrap_or_default();
            if matches!(name.as_str(), "chippy" | "hammie")
                || profile
                    .template("dailies/resonating_hall/chippy.png", 15..590, 20..330, 0.88)?
                    .is_some()
                || (position > 0 && first.matches(&nameplate))
            {
                finished = true;
                break;
            }
            if let Some(&index) = canonical.get(&name).or_else(|| aliases.get(&name)) {
                if !visited.insert(index) {
                    if visited.len() == heroes.len() {
                        finished = true;
                        break;
                    }
                    unresolved += 1;
                } else if let Some(ascension) = ascension(&profile)? {
                    let hero = &mut heroes[index];
                    hero["currentAscension"] = ascension.into();
                    let has_weapon = matches!(ascension, "Mythic+" | "Supreme" | "Supreme+")
                        || ascension.starts_with("Paragon");
                    hero["exWeaponUnlocked"] = has_weapon.into();
                    if has_weapon {
                        if let Some(level) = exclusive_level(&profile)? {
                            hero["currentExWeaponLevel"] = level.into();
                        } else {
                            unresolved += 1;
                        }
                    }
                    count += 1;
                    report(ActivityEvent::Collected(GameMode::HeroScan, 1));
                } else {
                    unresolved += 1;
                }
            } else {
                unresolved += 1;
            }
            driver.tap(&profile, hall::NEXT_HERO)?;
            (profile, nameplate) = hall::wait_for_affinity_profile(driver, Some(&nameplate))?;
        }
        driver.back()
    })();
    if count > 0 {
        let complete = result.is_ok() && finished && unresolved == 0;
        if complete {
            save("afkj_tracker_backup.json", &backup)?;
        } else {
            save("afkj_tracker_partial.json", &backup)?;
        }
    }
    result?;
    if !finished || unresolved > 0 {
        return Err(tr!(
            "scanner-hero-partial-roster-scan-unresolved-profiles-or",
            unresolved = unresolved
        )
        .into());
    }
    if count == 0 {
        return Err(tr!("scanner-hero-no-heroes-could-be-exported").into());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::afk_journey::progression::captured;

    #[test]
    #[ignore = "uses an English hero profile capture"]
    fn tracker_profile_capture() {
        for (name, level) in [
            ("hero_ready", 25),
            ("lily_may_reported", 25),
            ("affinity_fix_sinbad_detail", 25),
            ("scanner_profile_kruger", 10),
            ("scanner_hero_live_progress", 5),
        ] {
            let screen = captured(name);
            println!(
                "{name}: {:?} {:?} {:?}",
                hall::hero_name(&screen),
                ascension(&screen),
                exclusive_level(&screen)
            );
            assert!(ascension(&screen).unwrap().is_some(), "{name}");
            assert_eq!(exclusive_level(&screen).unwrap(), Some(level), "{name}");
        }
    }
}
