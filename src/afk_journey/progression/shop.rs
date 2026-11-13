use super::{common, key, text, Driver, Point, Progress, Screen};
use crate::{
    activity::ActivityEvent,
    afk_journey::{
        daily_extras::{amount, exact_amount},
        ocr, phase_rewards,
    },
    routine::EmporiumSettings,
};
use image::{GrayImage, Luma};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Store {
    Guild,
    Friendship,
    Dream,
}

#[derive(Default)]
pub(super) struct State {
    spent: std::collections::BTreeMap<Store, u32>,
    slots: std::collections::BTreeSet<(Store, u32)>,
}

impl Store {
    fn tab(self) -> (&'static str, u32) {
        match self {
            Self::Guild => ("guild", 710),
            Self::Friendship => ("friendship", 1475),
            Self::Dream => ("dream", 1090),
        }
    }

    fn currency(self) -> (&'static str, &'static str) {
        match self {
            Self::Guild => ("progression/diamond.png", "diamond"),
            Self::Friendship => ("progression/pal_coin.png", "palcoin"),
            Self::Dream => ("progression/dream_fragment.png", "dreamfragment"),
        }
    }

    fn budget(self, settings: EmporiumSettings) -> u32 {
        match self {
            Self::Guild => settings.diamonds,
            Self::Friendship => settings.pal_coins,
            Self::Dream => settings.dream_fragments,
        }
        .min(100_000)
    }

    fn wallet(self, screen: &Screen, offer: bool) -> Result<Option<u32>, String> {
        let region = if self == Self::Guild && !offer {
            // Keep the wallet's comma and digits clear of the surrounding banner.
            common::Rect {
                x: 565,
                y: 21,
                width: 160,
                height: 34,
            }
        } else {
            common::Rect {
                x: 862,
                y: 13,
                width: 155,
                height: 53,
            }
        };
        amount(&screen.gray, region)
    }
}

fn shop_screen(screen: &Screen) -> bool {
    [Store::Guild, Store::Dream, Store::Friendship]
        .into_iter()
        .all(|store| {
            let (name, y) = store.tab();
            screen.point(38..167, y..y + 67, name).is_some()
        })
}

fn selected(screen: &Screen, store: Store) -> bool {
    let [r, g, b] = screen.color.get_pixel(19, store.tab().1).0;
    shop_screen(screen) && r > 210 && g > 205 && b > 175
}

fn open_house(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let is_house = |screen: &Screen| {
        screen.point(105..530, 490..615, "nobletavern").is_some()
            && screen.point(70..450, 920..1090, "emporium").is_some()
    };
    let screen = driver.read()?;
    if is_house(&screen) {
        return Ok(screen);
    }
    driver.overview_tab(310, log)?;
    driver.wait_for(is_house)?.ok_or_else(|| {
        text(
            "Could not verify the Mystical House.",
            "Das Mystische Haus konnte nicht eindeutig erkannt werden.",
        )
        .into()
    })
}

