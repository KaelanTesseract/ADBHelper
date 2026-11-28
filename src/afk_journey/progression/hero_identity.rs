//! Which hero a profile shows. The title above the name ("Misty Scorpion")
//! is plain text and every hero has its own, so it decides first; the
//! stylized name, which the OCR misreads in many ways, decides only when no
//! title fits and must then point to exactly one hero. Anything unclear is
//! left for the user instead of being guessed.

use super::hero_db::{is_known, HeroDatabase};
use crate::afk_journey::text_match::ascii_key;
use std::collections::{BTreeMap, BTreeSet};

/// Edit distance between two name keys.
pub(super) fn distance(a: &str, b: &str) -> usize {
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

/// Letter groups the OCR reads for one letter of the stylized hero names
/// ("He" as "fle", "w" as "vu"), replaced before names are compared.
const OCR_SWAPS: [(&str, &str); 5] = [
    ("fl", "h"),
    ("vu", "w"),
    ("vv", "w"),
    ("rn", "m"),
    ("cl", "d"),
];

/// A misread name key with the typical OCR swaps undone.
fn unswapped(key: &str) -> String {
    OCR_SWAPS
        .iter()
        .fold(key.to_owned(), |text, (read, letter)| {
            text.replace(read, letter)
        })
}

/// The known hero whose name is closest to a misread profile name key
/// ("lvca" → Lyca, "natsu" → Natsu Dragneel). Only a clear single best match
/// counts; a name that fits no hero gives `None`.
pub(super) fn closest_hero(db: &HeroDatabase, key: &str) -> Option<String> {
    if key.len() < 3 {
        return None;
    }
    let mut scored: Vec<(usize, &String)> = db
        .heroes
        .iter()
        .filter(|(_, record)| is_known(record))
        .map(|(id, record)| {
            let name = ascii_key(&record.name);
            let score = if key.len() >= 4 && name.starts_with(key) {
                1
            } else {
                distance(key, &name).min(distance(&unswapped(key), &name))
            };
            // About a third of the letters may be misread, rounded up ("rhvse" → Rhys).
            let allowed = name.len().max(key.len()).div_ceil(3).max(1);
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

/// How far a read title may be from the stored one: about one letter in six.
fn title_tolerance(key: &str) -> usize {
    key.len().div_ceil(6).max(1)
}

/// The known hero a name reading points to, and whether it fits exactly
/// (its name, a misread name the user assigned, or a tracker alias) rather
/// than only closely.
fn named(
    db: &HeroDatabase,
    key: &str,
    aliases: &BTreeMap<String, String>,
) -> Option<(String, bool)> {
    let exact = |key: &str| {
        db.heroes
            .iter()
            .find(|(_, record)| {
                is_known(record)
                    && (ascii_key(&record.name) == key
                        || record.read_names.iter().any(|read| read == key))
            })
            .map(|(id, _)| id.clone())
    };
    exact(key)
        .or_else(|| aliases.get(key).and_then(|real| exact(real)))
        .map(|id| (id, true))
        .or_else(|| closest_hero(db, key).map(|id| (id, false)))
}

/// The known hero whose title the reading shows, and whether it fits
/// exactly. A close title counts only when no other hero's is as close.
fn titled(db: &HeroDatabase, epithet: &str) -> Option<(String, bool)> {
    let key = ascii_key(epithet);
    if key.len() < 5 {
        return None;
    }
    let mut scored: Vec<(usize, &String)> = db
        .heroes
        .iter()
        .filter(|(_, record)| is_known(record) && !record.epithet.is_empty())
        .map(|(id, record)| (distance(&key, &ascii_key(&record.epithet)), id))
        .filter(|(score, _)| *score <= title_tolerance(&key))
        .collect();
    scored.sort();
    match scored.as_slice() {
        [(best, id), rest @ ..] if rest.first().is_none_or(|(next, _)| next > best) => {
            Some(((*id).clone(), *best == 0))
        }
        _ => None,
    }
}

/// The hero a profile shows, from its name readings and its title; `None`
/// when that is not certain.
pub(super) fn identify(
    db: &HeroDatabase,
    names: &[String],
    epithet: Option<&str>,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    let reads: Vec<(String, bool)> = names
        .iter()
        .filter_map(|name| named(db, &ascii_key(name), aliases))
        .collect();
    let exact: BTreeSet<&String> = reads
        .iter()
        .filter(|(_, exact)| *exact)
        .map(|(id, _)| id)
        .collect();
    let close: BTreeSet<&String> = reads.iter().map(|(id, _)| id).collect();
    match epithet.and_then(|epithet| titled(db, epithet)) {
        // A title read letter for letter decides, unless the name is read
        // letter for letter as another hero's.
        Some((id, true)) => (exact.is_empty() || exact.contains(&id)).then_some(id),
        // A nearly read title needs a name that does not point elsewhere.
        Some((id, false)) => {
            let agrees = if exact.is_empty() {
                close.is_empty() || close.contains(&id)
            } else {
                exact.len() == 1 && exact.contains(&id)
            };
            agrees.then_some(id)
        }
        None => {
            let candidates = if exact.is_empty() { close } else { exact };
            let [id] = candidates.into_iter().collect::<Vec<_>>()[..] else {
                return None;
            };
            // A title was read that is not this hero's: something is off.
            let stored = ascii_key(&db.heroes[id].epithet);
            let contradicts = epithet.map(ascii_key).is_some_and(|read| {
                read.len() >= 5
                    && !stored.is_empty()
                    && distance(&read, &stored) > title_tolerance(&read)
            });
            (!contradicts).then(|| id.clone())
        }
    }
}

/// Remembers the title read on the hero's profile when no list gave one, so
/// later scans find the hero by it; never a title another hero has.
pub(super) fn learn_title(db: &mut HeroDatabase, id: &str, epithet: Option<&str>) {
    let Some(read) = epithet.map(ascii_key).filter(|read| read.len() >= 5) else {
        return;
    };
    let taken = db
        .heroes
        .iter()
        .any(|(other, record)| other != id && ascii_key(&record.epithet) == read);
    if let Some(record) = db.heroes.get_mut(id) {
        if record.epithet.is_empty() && !taken {
            record.epithet = read;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::afk_journey::progression::hero_db::HeroRecord;

    fn roster(heroes: &[(&str, &str)]) -> HeroDatabase {
        let mut db = HeroDatabase::default();
        for (name, epithet) in heroes {
            let record = db.record(name);
            record.sources.insert("list".into(), "afkj-tracker".into());
            record.epithet = (*epithet).into();
        }
        db
    }

    fn sample() -> HeroDatabase {
        roster(&[
            ("Phraesto", "Misty Scorpion"),
            ("Temesia", "Lightsavior"),
            ("Tephesia", ""),
            ("Lyca", "Forest's Arrow"),
            ("Lucius", "The Lightbringer"),
            ("Lucca", "Bamboo Guardian"),
            ("Hewynn", "Ode to Flowers"),
            ("Natsu Dragneel", "Fire Dragon Slayer Mage"),
            ("Alna", "Frozen Mother"),
        ])
    }

    fn names(reads: &[&str]) -> Vec<String> {
        reads.iter().map(|read| (*read).to_owned()).collect()
    }

    fn id(db: &HeroDatabase, reads: &[&str], title: Option<&str>) -> Option<String> {
        identify(db, &names(reads), title, &BTreeMap::new())
    }

    #[test]
    fn the_title_names_the_hero_whatever_the_name_reads() {
        let db = sample();
        assert_eq!(
            id(&db, &["lige"], Some("mistyscorpion")),
            Some("phraesto".into())
        );
        assert_eq!(id(&db, &[], Some("frozenmother")), Some("alna".into()));
        assert_eq!(
            id(&db, &["aina", "alna"], Some("frozenmother")),
            Some("alna".into())
        );
        // One misread letter in the title still finds the hero.
        assert_eq!(
            id(&db, &["flevutnn"], Some("odetoflovers")),
            Some("hewynn".into())
        );
    }

    #[test]
    fn an_unknown_hero_is_never_guessed() {
        let db = sample();
        // A new hero: neither its title nor its name is known.
        assert_eq!(id(&db, &["lige"], Some("wanderingfox")), None);
        assert_eq!(id(&db, &["lige"], None), None);
        assert_eq!(id(&db, &[], None), None);
    }

    #[test]
    fn readings_that_disagree_give_no_hero() {
        let db = sample();
        // The title says Phraesto, the name says Lyca letter for letter.
        assert_eq!(id(&db, &["lyca"], Some("mistyscorpion")), None);
        // Two names fit two different heroes and no title helps.
        assert_eq!(id(&db, &["lucius", "lucca"], None), None);
        // The name fits Temesia, but the title read is clearly not hers.
        assert_eq!(id(&db, &["temesia"], Some("wanderingfox")), None);
    }

    #[test]
    fn without_a_title_one_clear_name_still_counts() {
        let db = sample();
        assert_eq!(id(&db, &["lvca"], None), Some("lyca".into()));
        assert_eq!(id(&db, &["natsu"], None), Some("natsu-dragneel".into()));
        // Tephesia has no title yet; the one read on her profile is new.
        assert_eq!(
            id(&db, &["tephesia"], Some("palevotary")),
            Some("tephesia".into())
        );
        // A close and an exact reading: the exact one decides.
        assert_eq!(id(&db, &["lucius", "lucus"], None), Some("lucius".into()));
    }

    #[test]
    fn misread_names_find_the_closest_hero() {
        let db = roster(&[
            ("Lyca", ""),
            ("Berial", ""),
            ("Daimon", ""),
            ("Lucius", ""),
            ("Smokey and Meerky", ""),
            ("Solise", ""),
            ("Rhys", ""),
            ("Hewynn", ""),
            ("Temesia", ""),
            ("Tephesia", ""),
        ]);
        for (read, hero) in [
            ("lvca", "lyca"),
            ("beria", "berial"),
            ("smokeymeerky", "smokey-and-meerky"),
            ("soise", "solise"),
            ("soli", "solise"),
            ("rhvse", "rhys"),
            ("flevutnn", "hewynn"),
        ] {
            assert_eq!(closest_hero(&db, read).as_deref(), Some(hero), "{read}");
        }
        assert_eq!(closest_hero(&db, "zzzzzz"), None);
    }

    #[test]
    fn misread_names_the_user_assigned_are_exact() {
        let mut db = sample();
        db.heroes
            .get_mut("tephesia")
            .unwrap()
            .read_names
            .push("lige".into());
        assert_eq!(id(&db, &["lige"], None), Some("tephesia".into()));
    }

    #[test]
    fn records_from_misread_names_never_catch_a_reading() {
        let mut db = sample();
        db.heroes.insert(
            "soli".into(),
            HeroRecord {
                name: "soli".into(),
                owned: true,
                ..HeroRecord::default()
            },
        );
        assert_eq!(id(&db, &["soli"], None), None);
    }

    #[test]
    fn a_learned_title_is_never_one_of_another_hero() {
        let mut db = sample();
        learn_title(&mut db, "tephesia", Some("palevotary"));
        assert_eq!(db.heroes["tephesia"].epithet, "palevotary");
        learn_title(&mut db, "lucius", Some("palevotary"));
        assert_eq!(db.heroes["lucius"].epithet, "The Lightbringer");
        let mut fresh = sample();
        learn_title(&mut fresh, "tephesia", Some("lightsavior"));
        assert_eq!(fresh.heroes["tephesia"].epithet, "", "Temesia's title");
    }
}
