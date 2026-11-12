use super::{common, key, text, Driver, Point, Progress, Screen};
use crate::afk_journey::daily_extras::{amount, exact_amount};

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

fn hero_name(screen: &Screen) -> Option<String> {
    screen.line(810..1070, 75..160, |value| value.starts_with("affin"))?;
    screen
        .line(610..980, 1310..1530, |value| {
            matches!(value, "seasonlevel" | "resonancelevel")
        })
        .or_else(|| {
            screen
                .point(610..960, 1720..1820, "levelup")
                .and_then(|_| screen.line(50..625, 65..160, |_| true))
        })?;
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
}

fn affinity_gauge(screen: &Screen) -> Option<u32> {
    screen.point(145..380, 990..1090, "affinity")?;
    screen
        .lines
        .iter()
        .filter(|line| {
            let (x, y) = line.center();
            (360..810).contains(&x) && (1115..1215).contains(&y)
        })
        .find_map(|line| {
            let current = line.text.split('/').next()?;
            exact_amount(current)
        })
}

fn affinity_panel(screen: &Screen) -> bool {
    screen.point(145..380, 990..1090, "affinity").is_some()
        && screen.point(415..650, 990..1090, "tales").is_some()
}

fn read_affinity(
    driver: &Driver<'_>,
    detail: &Screen,
    hero: &str,
) -> Result<(Screen, Option<u32>), String> {
    driver.tap(detail, (880, 100))?;
    let panel = driver.wait_for(affinity_panel)?.ok_or_else(|| {
        text(
            "The hero's affinity panel could not be verified.",
            "Die Affinitätsansicht des Helden konnte nicht eindeutig erkannt werden.",
        )
        .to_owned()
    })?;
    let gauge = affinity_gauge(&panel);
    driver.tap(&panel, (100, 1795))?;
    let detail = driver
        .wait_for(|frame| hero_name(frame).as_deref() == Some(hero))?
        .ok_or_else(|| {
            text(
                "The hero details could not be restored.",
                "Die Heldenansicht konnte nicht wiederhergestellt werden.",
            )
            .to_owned()
        })?;
    Ok((detail, gauge))
}

fn model_view(screen: &Screen) -> bool {
    screen
        .line(245..915, 1740..1870, |value| {
            value.contains("fingertorotate") || value.contains("fingerstozoom")
        })
        .is_some()
}

fn first_hero(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let screen = open_hall(driver, log)?;
    driver.tap(&screen, (130, 1040))?;
    driver
        .wait_for(|frame| hero_name(frame).is_some())?
        .ok_or_else(|| {
            text(
                "Could not verify the first hero.",
                "Der erste Held konnte nicht eindeutig erkannt werden.",
            )
            .into()
        })
}

fn next_hero(driver: &Driver<'_>, detail: &Screen, hero: &str) -> Result<Screen, String> {
    driver.tap(detail, (995, 1090))?;
    driver
        .wait_for(|frame| hero_name(frame).is_some_and(|name| name != hero))?
        .ok_or_else(|| {
            text(
                "Could not verify the next hero.",
                "Der nächste Held konnte nicht eindeutig erkannt werden.",
            )
            .into()
        })
}

pub(super) fn affinity(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    let mut detail = first_hero(driver, log)?;
    progress.entered();
    let first = hero_name(&detail).expect("verified hero");
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..175 {
        let hero = hero_name(&detail).expect("verified hero");
        if !visited.insert(hero.clone()) {
            if hero == first {
                return Ok(());
            }
            break;
        }
        if !progress.state.affinity_visited.contains(&hero) {
            let (current, before) = read_affinity(driver, &detail, &hero)?;
            detail = current;
            {
                for _ in 0..3 {
                    driver.tap(&detail, (540, 840))?;
                    let model = driver.wait_for(model_view)?.ok_or_else(|| text(
                        "The hero interaction view could not be verified.",
                        "Die Ansicht für die Heldeninteraktion konnte nicht eindeutig erkannt werden.",
                    ).to_owned())?;
                    driver.tap(&model, (540, 840))?;
                    driver.tap(&model, (100, 1795))?;
                    detail = driver
                        .wait_for(|frame| hero_name(frame).as_deref() == Some(&hero))?
                        .ok_or_else(|| {
                            text(
                                "The hero details could not be restored.",
                                "Die Heldenansicht konnte nicht wiederhergestellt werden.",
                            )
                            .to_owned()
                        })?;
                }
                let (current, after) = read_affinity(driver, &detail, &hero)?;
                detail = current;
                if before.zip(after).is_some_and(|(old, new)| new > old) {
                    progress.completed();
                }
            }
            progress.state.affinity_visited.insert(hero.clone());
        }
        if hero == "chippy"
            || detail
                .template("dailies/resonating_hall/chippy.png", 15..590, 20..330, 0.88)?
                .is_some()
        {
            return Ok(());
        }
        detail = next_hero(driver, &detail, &hero)?;
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

fn equipment_overview(screen: &Screen) -> bool {
    screen
        .line(240..830, 1540..1665, |value| {
            value.contains("averageresonancelevel")
        })
        .is_some()
        && CLASSES
            .iter()
            .filter(|(name, (x, y))| {
                let range = if *x < 540 { 25..245 } else { 850..1060 };
                screen
                    .point(range, y.saturating_sub(75)..y + 100, name)
                    .is_some()
            })
            .count()
            >= 4
}

fn equipment_class(screen: &Screen, class: &str) -> bool {
    screen
        .line(105..650, 1130..1280, |value| {
            value.starts_with(class) && value.contains("equipment")
        })
        .is_some()
        && screen
            .line(570..1005, 1130..1280, |value| {
                value.contains("forginglevel")
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
    for (class, point) in CLASSES {
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
            if detail.point(180..900, 1460..1700, "quickequip").is_some() {
                driver.tap(&detail, button)?;
                driver
                    .wait_for(|frame| {
                        equipment_class(frame, class)
                            && frame
                                .line(180..900, 1460..1700, |value| {
                                    value.contains("noavailableequipment")
                                })
                                .is_some()
                    })?
                    .ok_or_else(|| {
                        text(
                            "Quick Equip could not be confirmed.",
                            "Schnellausrüstung konnte nicht bestätigt werden.",
                        )
                        .to_owned()
                    })?;
                progress.completed();
            }
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
    let (detail, gauge) = read_affinity(driver, &detail, &name)?;
    assert!(gauge.is_some(), "affinity gauge");
    let next = next_hero(driver, &detail, &name)?;
    println!(
        "Hero details and affinity: {name} {gauge:?}, next {:?}",
        hero_name(&next)
    );
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
        assert!(affinity_panel(&captured("kruger_affinity_before")));
        assert!(model_view(&captured("affinity_touch_two")));
        assert_eq!(
            affinity_gauge(&captured("kruger_affinity_before")),
            Some(10256)
        );
        assert_eq!(
            affinity_gauge(&captured("kruger_affinity_after")),
            Some(10258)
        );
        assert!(equipment_overview(&captured("equip_overview")));
        assert!(equipment_class(&captured("equip_class"), "support"));
        assert!(!equipment_class(&captured("equip_class"), "mage"));
        assert!(exclusive_equipment(&captured("weapon_final"), "kruger"));
        assert!(!exclusive_equipment(&captured("weapon_final"), "parisa"));
        assert!(swap_confirm(&captured("weapon_final")).is_none());
    }
}
