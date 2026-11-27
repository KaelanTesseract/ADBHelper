//! Hero portraits from github.com/arnecls/afjk-characters
//! (`site/assets/portraits/<Name>.png`). The pictures are game art without
//! a license, so they are never shipped: each one is downloaded once to the
//! user's portrait folder and used from there.

use super::hero_db::portrait_folder;
use crate::afk_journey::error::ActionError;
use crate::afk_journey::text_match::ascii_key;
use crate::localization::tr;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const LISTING: &str =
    "https://api.github.com/repos/arnecls/afjk-characters/contents/site/assets/portraits";
const RAW: &str =
    "https://raw.githubusercontent.com/arnecls/afjk-characters/main/site/assets/portraits/";

/// Folder of the downloaded portraits, inside the portrait folder.
pub fn web_folder() -> std::io::Result<PathBuf> {
    portrait_folder().map(|folder| folder.join("web"))
}

/// Portrait files whose names are not the hero's name in the collection.
const OTHER_NAMES: [(&str, &str); 1] = [("twins", "elijahlailah")];

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

/// The portrait file of a hero among `files`: its own name, a known other
/// name ("twins"), a short form ("lucy" for Lucy Heartfilia) or a near spelling
/// ("smokeymeerky" for Smokey and Meerky).
fn portrait_file_for(hero_name: &str, files: &[String]) -> Option<String> {
    let key = ascii_key(hero_name);
    let stem = |file: &String| file.trim_end_matches(".png").to_owned();
    if let Some(file) = files.iter().find(|file| stem(file) == key) {
        return Some(file.clone());
    }
    if let Some((other, _)) = OTHER_NAMES.iter().find(|(_, hero)| *hero == key) {
        if let Some(file) = files.iter().find(|file| stem(file) == *other) {
            return Some(file.clone());
        }
    }
    let mut close: Vec<(usize, &String)> = files
        .iter()
        .filter_map(|file| {
            let stem = stem(file);
            let score = if stem.len() >= 4 && key.starts_with(&stem) {
                1
            } else {
                distance(&stem, &key)
            };
            (score <= (key.len().max(stem.len()) / 4).max(1)).then_some((score, file))
        })
        .collect();
    close.sort();
    match close.as_slice() {
        [(best, file), rest @ ..] if rest.first().is_none_or(|(next, _)| next > best) => {
            Some((*file).clone())
        }
        _ => None,
    }
}

/// The downloaded portrait of a hero, if there is one.
pub fn web_portrait(hero_name: &str) -> Option<PathBuf> {
    let folder = web_folder().ok()?;
    let files: Vec<String> = std::fs::read_dir(&folder)
        .ok()?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    portrait_file_for(hero_name, &files).map(|file| folder.join(file))
}

/// File names of the PNG portraits in the GitHub folder listing.
fn portrait_names(listing: &Value) -> Vec<String> {
    listing
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
                .filter_map(|entry| entry.get("name")?.as_str())
                .filter(|name| name.ends_with(".png"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// "Elijah & Lailah.png" is stored as "elijahlailah.png", the hero key.
fn local_name(remote: &str) -> String {
    format!("{}.png", ascii_key(remote.trim_end_matches(".png")))
}

/// The remote portraits that are not stored yet.
fn missing(names: &[String], have: &BTreeSet<String>) -> Vec<String> {
    names
        .iter()
        .filter(|name| !have.contains(&local_name(name)))
        .cloned()
        .collect()
}

fn raw_url(name: &str) -> String {
    let escaped: String = name
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_owned(),
            '&' => "%26".to_owned(),
            '\'' => "%27".to_owned(),
            _ => c.to_string(),
        })
        .collect();
    format!("{RAW}{escaped}")
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    super::scanner::client()
        .get(url)
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(4 * 1024 * 1024)
                .read_to_vec()
        })
        .map_err(|error| error.to_string())
}

/// Downloads every portrait that is not stored yet; returns how many were
/// added. Failures are logged once; stored portraits stay in use.
pub fn fetch_missing(
    log: &mut impl FnMut(String),
    cancelled: &AtomicBool,
) -> Result<usize, ActionError> {
    let folder = web_folder().map_err(|error| error.to_string())?;
    let listing = match download(LISTING).and_then(|bytes| {
        serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())
    }) {
        Ok(listing) => listing,
        Err(error) => {
            log(tr!("hero-db-portraits-unavailable", error = error));
            return Ok(0);
        }
    };
    let have: BTreeSet<String> = std::fs::read_dir(&folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    let wanted = missing(&portrait_names(&listing), &have);
    if wanted.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let mut added = 0;
    let mut failed = 0;
    for name in &wanted {
        crate::afk_journey::vision::ensure_not_cancelled(cancelled)?;
        let saved = download(&raw_url(name)).and_then(|bytes| {
            image::load_from_memory(&bytes)
                .map_err(|error| error.to_string())?
                .save(folder.join(local_name(name)))
                .map_err(|error| error.to_string())
        });
        match saved {
            Ok(()) => added += 1,
            Err(_) => failed += 1,
        }
    }
    if failed > 0 {
        log(tr!(
            "hero-db-portraits-unavailable",
            error = format!("{failed}/{}", wanted.len())
        ));
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_listing_gives_png_names_only() {
        let listing = json!([
            {"name": ".DS_Store", "type": "file"},
            {"name": "Aliceth.png", "type": "file"},
            {"name": "Elijah & Lailah.png", "type": "file"},
            {"name": "notes", "type": "dir"}
        ]);
        assert_eq!(
            portrait_names(&listing),
            ["Aliceth.png", "Elijah & Lailah.png"]
        );
    }

    #[test]
    fn local_files_are_named_by_the_hero_key() {
        assert_eq!(local_name("Elijah & Lailah.png"), "elijahlailah.png");
        assert_eq!(local_name("Aliceth.png"), "aliceth.png");
    }

    #[test]
    fn only_missing_portraits_are_downloaded() {
        let have = ["aliceth.png".to_owned()].into();
        let names = vec!["Aliceth.png".to_owned(), "Thoran.png".to_owned()];
        assert_eq!(missing(&names, &have), ["Thoran.png"]);
    }

    #[test]
    #[ignore = "downloads the portraits once into the user's portrait folder"]
    fn portraits_download() {
        let added = fetch_missing(&mut |line| println!("{line}"), &AtomicBool::new(false)).unwrap();
        let stored = std::fs::read_dir(web_folder().unwrap()).unwrap().count();
        println!("added {added}, stored {stored}");
        assert!(stored >= 130);
    }

    #[test]
    fn portrait_files_with_short_names_find_their_hero() {
        let files: Vec<String> = ["lucy", "natsu", "smokeymeerky", "twins", "lyca", "chippy"]
            .iter()
            .map(|stem| format!("{stem}.png"))
            .collect();
        let find = |hero: &str| portrait_file_for(hero, &files);
        assert_eq!(find("Lucy Heartfilia").as_deref(), Some("lucy.png"));
        assert_eq!(find("Natsu Dragneel").as_deref(), Some("natsu.png"));
        assert_eq!(
            find("Smokey and Meerky").as_deref(),
            Some("smokeymeerky.png")
        );
        assert_eq!(find("Elijah & Lailah").as_deref(), Some("twins.png"));
        assert_eq!(find("Lyca").as_deref(), Some("lyca.png"));
        assert_eq!(find("Tephesia"), None);
    }

    #[test]
    fn names_with_spaces_are_escaped_in_the_address() {
        assert_eq!(
            raw_url("Elijah & Lailah.png"),
            format!("{RAW}Elijah%20%26%20Lailah.png")
        );
    }
}
