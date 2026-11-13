use super::{common, key, ocr, text, Driver, Point, Progress, Screen};
use crate::afk_journey::{daily_extras::amount, BASE_RESOLUTION};
use image::{GrayImage, Luma};

const NEXT_HERO: Point = (995, 1090);

fn hall(screen: &Screen) -> bool {
    screen
        .point(70..405, 790..910, "handsofresonance")
        .is_some()
        && screen
            .point(60..415, 1170..1315, "resonanceheroes")
            .is_some()
}

fn open_hall(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let screen = driver.read()?;
    if hall(&screen) {
        return Ok(screen);
    }
    driver.overview_tab(620, log)?;
    driver.wait_for(hall)?.ok_or_else(|| {
        text(
            "Could not verify the Resonating Hall.",
            "Die Resonanzhalle konnte nicht eindeutig erkannt werden.",
        )
        .into()
    })
}

fn hero_profile(screen: &Screen) -> bool {
    // The Skin button shifts Affinity to the left on some hero profiles.
    let affinity = screen
        .line(680..1070, 75..160, |value| value.starts_with("affin"))
        .is_some()
        || matches!(
            screen.template(
                "progression/hero_affinity_icon.png",
                720..945,
                28..115,
                0.88
            ),
            Ok(Some(_))
        );
    affinity
        && (screen
            .line(610..980, 1310..1530, |value| {
                matches!(value, "seasonlevel" | "resonancelevel")
            })
            .is_some()
            || screen
                .line(300..1000, 1720..1850, |value| {
                    matches!(value, "levelup" | "noneedtolevelup")
                })
                .is_some()
            || matches!(
                screen.template("progression/hero_profile_tab.png", 945..1069, 525..678, 0.9),
                Ok(Some(_))
            ))
}

fn hero_name(screen: &Screen) -> Option<String> {
    if !hero_profile(screen) {
        return None;
    }
    name_from_header(screen).or_else(|| {
        screen
            .lines
            .iter()
            .filter(|line| {
                let (x, y) = line.center();
                (115..630).contains(&x)
                    && (60..154).contains(&y)
                    && line.height > 36.0
                    && line.text.chars().any(char::is_alphabetic)
            })
            .max_by(|a, b| a.height.total_cmp(&b.height))
            .map(|line| key(&line.text))
            .filter(|name| name.len() >= 2)
    })
}