fn shop_entry(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let screen = driver.read()?;
    if shop_screen(&screen) {
        return Ok(screen);
    }
    let house = open_house(driver, log)?;
    let emporium = house
        .point(70..450, 920..1090, "emporium")
        .expect("verified house");
    driver.tap(&house, emporium)?;
    driver.wait_for(shop_screen)?.ok_or_else(|| {
        text(
            "Could not verify Emporium.",
            "Das Emporium konnte nicht eindeutig erkannt werden.",
        )
        .into()
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Product {
    InviteLetter,
    Affinity,
    BoundEssence(String),
}

impl Product {
    fn key(&self) -> &str {
        match self {
            Self::InviteLetter => "inviteletter",
            Self::Affinity => "affinity",
            Self::BoundEssence(hero) => hero,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Offer {
    product: Product,
    price: u32,
    balance: u32,
    button: Point,
}

impl Offer {
    fn fits_budget(&self, budget: u32) -> bool {
        self.price <= budget && self.price <= self.balance
    }
}

fn has_discount(screen: &Screen) -> bool {
    (1720..1780)
        .step_by(2)
        .flat_map(|y| (976..1067).step_by(2).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b] = screen.color.get_pixel(x, y).0;
            r > 220 && (82..183).contains(&g) && b < 95
        })
        .count()
        > 110
}

fn purchased_hero(screen: &Screen) -> Option<String> {
    // The character is printed in the item description even if OCR splits
    // the decorative initial from the Temporal Essence heading.
    let line = screen.line(170..890, 1175..1235, |value| {
        value.starts_with("usedtolevelup") && value.contains("exclusiveequipmentfor")
    })?;
    let compact = key(&line.text);
    let (_, hero) = compact.split_once("exclusiveequipmentfor")?;
    (hero.len() >= 3 && hero.chars().all(|c| c.is_ascii_alphabetic())).then(|| hero.to_owned())
}

fn green_amount(
    screen: &Screen,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Option<u32>, String> {
    // Discount offers show the crossed-out old price next to the mint-green
    // current price. Color isolation prevents OCR from combining both.
    let mut agreed = None;
    for threshold in [179, 198] {
        let mut patch = GrayImage::from_pixel(width + 24, height + 24, Luma([255]));
        for dy in 0..height {
            for dx in 0..width {
                let [r, g, b] = screen.color.get_pixel(x + dx, y + dy).0;
                if g >= threshold && g.saturating_sub(r) >= 30 && b >= 115 {
                    patch.put_pixel(dx + 12, dy + 12, Luma([0]));
                }
            }
        }
        let patch = image::imageops::resize(
            &patch,
            patch.width() * 3,
            patch.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = ocr::recognize(&patch)?;
        let current = (lines.len() == 1)
            .then(|| exact_amount(&lines[0].text))
            .flatten();
        if current.is_none() || agreed.is_some_and(|previous| current != Some(previous)) {
            return Ok(None);
        }
        agreed = current;
    }
    Ok(agreed)
}

fn read_offer(
    screen: &Screen,
    store: Store,
    full_price_affinity: bool,
) -> Result<Option<Offer>, String> {
    let Some(button) = screen
        .line(470..795, 1740..1820, |value| value == "purchase")
        .filter(|line| phase_rewards::green_control(&screen.color, line))
    else {
        return Ok(None);
    };
    let discounted = has_discount(screen);
    let product = match store {
        Store::Guild
            if discounted
                && screen.point(145..685, 900..1030, "inviteletter").is_some()
                && screen
                    .line(190..900, 1170..1240, |value| {
                        value.contains("recruitallheroes")
                    })
                    .is_some() =>
        {
            Product::InviteLetter
        }
        Store::Friendship
            if (discounted || full_price_affinity)
                && screen
                    .line(240..1000, 1160..1430, |value| {
                        value.contains("enhancetheiraffinity")
                    })
                    .is_some() =>
        {
            Product::Affinity
        }
        Store::Dream => {
            let Some(hero) = purchased_hero(screen) else {
                return Ok(None);
            };
            Product::BoundEssence(hero)
        }
        _ => return Ok(None),
    };
    let (currency, _) = store.currency();
    let Some((icon, _)) = screen.template(currency, 525..665, 1815..1870, 0.91)? else {
        return Ok(None);
    };
    let Some(price) = green_amount(screen, icon + 14, 1819, 105, 47)?
        .filter(|price| (1..=100_000).contains(price))
    else {
        return Ok(None);
    };
    let Some(balance) = store.wallet(screen, true)? else {
        return Ok(None);
    };
    Ok(Some(Offer {
        product,
        price,
        balance,
        button: button.center(),
    }))
}

fn sold_out(label: &str) -> bool {
    matches!(
        label,
        "soldout" | "sold0ut" | "soldut" | "soldput" | "solput"
    )
}

fn product_slot(
    screen: &Screen,
    store: Store,
    x: u32,
    settings: EmporiumSettings,
) -> Result<Option<Point>, String> {
    let (y, templates): (_, &[_]) = match store {
        Store::Guild => (742..1045, &["dailies/emporium/invite_letter.png"]),
        Store::Friendship if settings.full_price_affinity => (
            955..1032,
            &[
                "dailies/emporium/discount_affinity.png",
                "dailies/emporium/other_affinity.png",
            ],
        ),
        Store::Friendship => (955..1032, &["dailies/emporium/discount_affinity.png"]),
        Store::Dream => (650..925, &["dailies/emporium/bound_essence.png"]),
    };
    if screen.line(x - 72..x + 72, y.clone(), sold_out).is_some() {
        return Ok(None);
    }
    for template in templates {
        if screen
            .template(template, x - 79..x + 79, y.clone(), 0.82)?
            .is_some()
        {
            let row = if store == Store::Dream { 770 } else { 912 };
            return Ok(Some((x, row)));
        }
    }
    Ok(None)
}

fn confirmed_charge(before: u32, after: u32, price: u32) -> bool {
    price > 0 && before.checked_sub(after) == Some(price)
}

fn purchase_charged(screen: &Screen, store: Store, offer: &Offer) -> Result<bool, String> {
    let store_open = selected(screen, store);
    let offer_open = screen.point(460..810, 1740..1820, "purchase").is_some();
    Ok((store_open || offer_open)
        && store
            .wallet(screen, !store_open)?
            .is_some_and(|after| confirmed_charge(offer.balance, after, offer.price)))
}

fn confirming_purchase(screen: &Screen, store: Store, offer: &Offer) -> Option<Point> {
    let (_, currency) = store.currency();
    let body = screen
        .lines
        .iter()
        .filter(|line| (620..1330).contains(&line.center().1))
        .map(|line| key(&line.text))
        .collect::<Vec<_>>()
        .join(" ");
    let amount_seen = screen
        .lines
        .iter()
        .filter(|line| (620..1330).contains(&line.center().1))
        .flat_map(|line| line.text.split(|c: char| !c.is_ascii_digit() && c != ','))
        .any(|word| exact_amount(word) == Some(offer.price));
    if !body.contains("purchase")
        || !body.contains(currency)
        || !body.contains(offer.product.key())
        || !amount_seen
    {
        return None;
    }
    screen.point(520..930, 1050..1640, "confirm")
}

fn commit_purchase(
    driver: &Driver<'_>,
    detail: &Screen,
    store: Store,
    slot: u32,
    offer: &Offer,
    progress: &mut Progress<'_>,
) -> Result<Screen, String> {
    progress.state.pending = true;
    driver.tap(detail, offer.button)?;
    let mut confirmed_dialog = false;
    for _ in 0..12 {
        let current = driver.read()?;
        if let Some(close) = current.receipt_close() {
            driver.tap(&current, close)?;
            continue;
        }
        if purchase_charged(&current, store, offer)? {
            if let Product::BoundEssence(hero) = &offer.product {
                progress.state.purchased_essences.push(hero.clone());
            }
            *progress.state.shop.spent.entry(store).or_default() += offer.price;
            progress.state.shop.slots.insert((store, slot));
            progress.completed();
            if selected(&current, store) {
                return Ok(current);
            }
            driver.back()?;
            return driver
                .wait_for(|screen| selected(screen, store))?
                .ok_or_else(|| {
                    text(
                        "Could not return to the shop.",
                        "Der Shop konnte nicht wieder geöffnet werden.",
                    )
                    .into()
                });
        }
        if !confirmed_dialog {
            if let Some(confirm) = confirming_purchase(&current, store, offer) {
                driver.tap(&current, confirm)?;
                confirmed_dialog = true;
                continue;
            }
        }
        common::settle_cancellable(driver.cancelled)?;
    }
    Err(text(
        "The purchase charge could not be verified.",
        "Die Abbuchung für den Einkauf konnte nicht bestätigt werden.",
    )
    .into())
}

pub(super) fn purchase(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    settings: EmporiumSettings,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    if [
        settings.diamonds,
        settings.pal_coins,
        settings.dream_fragments,
    ] == [0; 3]
    {
        progress.entered();
        return Ok(());
    }
    if progress.count() == 0
        && crate::afk_journey::menu_rewards::emporium_already_purchased(
            driver.serial,
            driver.cancelled,
            log,
        )?
    {
        progress.entered();
        log(text(
            "Emporium purchases were already made today.",
            "Heute wurde bereits im Emporium eingekauft.",
        )
        .into());
        return Ok(());
    }
    let mut screen = shop_entry(driver, log)?;
    progress.entered();
    for store in [Store::Guild, Store::Friendship, Store::Dream] {
        let spent = progress
            .state
            .shop
            .spent
            .get(&store)
            .copied()
            .unwrap_or_default();
        let mut remaining = store.budget(settings).saturating_sub(spent);
        if remaining == 0 {
            continue;
        }
        if store == Store::Guild && spent > 0 {
            continue;
        }
        if !selected(&screen, store) {
            driver.tap(&screen, (100, store.tab().1))?;
            screen = driver
                .wait_for(|frame| selected(frame, store))?
                .ok_or_else(|| {
                    text(
                        "Could not verify the selected store.",
                        "Der ausgewählte Shop konnte nicht eindeutig erkannt werden.",
                    )
                    .to_owned()
                })?;
        }
        for x in [326, 485, 644, 803, 962] {
            if progress.state.shop.slots.contains(&(store, x)) {
                continue;
            }
            let Some(point) = product_slot(&screen, store, x, settings)? else {
                continue;
            };
            driver.tap(&screen, point)?;
            let detail = driver.wait_for(|frame| {
                frame.point(460..810, 1740..1820, "purchase").is_some()
                    || frame.line(450..830, 1740..1860, sold_out).is_some()
            })?;
            if let Some(detail) = detail {
                if let Some(offer) = read_offer(&detail, store, settings.full_price_affinity)? {
                    if offer.fits_budget(remaining) {
                        screen = commit_purchase(driver, &detail, store, x, &offer, progress)?;
                        remaining -= offer.price;
                        if store == Store::Guild || remaining == 0 {
                            break;
                        }
                        continue;
                    }
                    if offer.product == Product::InviteLetter && offer.price > remaining {
                        (progress.report)(ActivityEvent::InviteLetterOverBudget {
                            price: offer.price,
                            budget: remaining,
                        });
                    }
                }
                driver.back()?;
                screen = driver
                    .wait_for(|frame| selected(frame, store))?
                    .ok_or_else(|| {
                        text(
                            "Could not close the offer details.",
                            "Die Angebotsdetails konnten nicht geschlossen werden.",
                        )
                        .to_owned()
                    })?;
            } else {
                screen = driver
                    .wait_for(|frame| selected(frame, store))?
                    .ok_or_else(|| {
                        text(
                            "Could not verify the offer details.",
                            "Die Angebotsdetails konnten nicht eindeutig erkannt werden.",
                        )
                        .to_owned()
                    })?;
            }
        }
    }
    Ok(())
}

fn tavern(screen: &Screen) -> bool {
    screen
        .line(480..723, 1830..1920, |value| {
            value.contains("allhero") || value.contains("allihero") || value.contains("alliero")
        })
        .is_some()
        && screen
            .line(700..1020, 1410..1505, |value| {
                value.contains("hero") || value.contains("prizepool")
            })
            .is_some()
}

fn recruit_button(screen: &Screen) -> Result<Option<Point>, String> {
    let Some(button) = screen
        .line(85..392, 1510..1605, |value| value == "recruit1")
        .filter(|line| phase_rewards::green_control(&screen.color, line))
    else {
        return Ok(None);
    };
    Ok(screen
        .template("progression/invite_cost.png", 145..310, 1584..1648, 0.91)?
        .map(|_| button.center()))
}

fn letter_balance(screen: &Screen) -> Result<Option<u32>, String> {
    let badge = common::Rect {
        x: 368,
        y: 19,
        width: 120,
        height: 30,
    };
    // Windows OCR often omits a lone digit. Use the actual wallet font for
    // these small balances, then fall back to OCR for multi-digit values.
    let crop = image::imageops::crop_imm(&screen.gray, badge.x, badge.y, badge.width, badge.height);
    if let Some(digit) = wallet_digit(&crop.to_image())? {
        return Ok(Some(digit));
    }
    Ok(amount(&screen.gray, badge)?.filter(|value| *value >= 10))
}

fn glyph(image: &GrayImage) -> Option<(GrayImage, u32)> {
    let white = |pixel: &Luma<u8>| pixel.0[0] >= 205;
    let left = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| white(pixel))
        .map(|(x, _, _)| x)
        .min()?;
    let right = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| white(pixel))
        .map(|(x, _, _)| x)
        .max()?;
    let top = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| white(pixel))
        .map(|(_, y, _)| y)
        .min()?;
    let bottom = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| white(pixel))
        .map(|(_, y, _)| y)
        .max()?;
    let width = right - left + 1;
    let height = bottom - top + 1;
    if width > 22 || height < 20 {
        return None;
    }
    let mask = GrayImage::from_fn(width, height, |x, y| {
        Luma([if white(image.get_pixel(left + x, top + y)) {
            255
        } else {
            0
        }])
    });
    Some((
        image::imageops::resize(&mask, 18, 28, image::imageops::FilterType::Nearest),
        width,
    ))
}

fn wallet_digit(image: &GrayImage) -> Result<Option<u32>, String> {
    let Some((observed, observed_width)) = glyph(image) else {
        return Ok(None);
    };
    let references = image::load_from_memory(include_bytes!(
        "../../../assets/AFKJourney/progression/wallet_digits.png"
    ))
    .map_err(|error| error.to_string())?
    .to_luma8();
    let mut scores = Vec::new();
    for digit in 0..10 {
        let sample = image::imageops::crop_imm(&references, digit * 24, 0, 24, 36).to_image();
        if let Some((sample, width)) = glyph(&sample) {
            let differences = observed
                .pixels()
                .zip(sample.pixels())
                .filter(|(a, b)| a != b)
                .count();
            scores.push((
                differences + observed_width.abs_diff(width) as usize * 6,
                digit,
            ));
        }
    }
    scores.sort_unstable();
    Ok(match scores.as_slice() {
        [(best, digit), (next, _), ..] if *best <= 100 && next - best >= 20 => Some(*digit),
        _ => None,
    })
}

fn open_all_hero(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let mut screen = driver.read()?;
    if !tavern(&screen) {
        let house = open_house(driver, log)?;
        driver.tap(
            &house,
            house
                .point(105..530, 490..615, "nobletavern")
                .expect("verified house"),
        )?;
        screen = driver.wait_for(tavern)?.ok_or_else(|| {
            text(
                "Could not verify Noble Tavern.",
                "Noble Tavern konnte nicht eindeutig erkannt werden.",
            )
            .to_owned()
        })?;
    }
    let tab = screen
        .template(
            "dailies/noble_tavern/all_hero_recruit.png",
            500..730,
            1795..1920,
            0.84,
        )?
        .ok_or_else(|| {
            text(
                "Could not find All-Hero Recruitment.",
                "All-Hero Recruitment konnte nicht gefunden werden.",
            )
            .to_owned()
        })?;
    driver.tap(&screen, (tab.0, tab.1.saturating_sub(57)))?;
    driver
        .wait_for(|frame| tavern(frame) && frame.point(85..392, 1510..1605, "recruit1").is_some())?
        .ok_or_else(|| {
            text(
                "Could not verify the recruitment options.",
                "Die Rekrutierungsoptionen konnten nicht eindeutig erkannt werden.",
            )
            .to_owned()
        })
}

pub(super) fn recruit(
    driver: &Driver<'_>,
    progress: &mut Progress<'_>,
    log: &mut impl FnMut(String),
) -> Result<(), String> {
    if progress.count() >= 1 {
        return Ok(());
    }
    let screen = open_all_hero(driver, log)?;
    progress.entered();
    let Some(button) = recruit_button(&screen)? else {
        return Ok(());
    };
    let Some(before) = letter_balance(&screen)?.filter(|value| *value > 0) else {
        return Ok(());
    };
    progress.state.pending = true;
    driver.tap(&screen, button)?;
    let mut dismissed_max = false;
    for attempt in 0..24 {
        let current = driver.read()?;
        if tavern(&current)
            && letter_balance(&current)?.is_some_and(|after| confirmed_charge(before, after, 1))
        {
            progress.completed();
            return Ok(());
        }
        let max_warning = current.lines.iter().any(|line| {
            let label = key(&line.text);
            (label.contains("max") || label.contains("highest"))
                && (label.contains("hero") || label.contains("ascension"))
        });
        if max_warning && !dismissed_max {
            if let Some(button) = current.template(
                "dailies/noble_tavern/maxed_hero_continue.png",
                420..1010,
                930..1680,
                0.9,
            )? {
                driver.tap(&current, button)?;
                dismissed_max = true;
                continue;
            }
        }
        let result = current.lines.iter().any(|line| {
            let label = key(&line.text);
            label.contains("recruitmentresult")
                || label.contains("recruitagain")
                || label.contains("tapanywheretocontinue")
        });
        if result {
            driver.back()?;
            continue;
        }
        // The result animation can show the in-game back arrow without a
        // readable title. The Invite Letter balance still confirms the pull.
        if attempt >= 4 && !max_warning && !tavern(&current) {
            if let Some(back) = current.template("back.png", 0..230, 1650..1920, 0.91)? {
                driver.tap(&current, back)?;
                continue;
            }
        }
        common::settle_cancellable(driver.cancelled)?;
    }
    Err(text(
        "The Invite Letter consumption could not be confirmed.",
        "Der Verbrauch des Invite Letters konnte nicht bestätigt werden.",
    )
    .into())
}

#[cfg(test)]
pub(super) fn preview(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<(), String> {
    let mut screen = shop_entry(driver, log)?;
    for (store, slot) in [
        (Store::Guild, 326),
        (Store::Friendship, 485),
        (Store::Dream, 485),
    ] {
        if !selected(&screen, store) {
            driver.tap(&screen, (100, store.tab().1))?;
            screen = driver
                .wait_for(|frame| selected(frame, store))?
                .expect("selected store");
        }
        let item =
            product_slot(&screen, store, slot, EmporiumSettings::default())?.expect("offer slot");
        driver.tap(&screen, item)?;
        let detail = driver
            .wait_for(|frame| frame.point(460..810, 1740..1820, "purchase").is_some())?
            .expect("offer");
        let offer = read_offer(&detail, store, false)?.expect("verified offer");
        println!(
            "Offer preview {store:?}: {:?}, price {}",
            offer.product, offer.price
        );
        driver.back()?;
        screen = driver
            .wait_for(|frame| selected(frame, store))?
            .expect("return to store");
    }
    let screen = open_all_hero(driver, log)?;
    assert!(recruit_button(&screen)?.is_some());
    println!("All-Hero Recruitment preview verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::captured;
    use super::*;

    #[test]
    fn purchases_require_an_exact_positive_charge_and_opt_in() {
        for store in [Store::Guild, Store::Friendship, Store::Dream] {
            assert_eq!(store.budget(EmporiumSettings::default()), 0);
        }
        assert!(confirmed_charge(1008, 888, 120));
        assert!(!confirmed_charge(1008, 858, 120));
        assert!(!confirmed_charge(1008, 1008, 0));
        assert!(!confirmed_charge(888, 1008, 120));
    }

    #[test]
    #[ignore = "verifies the reported Invite Letter price against the configured diamond budget"]
    fn captured_invite_letter_budget() {
        let screen = captured("invite_offer_live");
        let offer = read_offer(&screen, Store::Guild, false).unwrap().unwrap();
        assert_eq!(offer.product, Product::InviteLetter);
        assert_eq!(offer.price, 210);
        assert!(!offer.fits_budget(1));
        assert!(!offer.fits_budget(209));
        assert!(offer.fits_budget(210));
    }

    #[test]
    #[ignore = "checks the Invite Letter purchase using before and after emulator captures"]
    fn captured_invite_letter_charge() {
        let offer = read_offer(&captured("invite_offer_live"), Store::Guild, false)
            .unwrap()
            .unwrap();
        assert_eq!((offer.balance, offer.price), (157_976, 210));
        let before = captured("invite_guild_live");
        let after = captured("invite_purchase_sold_live");
        assert_eq!(Store::Guild.wallet(&after, false).unwrap(), Some(157_766));
        assert!(!purchase_charged(&before, Store::Guild, &offer).unwrap());
        assert!(purchase_charged(&after, Store::Guild, &offer).unwrap());
        assert!(
            product_slot(&after, Store::Guild, 326, EmporiumSettings::default())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    #[ignore = "requires Windows OCR and AFK Journey captures"]
    fn captured_shop_views() {
        let settings = EmporiumSettings::default();
        for (name, store, slot) in [
            ("shop_guild", Store::Guild, 326),
            ("shop_friend", Store::Friendship, 485),
            ("shop_dream", Store::Dream, 485),
        ] {
            let screen = captured(name);
            assert!(selected(&screen, store), "{name}");
            assert!(
                product_slot(&screen, store, slot, settings)
                    .unwrap()
                    .is_some(),
                "{name}"
            );
        }
        for (name, store, product, price, balance) in [
            (
                "offer_letter",
                Store::Guild,
                Product::InviteLetter,
                210,
                157040,
            ),
            (
                "offer_affinity",
                Store::Friendship,
                Product::Affinity,
                120,
                1008,
            ),
            (
                "offer_essence",
                Store::Dream,
                Product::BoundEssence("parisa".into()),
                300,
                57872,
            ),
        ] {
            let screen = captured(name);
            let offer = read_offer(&screen, store, false)
                .unwrap()
                .unwrap_or_else(|| panic!("{name}: {:?}", screen.lines));
            assert_eq!(offer.product, product);
            assert_eq!(offer.price, price);
            assert_eq!(offer.balance, balance);
        }
        let all = captured("tavern_all_ready");
        assert!(tavern(&all));
        assert_eq!(letter_balance(&all).unwrap(), Some(8));
        assert!(recruit_button(&all).unwrap().is_some());
        assert!(recruit_button(&captured("tavern_wait")).unwrap().is_none());
    }
}