fn name_from_header(screen: &Screen) -> Option<String> {
    // The isolated header avoids split or missing names during hero transitions.
    let name = image::imageops::crop_imm(&screen.gray, 112, 52, 613, 99).to_image();
    // Pale backdrops such as Lily May's require a higher threshold.
    [210, 197].into_iter().find_map(|threshold| {
        let mut patch = GrayImage::from_pixel(name.width() + 32, name.height() + 32, Luma([255]));
        for (x, y, pixel) in name.enumerate_pixels() {
            if pixel.0[0] >= threshold {
                patch.put_pixel(x + 16, y + 16, Luma([0]));
            }
        }
        let patch = image::imageops::resize(
            &patch,
            patch.width() * 3,
            patch.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = ocr::recognize(&patch).ok()?;
        lines
            .iter()
            .filter(|line| line.height >= 108.0 && line.text.chars().any(char::is_alphabetic))
            .max_by(|a, b| a.height.total_cmp(&b.height))
            .map(|line| key(&line.text))
            .filter(|name| name.len() >= 2)
    })
}

fn model_view(screen: &Screen) -> bool {
    screen
        .line(245..915, 1740..1870, |value| {
            value.contains("fingertorotate") || value.contains("fingerstozoom")
        })
        .is_some()
        || matches!(
            screen.template("progression/hero_model_eye.png", 968..1060, 38..101, 0.9),
            Ok(Some(_))
        )
}

#[derive(Clone)]
pub(super) struct HeroNameplate {
    outline: Vec<u8>,
    title: Option<String>,
}

impl HeroNameplate {
    fn read(screen: &Screen) -> Option<Self> {
        if !hero_profile(screen) {
            return None;
        }
        // Compare the outline of the rendered name across frames. Local contrast
        // removes the changing background behind the pale lettering.
        let mut outline = Vec::new();
        let gray = &screen.gray;
        for y in (61..162).step_by(2) {
            for x in (130..725).step_by(2) {
                let light = gray.get_pixel(x, y).0[0];
                let dark = [x - 4, x, x + 4]
                    .into_iter()
                    .flat_map(|px| [y - 4, y, y + 4].map(|py| gray.get_pixel(px, py).0[0]))
                    .min()
                    .unwrap_or(light);
                outline.push(u8::from(light >= 180 && light - dark >= 24));
            }
        }
        let title = screen.lines.iter().find_map(|line| {
            if !(140.0..600.0).contains(&line.x)
                || !(16.0..50.0).contains(&line.y)
                || !(16.0..40.0).contains(&line.height)
            {
                return None;
            }
            let title = key(&line.text);
            (title.len() >= 5
                && title
                    .bytes()
                    .all(|character| character.is_ascii_alphabetic()))
            .then_some(title)
        });
        (outline
            .iter()
            .map(|&pixel| usize::from(pixel))
            .sum::<usize>()
            >= 150)
            .then_some(Self { outline, title })
    }

    fn matches(&self, other: &Self) -> bool {
        let (shared, total) = self.outline.iter().zip(&other.outline).fold(
            (0_u32, 0_u32),
            |(shared, total), (&left, &right)| {
                (
                    shared + u32::from(left & right),
                    total + u32::from(left | right),
                )
            },
        );
        // A matching title corroborates the lettering when animated effects
        // obscure part of the name or make the background brighter.
        let minimum = if self.title.is_some() && self.title == other.title {
            45
        } else {
            72
        };
        shared * 100 >= total * minimum
    }
}

fn interact_for_affinity(
    driver: &Driver<'_>,
    detail: &Screen,
    hero: &HeroNameplate,
    attempted: &mut Vec<HeroNameplate>,
) -> Result<Screen, String> {
    let center = (BASE_RESOLUTION.0 / 2, BASE_RESOLUTION.1 / 2);
    let interaction_error = || {
        text(
            "The hero interaction view could not be verified.",
            "Die Ansicht für die Heldeninteraktion konnte nicht eindeutig erkannt werden.",
        )
        .to_owned()
    };
    driver.tap(detail, center)?;
    let model = driver.wait_for(model_view)?.ok_or_else(interaction_error)?;
    // Reserve the second click before sending input so recovery cannot repeat
    // it if the game disconnects before we see the result.
    common::ensure_not_cancelled(driver.cancelled)?;
    attempted.push(hero.clone());
    driver.tap(&model, center)?;
    let current = driver
        .wait_for(|screen| model_view(screen) || hero_profile(screen))?
        .ok_or_else(interaction_error)?;
    if !model_view(&current) {
        return Ok(current);
    }
    let back = current
        .template("back.png", 20..190, 1720..1880, 0.88)?
        .ok_or_else(interaction_error)?;
    driver.tap(&current, back)?;
    // Back closes the verified model view and returns to the underlying profile.
    // Its controls are sufficient to confirm this transition during animations.
    driver.wait_for(hero_profile)?.ok_or_else(|| {
        text(
            "The hero details could not be restored.",
            "Die Heldenansicht konnte nicht wiederhergestellt werden.",
        )
        .to_owned()
    })
}

fn open_first_hero(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<(), String> {
    let screen = open_hall(driver, log)?;
    driver.tap(&screen, (130, 1040))
}

fn first_hero(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    open_first_hero(driver, log)?;
    wait_for_hero(driver, None)?.ok_or_else(|| {
        text(
            "Could not verify the first hero.",
            "Der erste Held konnte nicht eindeutig erkannt werden.",
        )
        .into()
    })
}

fn next_hero(driver: &Driver<'_>, detail: &Screen, hero: &str) -> Result<Screen, String> {
    driver.tap(detail, NEXT_HERO)?;
    wait_for_hero(driver, Some(hero))?.ok_or_else(|| {
        text(
            "Could not verify the next hero.",
            "Der nächste Held konnte nicht eindeutig erkannt werden.",
        )
        .into()
    })
}

fn wait_for_hero(driver: &Driver<'_>, previous: Option<&str>) -> Result<Option<Screen>, String> {
    let mut observed = None;
    driver.wait_for(|frame| {
        let name = hero_name(frame).filter(|name| Some(name.as_str()) != previous);
        let stable = name.is_some() && name == observed;
        observed = name;
        stable
    })
}

fn wait_for_affinity_profile(
    driver: &Driver<'_>,
    previous: Option<&HeroNameplate>,
) -> Result<(Screen, HeroNameplate), String> {
    let mut observed: Option<HeroNameplate> = None;
    let screen = driver.wait_for(|frame| {
        let name = HeroNameplate::read(frame)
            .filter(|name| !previous.is_some_and(|previous| previous.matches(name)));
        let stable = name
            .as_ref()
            .zip(observed.as_ref())
            .is_some_and(|(name, last)| name.matches(last));
        observed = name;
        stable
    })?;
    screen.zip(observed).ok_or_else(|| {
        text(
            "Could not verify the hero profile for Affinity.",
            "Das Heldenprofil für die Affinität konnte nicht eindeutig erkannt werden.",
        )
        .to_owned()
    })
}

pub(super) fn affinity(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    open_first_hero(driver, log)?;
    let (detail, hero) = wait_for_affinity_profile(driver, None)?;
    collect_affinity_profiles(driver, progress, detail, hero)
}

fn collect_affinity_profiles(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    mut detail: Screen,
    mut hero: HeroNameplate,
) -> Result<(), String> {
    progress.entered();
    let first = hero.clone();
    let mut visited: Vec<HeroNameplate> = Vec::new();
    for _ in 0..175 {
        if visited.iter().any(|visited| visited.matches(&hero)) {
            if first.matches(&hero) {
                return Ok(());
            }
            break;
        }
        visited.push(hero.clone());
        if !progress
            .state
            .affinity_attempted
            .iter()
            .any(|attempted| attempted.matches(&hero))
        {
            detail = interact_for_affinity(
                driver,
                &detail,
                &hero,
                &mut progress.state.affinity_attempted,
            )?;
            progress.completed();
        }
        if detail
            .template("dailies/resonating_hall/chippy.png", 15..590, 20..330, 0.88)?
            .is_some()
        {
            return Ok(());
        }
        let previous = HeroNameplate::read(&detail).unwrap_or(hero);
        driver.tap(&detail, NEXT_HERO)?;
        (detail, hero) = wait_for_affinity_profile(driver, Some(&previous))?;
    }
    Err(text(
        "The hero list did not reach its end.",
        "Das Ende der Heldenliste wurde nicht erreicht.",
    )
    .into())
}

fn resonance_level(screen: &Screen) -> Result<Option<u32>, String> {
    if !hall(screen) {
        return Ok(None);
    }
    amount(
        &screen.gray,
        common::Rect {
            x: 690,
            y: 619,
            width: 69,
            height: 42,
        },
    )
}

fn ready_level_control(driver: &Driver<'_>) -> Result<Option<(Screen, Point, u32)>, String> {
    let mut absent = 0;
    for _ in 0..8 {
        let screen = driver.read()?;
        if hall(&screen) {
            match screen.template(
                "resonating_hall/level_up_all.png",
                400..690,
                1490..1750,
                0.95,
            )? {
                Some(button) => {
                    absent = 0;
                    if let Some(level) = resonance_level(&screen)? {
                        return Ok(Some((screen, button, level)));
                    }
                }
                None => {
                    absent += 1;
                    if absent >= 3 {
                        return Ok(None);
                    }
                }
            }
        } else {
            absent = 0;
        }
        common::settle_cancellable(driver.cancelled)?;
    }
    Err(text(
        "The current Resonance Level could not be verified.",
        "Die aktuelle Resonanzstufe konnte nicht eindeutig erkannt werden.",
    )
    .into())
}

pub(super) fn level_up(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    open_hall(driver, log)?;
    progress.entered();
    while progress.count() < 30 {
        let Some((screen, button, before)) = ready_level_control(driver)? else {
            return Ok(());
        };
        progress.state.pending = true;
        driver.tap(&screen, button)?;
        let mut unchanged = None;
        for _ in 0..8 {
            let screen = driver.read()?;
            if let Some(after) = resonance_level(&screen)? {
                if after == before + 1 {
                    progress.completed();
                    break;
                }
                if after == before && unchanged == Some(before) {
                    progress.state.pending = false;
                    return Ok(());
                }
                unchanged = Some(after);
            }
            common::settle_cancellable(driver.cancelled)?;
        }
        if progress.state.pending {
            return Err(text(
                "The new Resonance Level could not be confirmed.",
                "Die neue Resonanzstufe konnte nicht bestätigt werden.",
            )
            .into());
        }
    }
    Ok(())
}

const CLASSES: [(&str, Point); 6] = [
    ("support", (272, 980)),
    ("mage", (297, 1190)),
    ("warrior", (323, 1400)),
    ("rogue", (815, 980)),
    ("marksman", (770, 1200)),
    ("tank", (756, 1408)),
];

fn equipment_class_button(screen: &Screen, class: &str, (x, y): Point) -> Option<Point> {
    screen
        .template(
            &format!("equipment/{class}.png"),
            x - 105..x + 105,
            y - 115..y + 115,
            0.86,
        )
        .ok()
        .flatten()
}

fn equipment_overview(screen: &Screen) -> bool {
    let icons = CLASSES
        .iter()
        .filter(|(class, center)| equipment_class_button(screen, class, *center).is_some())
        .count();
    icons == CLASSES.len()
        || (icons >= 4
            && screen
                .line(240..830, 1540..1855, |value| {
                    value.contains("averageresonancelevel")
                })
                .is_some())
}

fn equipment_class(screen: &Screen, class: &str) -> bool {
    let class_heading = format!("{class}equipment");
    if screen
        .line(105..650, 1130..1280, |value| {
            value.starts_with(&class_heading)
        })
        .is_some()
        && screen
            .line(105..1005, 1130..1280, |value| {
                value.contains("forginglevel")
            })
            .is_some()
    {
        return true;
    }

    // The full-frame OCR can drop the entire equipment panel heading.
    // Read that row at double size, excluding the title art and item grid.
    let heading = image::imageops::crop_imm(&screen.gray, 28, 1157, 1012, 88).to_image();
    let heading = image::imageops::resize(
        &heading,
        heading.width() * 2,
        heading.height() * 2,
        image::imageops::FilterType::CatmullRom,
    );
    let Ok(lines) = ocr::recognize(&heading) else {
        return false;
    };
    let labels: Vec<_> = lines.iter().map(|line| key(&line.text)).collect();
    labels.iter().any(|label| label.starts_with(&class_heading))
        && labels.iter().any(|label| label.contains("forginglevel"))
}

fn equipment_exhausted(screen: &Screen) -> bool {
    screen
        .line(180..900, 1460..1700, |value| {
            value.contains("noavailableequipment") || value.contains("noequipmenttoreplace")
        })
        .is_some()
}

pub(super) fn equipment(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    let hall = open_hall(driver, log)?;
    let entry = hall
        .template("resonating_hall/equipment.png", 670..1070, 1530..1765, 0.84)?
        .ok_or_else(|| {
            text(
                "Could not locate Equipment.",
                "Ausrüstung konnte nicht gefunden werden.",
            )
            .to_owned()
        })?;
    driver.tap(&hall, entry)?;
    let mut screen = driver.read()?;
    for _ in 0..8 {
        if let Some(close) = screen.receipt_close() {
            driver.tap(&screen, close)?;
        } else if let Some(open) =
            screen.template("equipment/open_all.png", 220..850, 750..1536, 0.9)?
        {
            driver.tap(&screen, open)?;
        } else if equipment_overview(&screen) {
            break;
        }
        common::settle_cancellable(driver.cancelled)?;
        screen = driver.read()?;
    }
    if !equipment_overview(&screen) {
        return Err(text(
            "Could not verify the Equipment overview.",
            "Die Ausrüstungsübersicht konnte nicht eindeutig erkannt werden.",
        )
        .into());
    }
    progress.entered();
    for (class, center) in CLASSES {
        let point = equipment_class_button(&screen, class, center).ok_or_else(|| {
            crate::localization::format_text!(
                "Could not locate the equipment icon for {class}.",
                "Das Ausrüstungssymbol für {class} konnte nicht gefunden werden."
            )
        })?;
        driver.tap(&screen, point)?;
        let detail = driver
            .wait_for(|frame| equipment_class(frame, class))?
            .ok_or_else(|| {
                text(
                    "The equipment class could not be verified.",
                    "Die Ausrüstungsklasse konnte nicht eindeutig erkannt werden.",
                )
                .to_owned()
            })?;
        if let Some(button) =
            detail.template("equipment/quick_equip.png", 140..950, 1390..1740, 0.9)?
        {
            driver.tap(&detail, button)?;
            driver
                .wait_for(|frame| equipment_class(frame, class) && equipment_exhausted(frame))?
                .ok_or_else(|| {
                    text(
                        "Quick Equip could not be confirmed.",
                        "Schnellausrüstung konnte nicht bestätigt werden.",
                    )
                    .to_owned()
                })?;
            progress.completed();
        }
        driver.back()?;
        screen = driver.wait_for(equipment_overview)?.ok_or_else(|| {
            text(
                "Could not return to the Equipment overview.",
                "Die Ausrüstungsübersicht konnte nicht wieder geöffnet werden.",
            )
            .to_owned()
        })?;
    }
    Ok(())
}

fn swap_action(screen: &Screen) -> Result<Option<Point>, String> {
    for path in [
        "resonating_hall/hero_action.png",
        "resonating_hall/action.png",
    ] {
        if let Some(point) = screen.template(path, 150..1050, 1710..1920, 0.88)? {
            return Ok(Some(point));
        }
    }
    Ok(None)
}

fn mentions(screen: &Screen, hero: &str, y: std::ops::Range<u32>) -> bool {
    screen
        .line(60..1035, y, |value| value.contains(hero))
        .is_some()
}

fn find_hero(
    driver: &Driver<'_>,
    target: &str,
    log: &mut impl FnMut(String),
) -> Result<Option<Screen>, String> {
    let mut detail = first_hero(driver, log)?;
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..175 {
        let name = hero_name(&detail).expect("verified hero");
        if name == target {
            return Ok(Some(detail));
        }
        if !visited.insert(name.clone()) || name == "chippy" {
            return Ok(None);
        }
        detail = next_hero(driver, &detail, &name)?;
    }
    Err(text(
        "The hero list did not reach its end.",
        "Das Ende der Heldenliste wurde nicht erreicht.",
    )
    .into())
}

fn exclusive_equipment(screen: &Screen, hero: &str) -> bool {
    screen.point(90..435, 1320..1420, "basicstats").is_some() && mentions(screen, hero, 1030..1240)
}

fn swap_button(screen: &Screen) -> Option<Point> {
    screen
        .line(190..1030, 1380..1900, |value| {
            matches!(
                value,
                "swap" | "essenceswap" | "swapessence" | "exchangeessence"
            )
        })
        .map(|line| line.center())
}

fn swap_confirm(screen: &Screen) -> Option<Point> {
    screen.line(160..960, 450..1510, |value| {
        value.contains("temporalessence")
    })?;
    screen.line(160..960, 420..1510, |value| {
        value.contains("swap") || value.contains("exchange")
    })?;
    screen.point(500..960, 1060..1850, "confirm")
}

#[cfg(test)]
pub(super) fn preview(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<(), String> {
    open_hall(driver, log)?;
    let (_, _, level) = ready_level_control(driver)?.expect("available Level Up All");
    println!("Resonating Hall and Level Up All: {level}");
    let detail = first_hero(driver, log)?;
    let name = hero_name(&detail).expect("verified hero");
    let next = next_hero(driver, &detail, &name)?;
    println!("Hero details: {name}, next {:?}", hero_name(&next));
    let hero = hero_name(&next).expect("verified hero");
    driver.tap(&next, (105, 1600))?;
    assert!(driver
        .wait_for(|frame| exclusive_equipment(frame, &hero))?
        .is_some());
    let hall = open_hall(driver, log)?;
    let entry = hall
        .template("resonating_hall/equipment.png", 670..1070, 1530..1765, 0.84)?
        .expect("equipment entry");
    driver.tap(&hall, entry)?;
    let screen = driver
        .wait_for(equipment_overview)?
        .expect("equipment overview");
    driver.tap(&screen, CLASSES[0].1)?;
    assert!(driver
        .wait_for(|frame| equipment_class(frame, CLASSES[0].0))?
        .is_some());
    println!("Equipment class preview verified");
    Ok(())
}

pub(super) fn swap(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    progress.entered();
    while let Some(hero) = progress.state.purchased_essences.first().cloned() {
        let Some(detail) = find_hero(driver, &hero, log)? else {
            progress.state.purchased_essences.remove(0);
            continue;
        };
        driver.tap(&detail, (105, 1600))?;
        let mut screen = driver
            .wait_for(|frame| exclusive_equipment(frame, &hero))?
            .ok_or_else(|| {
                text(
                    "The hero's Exclusive Equipment could not be verified.",
                    "Die exklusive Ausrüstung des Helden konnte nicht eindeutig erkannt werden.",
                )
                .to_owned()
            })?;
        if swap_button(&screen).is_none() {
            let Some(action) = swap_action(&screen)? else {
                progress.state.purchased_essences.remove(0);
                continue;
            };
            driver.tap(&screen, action)?;
            screen = driver.read()?;
        }
        if swap_confirm(&screen).is_none() {
            if let Some(button) = swap_button(&screen).or(swap_action(&screen)?) {
                driver.tap(&screen, button)?;
            } else {
                progress.state.purchased_essences.remove(0);
                continue;
            }
        }
        let dialog = driver
            .wait_for(|frame| swap_confirm(frame).is_some())?
            .ok_or_else(|| {
                text(
                    "The Essence Swap confirmation could not be verified.",
                    "Der Tauschdialog der Essenz konnte nicht eindeutig erkannt werden.",
                )
                .to_owned()
            })?;
        progress.state.pending = true;
        driver.tap(&dialog, swap_confirm(&dialog).expect("verified swap"))?;
        let result = driver
            .wait_for(|frame| frame.receipt_close().is_some())?
            .ok_or_else(|| {
                text(
                    "The Essence Swap result could not be confirmed.",
                    "Der Tausch der Essenz konnte nicht bestätigt werden.",
                )
                .to_owned()
            })?;
        progress.state.purchased_essences.remove(0);
        progress.completed();
        driver.tap(&result, result.receipt_close().expect("verified receipt"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::captured;
    use super::*;

    #[test]
    #[ignore = "checks the highlighted Season Equipment tiles in emulator captures"]
    fn captured_highlighted_equipment_overview() {
        let mut overview = captured("equipment_new_reported");
        assert!(equipment_overview(&overview));
        overview.lines.clear();
        assert!(equipment_overview(&overview));
        for (class, center) in CLASSES {
            assert!(
                equipment_class_button(&overview, class, center).is_some(),
                "{class}"
            );
        }
        let support = captured("equipment_new_support");
        assert!(!equipment_overview(&support));
        assert!(equipment_class(&support, "support"));
        assert!(!equipment_exhausted(&support));
        let replaced = captured("equipment_after_quick_equip");
        assert!(equipment_class(&replaced, "support"));
        assert!(equipment_exhausted(&replaced));
        assert!(equipment_exhausted(&captured("equip_class")));
        let marksman = captured("equipment_current_class_failure");
        assert!(equipment_class(&marksman, "marksman"));
        assert!(!equipment_class(&marksman, "mage"));
        assert!(marksman
            .template("equipment/quick_equip.png", 140..950, 1390..1740, 0.9)
            .unwrap()
            .is_some());
        for class in ["tank", "rogue", "warrior", "mage", "support"] {
            let detail = captured(&format!("equipment_detail_{class}"));
            assert!(equipment_class(&detail, class), "{class}");
        }
        let mut warrior = captured("equipment_warrior_latest");
        warrior.lines.clear();
        assert!(equipment_class(&warrior, "warrior"));
        assert!(!equipment_class(&warrior, "tank"));
        assert!(!equipment_class(&overview, "warrior"));
        assert!(warrior
            .template("equipment/quick_equip.png", 140..950, 1390..1740, 0.9)
            .unwrap()
            .is_some());
    }

    #[test]
    #[ignore = "equips available class equipment, then reruns on the connected emulator"]
    fn connected_emulator_equip_new_equipment() {
        use super::super::{run, GameMode, RunState};
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let equip = || {
            run(
                &serial,
                GameMode::EquipNewEquipment,
                Default::default(),
                &mut RunState::default(),
                &cancelled,
                &mut |message| println!("{message}"),
                &mut |event| println!("{event:?}"),
            )
            .unwrap()
        };
        println!("Classes with confirmed equipment changes: {}", equip());
        assert_eq!(equip(), 0);
    }

    #[test]
    #[ignore = "verifies Lily May's max-level hero profile with the supplied emulator capture"]
    fn captured_lily_may_profile() {
        let mut frame = captured("lily_may_reported");
        assert_eq!(hero_name(&frame).as_deref(), Some("lilymay"));
        frame.lines.clear();
        assert!(hero_profile(&frame), "profile controls without OCR");
        assert_eq!(hero_name(&frame).as_deref(), Some("lilymay"));
        assert!(HeroNameplate::read(&frame).is_some());
    }

    #[test]
    #[ignore = "compares Lyca's profile before and after the interaction against neighboring profiles"]
    fn captured_lyca_nameplate() {
        let lyca = HeroNameplate::read(&captured("affinity_lyca_idle")).unwrap();
        assert!(lyca.matches(&HeroNameplate::read(&captured("affinity_lyca_after")).unwrap()));
        assert!(lyca.matches(&HeroNameplate::read(&captured("lily_roster_23_failure")).unwrap()));
        assert!(!lyca.matches(&HeroNameplate::read(&captured("affinity_lyca_next")).unwrap()));
        assert!(!lyca.matches(&HeroNameplate::read(&captured("lily_may_reported")).unwrap()));
        assert!(HeroNameplate::read(&captured("affinity_lyca_model")).is_none());
    }

    #[test]
    #[ignore = "compares Cryonaia's profile across animation frames using Windows OCR"]
    fn captured_cryonaia_nameplate() {
        let mut before = HeroNameplate::read(&captured("affinity_cryonaia_idle")).unwrap();
        let after = HeroNameplate::read(&captured("affinity_cryonaia_restored")).unwrap();
        let next = HeroNameplate::read(&captured("affinity_cryonaia_next")).unwrap();
        assert_eq!(before.title.as_deref(), Some("arcticrevenant"));
        assert!(before.matches(&after));
        assert!(!before.matches(&next));
        before.title = None;
        assert!(!before.matches(&after));
    }

    #[test]
    #[ignore = "checks the profile and model controls in Dunlingr's emulator captures"]
    fn captured_dunlingr_affinity_views() {
        for name in ["affinity_dunlingr_reported", "affinity_dunlingr_restored"] {
            let frame = captured(name);
            assert!(hero_profile(&frame), "{name}");
            assert!(!model_view(&frame), "{name}");
        }
        for name in ["affinity_dunlingr_model", "affinity_dunlingr_interacted"] {
            let frame = captured(name);
            assert!(model_view(&frame), "{name}");
            assert!(!hero_profile(&frame), "{name}");
        }
    }

    #[test]
    #[ignore = "runs free affinity from an open hero profile to the end of the roster"]
    fn connected_emulator_affinity_tail() {
        use super::super::{ActivityEvent, GameMode, RunState};
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let driver = Driver {
            serial: &serial,
            cancelled: &cancelled,
        };
        let (detail, hero) = wait_for_affinity_profile(&driver, None).unwrap();
        println!("Starting from hero profile with title {:?}.", hero.title);
        let mut visited = 0;
        let mut progress = Progress {
            mode: GameMode::CollectAffinity,
            state: &mut RunState::default(),
            report: &mut |event| {
                if let ActivityEvent::Collected(GameMode::CollectAffinity, count) = event {
                    visited += count;
                    println!("Heroes visited in the final part of the roster: {visited}");
                }
            },
        };
        collect_affinity_profiles(&driver, &mut progress, detail, hero).unwrap();
        let last = driver.read().unwrap();
        assert!(last
            .template("dailies/resonating_hall/chippy.png", 15..590, 20..330, 0.88)
            .unwrap()
            .is_some());
        assert!(visited >= 2);
    }

    #[test]
    #[ignore = "runs the complete free affinity routine on a connected emulator with Lily May"]
    fn connected_emulator_affinity_routine() {
        use super::super::{run, ActivityEvent, GameMode, RunState};
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let mut state = RunState::default();
        let mut visited = 0;
        let mode = GameMode::CollectAffinity;
        let result = run(
            &serial,
            mode,
            Default::default(),
            &mut state,
            &std::sync::atomic::AtomicBool::new(false),
            &mut |message| println!("{message}"),
            &mut |event| {
                if let ActivityEvent::Collected(GameMode::CollectAffinity, count) = event {
                    visited += count;
                    println!("Heroes visited: {visited}");
                }
            },
        );
        assert_eq!(result.as_ref(), Ok(&visited), "{result:?}");
        for name in ["lily_may_reported", "affinity_lyca_after"] {
            let known = HeroNameplate::read(&captured(name)).unwrap();
            assert!(
                state
                    .affinity_attempted
                    .iter()
                    .any(|hero| hero.matches(&known)),
                "{name}"
            );
        }
        assert!(visited > 22);
    }

    #[test]
    #[ignore = "reads captures of Baelran during the hero transition and after interacting"]
    fn captured_baelran_transition() {
        for frame in [
            "baelran_transition_1",
            "baelran_transition_2",
            "affinity_baelran_restored",
        ] {
            assert_eq!(
                hero_name(&captured(frame)).as_deref(),
                Some("baelran"),
                "{frame}"
            );
        }
        assert!(model_view(&captured("affinity_baelran_interacted")));
    }

    #[test]
    #[ignore = "verifies the skin, model view, and next hero from AFK Journey captures"]
    fn captured_affinity_skin_and_next_hero() {
        let sinbad = captured("affinity_fix_sinbad_detail");
        assert_eq!(hero_name(&sinbad).as_deref(), Some("sinbad"));
        assert!(!model_view(&sinbad));
        assert!(hero_name(&captured("affinity_fix_sinbad_before")).is_none());
        for name in ["affinity_fix_center_one", "affinity_fix_center_two"] {
            let model = captured(name);
            assert!(model_view(&model), "{name}");
            assert!(hero_name(&model).is_none(), "{name}");
            assert!(
                model
                    .template("back.png", 20..190, 1720..1880, 0.88)
                    .unwrap()
                    .is_some(),
                "{name}"
            );
        }
        assert_eq!(
            hero_name(&captured("affinity_fix_detail_after")).as_deref(),
            Some("sinbad")
        );
        assert_eq!(
            hero_name(&captured("affinity_fix_next_stable")).as_deref(),
            Some("elijahlailah")
        );
    }

    #[test]
    #[ignore = "runs the two free affinity clicks on three heroes on a connected emulator"]
    fn connected_emulator_two_center_affinity_clicks() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").expect("set serial");
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let driver = Driver {
            serial: &serial,
            cancelled: &cancelled,
        };
        let mut log = |message| println!("{message}");
        let baelran = find_hero(&driver, "baelran", &mut log)
            .unwrap()
            .expect("Baelran");
        let mut attempted = Vec::new();
        let nameplate = HeroNameplate::read(&baelran).unwrap();
        let baelran = interact_for_affinity(&driver, &baelran, &nameplate, &mut attempted).unwrap();
        let next = next_hero(&driver, &baelran, "baelran").unwrap();
        assert_eq!(hero_name(&next).as_deref(), Some("kruger"));
        println!("Baelran's details were restored and the run advanced to Kruger.");
        let sinbad = find_hero(&driver, "sinbad", &mut log)
            .unwrap()
            .expect("Sinbad");
        let nameplate = HeroNameplate::read(&sinbad).unwrap();
        let sinbad = interact_for_affinity(&driver, &sinbad, &nameplate, &mut attempted).unwrap();
        let next = next_hero(&driver, &sinbad, "sinbad").unwrap();
        let name = hero_name(&next).expect("next hero");
        assert_eq!(name, "elijahlailah");
        let nameplate = HeroNameplate::read(&next).unwrap();
        let detail = interact_for_affinity(&driver, &next, &nameplate, &mut attempted).unwrap();
        assert_eq!(hero_name(&detail).as_deref(), Some(name.as_str()));
        assert_eq!(attempted.len(), 3);
        println!(
            "Both center clicks completed for Sinbad and {name}, with the hero details restored."
        );
    }

    #[test]
    #[ignore = "requires Windows OCR and AFK Journey captures"]
    fn captured_hall_views() {
        let overview = captured("hall");
        assert!(hall(&overview));
        assert_eq!(resonance_level(&overview).unwrap(), Some(809));
        assert!(overview
            .template(
                "resonating_hall/level_up_all.png",
                400..690,
                1490..1750,
                0.95
            )
            .unwrap()
            .is_some());
        assert_eq!(
            hero_name(&captured("hero_ready")).as_deref(),
            Some("baelran")
        );
        assert_eq!(
            hero_name(&captured("affinity_next_observe")).as_deref(),
            Some("kruger")
        );
        assert!(model_view(&captured("affinity_touch_two")));
        assert!(equipment_overview(&captured("equip_overview")));
        assert!(equipment_class(&captured("equip_class"), "support"));
        assert!(!equipment_class(&captured("equip_class"), "mage"));
        assert!(exclusive_equipment(&captured("weapon_final"), "kruger"));
        assert!(!exclusive_equipment(&captured("weapon_final"), "parisa"));
        assert!(swap_confirm(&captured("weapon_final")).is_none());
    }
}
