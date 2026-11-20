use super::{client, common, crop, text, ActivityEvent, Driver, GameMode, GuildSettings, Screen};
use crate::afk_journey::{arena, dream_realm, phase_rewards};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

const DREAM_FILE: &str = "dream_realm_rankings.json";
const SUPREME_FILE: &str = "supreme_arena_rankings.json";
const AFK_FILE: &str = "afk_stages_rankings.json";
const ACTIVENESS_FILE: &str = "guild_activeness.json";

#[derive(Deserialize)]
struct GuildState {
    state: GuildPlayers,
}

#[derive(Deserialize)]
struct GuildPlayers {
    players: Vec<Member>,
}

#[derive(Deserialize)]
struct Member {
    name: String,
    #[serde(default)]
    id: Option<Value>,
}

fn members_url(input: &str) -> Result<ureq::http::Uri, String> {
    let uri: ureq::http::Uri = input.trim().parse().map_err(|_| invalid_endpoint())?;
    let project = uri
        .host()
        .and_then(|host| host.strip_suffix(".supabase.co"));
    if uri.scheme_str() != Some("https")
        || uri.path() != "/rest/v1/guild_states"
        || uri.port().is_some()
        || !project.is_some_and(|name| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        || uri
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(invalid_endpoint());
    }
    Ok(uri)
}

fn invalid_endpoint() -> String {
    text(
        "Use the HTTPS guild_states API URL from AFKJ Guild Manager.",
        "Verwende die HTTPS-URL für guild_states aus AFKJ Guild Manager.",
    )
    .into()
}

/// Splits a pasted Guild Manager URL into the URL without `apikey` and that key.
///
/// The key belongs in the `apikey` header only; a query parameter would also end
/// up in server and proxy logs.
pub(crate) fn split_access(input: &str) -> (String, Option<String>) {
    let input = input.trim();
    let Some((base, query)) = input.split_once('?') else {
        return (input.to_owned(), None);
    };
    let mut key = None;
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| match pair.strip_prefix("apikey=") {
            Some(value) => {
                if key.is_none() && !value.is_empty() {
                    key = Some(value.to_owned());
                }
                false
            }
            None => !pair.is_empty(),
        })
        .collect();
    let url = if kept.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", kept.join("&"))
    };
    (url, key)
}

fn remote_members(input: &str, api_key: &str) -> Result<Vec<Member>, String> {
    let (url, pasted_key) = split_access(input);
    let uri = members_url(&url)?;
    let api_key = Some(api_key.trim())
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .or(pasted_key);
    let agent = client();
    let mut request = agent.get(uri).header("Cache-Control", "no-cache");
    if let Some(api_key) = api_key {
        request = request.header("apikey", api_key);
    }
    let mut rows: Vec<GuildState> = request
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(4 * 1024 * 1024)
                .read_json()
        })
        .map_err(|_| {
            text(
                "The Supabase guild roster could not be read.",
                "Die Supabase-Mitgliederliste konnte nicht gelesen werden.",
            )
            .to_owned()
        })?;
    if rows.len() != 1 {
        return Err(text(
            "The API URL must select exactly one guild state.",
            "Die API-URL muss genau einen Gildenstatus auswählen.",
        )
        .into());
    }
    Ok(rows.remove(0).state.players)
}

fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn identity<'a>(name: &str, roster: &'a [Member]) -> Option<&'a Member> {
    let name = name_key(name);
    if name.is_empty() {
        return None;
    }
    let mut matches = roster
        .iter()
        .filter(|member| name_key(&member.name) == name);
    let member = matches.next()?;
    matches.next().is_none().then_some(member)
}

fn corrected_identity<'a>(names: &[String], roster: &'a [Member]) -> Option<&'a Member> {
    let mut matches = names.iter().filter_map(|name| identity(name, roster));
    if let Some(first) = matches.next() {
        return matches
            .all(|member| std::ptr::eq(member, first))
            .then_some(first);
    }
    let keys: Vec<Vec<char>> = names
        .iter()
        .map(|name| name_key(name).chars().collect())
        .collect();
    let mut matches = roster.iter().filter(|member| {
        let canonical: Vec<_> = name_key(&member.name).chars().collect();
        canonical.len() >= 6 && keys.iter().any(|candidate| one_edit(candidate, &canonical))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn one_edit(a: &[char], b: &[char]) -> bool {
    if a.len() == b.len() {
        return a.iter().zip(b).filter(|(x, y)| x != y).count() == 1;
    }
    let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
    if long.len() != short.len() + 1 {
        return false;
    }
    let first = short
        .iter()
        .zip(long)
        .position(|(x, y)| x != y)
        .unwrap_or(short.len());
    short[first..] == long[first + 1..]
}

fn put_identity(row: &mut Value, member: &Member) {
    row["Name"] = json!(member.name);
    if let Some(id) = &member.id {
        row["Id"] = id.clone();
    }
}

fn digits(label: &str) -> Option<u32> {
    let label = label.trim();
    label
        .bytes()
        .all(|c| c.is_ascii_digit())
        .then(|| label.parse().ok())
        .flatten()
}

fn member_header(screen: &Screen) -> bool {
    let heading = screen
        .line(110..500, 515..590, |s| s.starts_with("guildmember"))
        .is_some();
    let score = screen
        .line(715..940, 515..590, |s| s == "activeness")
        .is_some();
    let guild_id = screen
        .line(770..1080, 0..85, |s| s.starts_with("guildid"))
        .is_some();
    (heading && score) || (guild_id && (heading || score))
}

fn member_count(screen: &Screen) -> Option<usize> {
    let line = screen.line(110..500, 515..590, |s| s.starts_with("guildmember"))?;
    let (_, count) = line.text.split_once('(')?;
    let (count, _) = count.split_once('/')?;
    digits(count)
        .map(|value| value as usize)
        .filter(|n| (1..=100).contains(n))
}

fn clean_name(label: &str) -> Option<String> {
    let stripped = label.trim_matches(|c: char| !c.is_alphanumeric());
    let name = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    let key = name_key(&name);
    (key.len() >= 2
        && name.len() <= 90
        && !matches!(
            key.as_str(),
            "guildmember" | "friends" | "warband" | "activeness" | "descending" | "ascending"
        )
        && !name.contains("(Base"))
    .then_some(name)
}

fn read_members(screen: &Screen) -> Result<Vec<(String, u32)>, String> {
    use image::{GrayImage, Luma};

    let scores = crop(screen, 752, 640, 126, 960)?;
    let mut rows = Vec::new();
    for score in scores {
        let y = score.center().1;
        let Some(value) = digits(&score.text).filter(|&n| n <= 9999) else {
            continue;
        };
        if !(715..1600).contains(&y) {
            continue;
        }
        // The names share a text color. Mask away the class and gender icons
        // on either side, and keep the sticky table header out of the crop.
        let mut name = GrayImage::from_pixel(500, 94, Luma([255]));
        let top = (y - 123).max(603);
        for py in top..y - 61 {
            for px in 235..703 {
                if name_ink(screen, px, py) {
                    name.put_pixel(px - 235 + 16, py - top + 16, Luma([0]));
                }
            }
        }
        let name = image::imageops::resize(
            &name,
            name.width() * 3,
            name.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        if let Some(name) = super::super::ocr::recognize(&name)?
            .iter()
            .filter(|line| line.height >= 60.0)
            .max_by(|a, b| a.height.total_cmp(&b.height))
            .and_then(|line| clean_name(&line.text))
        {
            rows.push((name, value));
        }
    }
    Ok(rows)
}

fn open_members(driver: &Driver<'_>, log: &mut impl FnMut(String)) -> Result<Screen, String> {
    let screen = driver.read()?;
    if member_header(&screen) {
        return Ok(screen);
    }
    driver.overview_tab(765, log)?;
    for _ in 0..7 {
        let screen = driver.read()?;
        if member_header(&screen) {
            return Ok(screen);
        }
        if let Some(close) = screen
            .receipt_close()
            .or_else(|| screen.point(295..785, 1740..1890, "taptoclose"))
        {
            driver.tap(&screen, close)?;
            continue;
        }
        if let Some(entry) = screen.point(480..1070, 120..990, "members") {
            driver.tap(&screen, entry)?;
            return driver.wait_for(member_header)?.ok_or_else(member_error);
        }
        swipe(driver, &screen, (700, 600), (300, 1200))?;
    }
    Err(member_error())
}

fn member_error() -> String {
    text(
        "Guild Members could not be opened.",
        "Die Gildenmitgliederliste konnte nicht geöffnet werden.",
    )
    .into()
}

fn swipe(
    driver: &Driver<'_>,
    screen: &Screen,
    start: (u32, u32),
    end: (u32, u32),
) -> Result<(), String> {
    common::ensure_not_cancelled(driver.cancelled)?;
    crate::adb::swipe_game_points(driver.serial, start, end, screen.size, 650)
        .map_err(|error| error.to_string())?;
    common::settle_cancellable(driver.cancelled)
}

fn guild_identity(screen: &Screen) -> Option<&str> {
    screen.lines.iter().find_map(|l| {
        (l.x < 180.0 && (20.0..96.0).contains(&l.y))
            .then(|| {
                l.text
                    .trim()
                    .strip_prefix("Lvl.")?
                    .split_once(' ')
                    .map(|(_, guild)| guild.trim())
            })
            .flatten()
    })
}

struct ActivityScan {
    rows: Vec<Value>,
    guild: String,
    local: Vec<Member>,
    expected: Option<usize>,
}

fn activity_export(
    driver: &Driver<'_>,
    roster: &[Member],
    log: &mut impl FnMut(String),
) -> Result<ActivityScan, String> {
    let mut screen = open_members(driver, log)?;
    // A manually opened Members list can already be scrolled down.
    let mut first_page = read_members(&screen)?;
    for _ in 0..32 {
        swipe(driver, &screen, (960, 776), (960, 1480))?;
        let frame = driver.read()?;
        if !member_header(&frame) {
            return Err(member_error());
        }
        let page = read_members(&frame)?;
        screen = frame;
        if !page.is_empty() && page == first_page {
            break;
        }
        first_page = page;
    }
    let guild = guild_identity(&screen).unwrap_or_default().to_owned();
    let mut expected = member_count(&screen);
    let mut rows = BTreeMap::new();
    let mut previous = Vec::new();
    let mut unchanged = 0;
    for _ in 0..60 {
        common::ensure_not_cancelled(driver.cancelled)?;
        if !member_header(&screen) {
            screen = driver.wait_for(member_header)?.ok_or_else(member_error)?;
        }
        expected = expected.or_else(|| member_count(&screen));
        let page = read_members(&screen)?;
        if page.is_empty() {
            return Err(text(
                "The activity rows could not be read.",
                "Die Aktivitätswerte konnten nicht gelesen werden.",
            )
            .into());
        }
        for (name, value) in &page {
            let member = corrected_identity(std::slice::from_ref(name), roster);
            let output_name = member.map_or(name.as_str(), |m| &m.name);
            let mut record = json!({"Name": output_name, "Activeness": value});
            if let Some(member) = member {
                put_identity(&mut record, member);
            }
            rows.entry(name_key(output_name)).or_insert(record);
        }
        unchanged = if page == previous { unchanged + 1 } else { 0 };
        if unchanged >= 2 {
            break;
        }
        previous = page;
        swipe(driver, &screen, (960, 1480), (960, 917))?;
        screen = driver.read()?;
    }
    let rows: Vec<_> = rows.into_values().collect();
    let members = rows
        .iter()
        .filter_map(|row| {
            Some(Member {
                name: row["Name"].as_str()?.to_owned(),
                id: row.get("Id").cloned(),
            })
        })
        .collect();
    Ok(ActivityScan {
        rows,
        guild,
        local: members,
        expected,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rankings {
    Dream,
    Supreme,
    Afk,
}

impl Rankings {
    fn top(self) -> u32 {
        match self {
            Self::Supreme => 715,
            _ => 805,
        }
    }

    fn verifies(self, screen: &Screen) -> bool {
        match self {
            Self::Dream => {
                screen
                    .point(680..1040, 1820..1920, "districtrankings")
                    .is_some()
                    && screen
                        .line(10..1050, 20..190, |label| label.contains("district"))
                        .is_some()
            }
            Self::Supreme => {
                screen
                    .line(10..735, 20..160, |label| label.starts_with("supr"))
                    .is_some()
                    && screen
                        .line(10..735, 20..160, |label| label.ends_with("arena"))
                        .is_some()
                    && screen
                        .line(760..1040, 20..145, |label| label.starts_with("district"))
                        .is_some()
            }
            Self::Afk => screen
                .point(475..750, 1820..1920, "phaserankings")
                .is_some(),
        }
    }
}

fn open_rankings(
    driver: &Driver<'_>,
    kind: Rankings,
    log: &mut impl FnMut(String),
) -> Result<Screen, String> {
    let serial = driver.serial;
    let cancelled = driver.cancelled;
    match kind {
        Rankings::Dream => {
            dream_realm::open_rankings_for_scan(serial, cancelled, log)?;
            let screen = driver.read()?;
            if !kind.verifies(&screen) {
                let tab = screen
                    .point(680..1040, 1820..1920, "districtrankings")
                    .ok_or_else(rank_error)?;
                driver.tap(&screen, tab)?;
            }
        }
        Rankings::Supreme => {
            arena::navigate(serial, arena::Kind::Supreme, cancelled, log)?;
            let home = driver
                .wait_for(|screen| screen.point(925..1080, 220..395, "rankings").is_some())?
                .ok_or_else(rank_error)?;
            let button = home
                .point(925..1080, 220..395, "rankings")
                .ok_or_else(rank_error)?;
            driver.tap(&home, button)?;
        }
        Rankings::Afk => phase_rewards::open_rankings_for_scan(serial, cancelled, log)?,
    }
    driver
        .wait_for(|screen| kind.verifies(screen))?
        .ok_or_else(rank_error)
}

fn rank_error() -> String {
    text(
        "The ranking view could not be verified.",
        "Die Rangliste konnte nicht bestätigt werden.",
    )
    .into()
}

fn card_centers(screen: &Screen, top: u32) -> Vec<u32> {
    let background = |y| {
        [44, 49, 54].into_iter().all(|x| {
            let [r, g, b] = screen.color.get_pixel(x, y).0;
            r.min(g).min(b) > 190 && r.max(g).max(b) - r.min(g).min(b) < 40
        })
    };
    let mut start = top;
    let mut centers = Vec::new();
    for y in top..=1565 {
        if y == 1565 || background(y) {
            let center = (start + y) / 2;
            // The player's own card may be pinned above or below the list.
            if (130..=179).contains(&y.saturating_sub(start))
                && screen.color.get_pixel(1010, center - 30).0[1] >= 200
            {
                centers.push(center);
            }
            start = y + 1;
        }
    }
    centers
}

fn rank_number(screen: &Screen, center: u32) -> Result<Option<u32>, String> {
    use image::{GrayImage, Luma};
    let area = image::imageops::crop_imm(&screen.gray, 54, center - 44, 90, 80).to_image();
    for threshold in [175, 150, 190] {
        let (mut left, mut top, mut right, mut bottom) = (90_u32, 80_u32, 0_u32, 0_u32);
        for (x, y, pixel) in area.enumerate_pixels() {
            if pixel.0[0] >= threshold {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
        if right <= left || !(24..=67).contains(&bottom.saturating_sub(top)) {
            continue;
        }
        let (width, height) = (right - left, bottom - top);
        let mut word = GrayImage::from_pixel(width * 3 + 64, height + 60, Luma([255]));
        for x in 0..width {
            for y in 0..height {
                if area.get_pixel(left + x, top + y).0[0] >= threshold {
                    for copy in 0..3 {
                        word.put_pixel(30 + copy * (width + 2) + x, 30 + y, Luma([0]));
                    }
                }
            }
        }
        let word = image::imageops::resize(
            &word,
            word.width() * 3,
            word.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        let lines = super::super::ocr::recognize(&word)?;
        if let [line] = lines.as_slice() {
            let word = line.text.split_whitespace().collect::<String>();
            if (3..=12).contains(&word.len())
                && word.len() % 3 == 0
                && word.bytes().all(|c| c.is_ascii_digit())
            {
                let width = word.len() / 3;
                let prefix = &word[..width];
                if word[width..2 * width] == *prefix && word[2 * width..] == *prefix {
                    if let Some(rank) = digits(prefix).filter(|rank| (1..=1000).contains(rank)) {
                        return Ok(Some(rank));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn ranking_name(label: &str) -> Option<String> {
    let label = label.trim();
    let without_shard = label
        .rfind('S')
        .filter(|&start| {
            let code = label[start + 1..].trim_matches(|c: char| !c.is_alphanumeric());
            (3..=4).contains(&code.len()) && digits(code).is_some()
        })
        .map_or(label, |start| &label[..start]);
    clean_name(without_shard)
}

fn name_ink(screen: &Screen, x: u32, y: u32) -> bool {
    let [r, g, b] = screen.color.get_pixel(x, y).0.map(i16::from);
    (86..=166).contains(&r) && (4..=25).contains(&(r - g)) && (0..=10).contains(&(b - g))
}

fn rank_names(screen: &Screen, center: u32) -> Result<Vec<String>, String> {
    use image::{GrayImage, Luma};
    let mut candidates = Vec::new();
    // Isolate the warm gray name from the colored avatar and server pill.
    for threshold in [0, 140, 165] {
        let mut word = GrayImage::from_pixel(508, 103, Luma([255]));
        for y in 0..71 {
            for x in 0..476 {
                let px = 303 + x;
                let py = center - 60 + y;
                let dark = if threshold == 0 {
                    name_ink(screen, px, py)
                } else {
                    screen.gray.get_pixel(px, py).0[0] <= threshold
                };
                if dark {
                    word.put_pixel(x + 16, y + 16, Luma([0]));
                }
            }
        }
        let word = image::imageops::resize(
            &word,
            word.width() * 3,
            word.height() * 3,
            image::imageops::FilterType::CatmullRom,
        );
        candidates.extend(
            super::super::ocr::recognize(&word)?
                .iter()
                .filter(|line| line.height >= 72.0 && line.x < 650.0)
                .filter_map(|line| ranking_name(&line.text)),
        );
    }
    candidates.extend(
        crop(screen, 303, center - 60, 476, 71)?
            .iter()
            .filter(|line| line.height >= 25.0 && line.x < 475.0)
            .filter_map(|line| ranking_name(&line.text)),
    );
    Ok(candidates)
}

fn ranking_page(
    screen: &Screen,
    kind: Rankings,
    roster: &[Member],
    guild: &str,
) -> Result<Vec<(u32, Option<Value>)>, String> {
    let mut page = Vec::new();
    for center in card_centers(screen, kind.top()) {
        let Some(rank) = rank_number(screen, center)? else {
            continue;
        };
        let in_guild = !guild.is_empty()
            && crop(screen, 352, center + 17, 422, 36)?
                .iter()
                .any(|l| name_key(&l.text) == name_key(guild));
        let names = rank_names(screen, center)?;
        let member = corrected_identity(&names, roster);
        let name = member.map(|m| m.name.as_str()).or_else(|| {
            in_guild
                .then(|| names.first().map(String::as_str))
                .flatten()
        });
        let record = name.map(|name| {
            let mut row = json!({"Rank":rank.to_string(), "Name":name});
            if let Some(member) = member {
                put_identity(&mut row, member);
            }
            row
        });
        page.push((rank, record));
    }
    Ok(page)
}

fn scan_ranks(
    driver: &Driver<'_>,
    mut screen: Screen,
    kind: Rankings,
    roster: &[Member],
    guild: &str,
    date: &str,
) -> Result<Vec<Value>, String> {
    let first_rank = |screen: &Screen| {
        card_centers(screen, kind.top())
            .first()
            .and_then(|&y| rank_number(screen, y).ok().flatten())
    };
    if let Some(mut rank) = first_rank(&screen).filter(|&rank| rank > 2) {
        for _ in 0..32 {
            swipe(driver, &screen, (960, 920), (960, 1490))?;
            screen = driver.read()?;
            if !kind.verifies(&screen) {
                return Err(rank_error());
            }
            let Some(current) = first_rank(&screen) else {
                continue;
            };
            if current <= 2 || current == rank {
                break;
            }
            rank = current;
        }
    }
    let mut rows = BTreeMap::new();
    let mut previous = Vec::new();
    let mut unchanged = 0;
    for _ in 0..150 {
        common::ensure_not_cancelled(driver.cancelled)?;
        if !kind.verifies(&screen) {
            return Err(rank_error());
        }
        let page = ranking_page(&screen, kind, roster, guild)?;
        let ranks: Vec<_> = page.iter().map(|(rank, _)| *rank).collect();
        if ranks.is_empty() {
            return Err(rank_error());
        }
        for (rank, row) in page {
            if let Some(mut row) = row {
                row["Date"] = json!(date);
                rows.entry(rank).or_insert(row);
            }
        }
        unchanged = if ranks == previous { unchanged + 1 } else { 0 };
        if unchanged >= 2 {
            return Ok(rows.into_values().collect());
        }
        previous = ranks;
        swipe(driver, &screen, (960, 1470), (960, 980))?;
        screen = driver.read()?;
    }
    Err(text(
        "The end of the ranking list could not be verified.",
        "Das Ende der Rangliste konnte nicht bestätigt werden.",
    )
    .into())
}

fn dates(days_ago: u64) -> ((u32, u32), &'static str) {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400;
    let days = days.saturating_sub(days_ago);
    let weekday = [
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
    ][(days % 7) as usize];
    let mut remainder = days as u32;
    for year in 1970.. {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        for (i, len) in [
            31,
            28 + u32::from(leap),
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ]
        .into_iter()
        .enumerate()
        {
            if remainder < len {
                return (((i + 1) as u32, remainder + 1), weekday);
            }
            remainder -= len;
        }
    }
    unreachable!()
}

fn date_tab(line: &super::super::ocr::TextLine) -> Option<(u32, u32)> {
    if !(700..790).contains(&line.center().1) {
        return None;
    }
    let (m, d) = line.text.trim().split_once('/')?;
    Some((digits(m)?, digits(d)?))
}

fn select_date(
    driver: &Driver<'_>,
    mut screen: Screen,
    date: (u32, u32),
) -> Result<Screen, String> {
    for _ in 0..4 {
        if let Some(tab) = screen
            .lines
            .iter()
            .find(|line| date_tab(line) == Some(date))
        {
            let point = tab.center();
            driver.tap(&screen, point)?;
            return driver
                .wait_for(|frame| {
                    Rankings::Dream.verifies(frame)
                        && frame.lines.iter().any(|l| {
                            if date_tab(l) != Some(date) {
                                return false;
                            }
                            let (x, _) = l.center();
                            ((x.saturating_sub(145))..x).any(|x| {
                                (790..805).any(|y| {
                                    let [r, g, b] = frame.color.get_pixel(x, y).0;
                                    g > r.saturating_add(18) && g > b.saturating_add(15)
                                })
                            })
                        })
                })?
                .ok_or_else(rank_error);
        }
        swipe(driver, &screen, (930, 745), (230, 745))?;
        screen = driver.read()?;
    }
    Err(text(
        "The requested Dream Realm date was not found.",
        "Das gewünschte Dream-Realm-Datum wurde nicht gefunden.",
    )
    .into())
}

pub(super) fn scan(
    driver: &Driver<'_>,
    settings: &GuildSettings,
    log: &mut impl FnMut(String),
    report: &mut dyn FnMut(ActivityEvent),
    save: &mut impl FnMut(&str, &Value) -> Result<(), String>,
) -> Result<u32, String> {
    if settings.dream_days == 0
        && !settings.supreme_arena
        && !settings.activeness
        && !settings.afk_stages
    {
        return Err(text(
            "Select at least one Guild Manager export.",
            "Wähle mindestens einen Guild-Manager-Export.",
        )
        .into());
    }
    let remote = if settings.guild_members_url.trim().is_empty() {
        Vec::new()
    } else {
        remote_members(&settings.guild_members_url, &settings.guild_api_key)?
    };
    let ActivityScan {
        rows: activeness,
        guild,
        local,
        expected,
    } = if settings.activeness || remote.is_empty() {
        activity_export(driver, &remote, log)?
    } else {
        ActivityScan {
            rows: Vec::new(),
            guild: String::new(),
            local: Vec::new(),
            expected: None,
        }
    };
    let mut total = 0;
    let mut export = |file, rows: Vec<Value>| -> Result<(), String> {
        save(file, &json!(rows))?;
        total += rows.len() as u32;
        report(ActivityEvent::Collected(
            GameMode::GuildScan,
            rows.len() as u32,
        ));
        Ok(())
    };
    let partial_activity = settings.activeness && expected != Some(activeness.len());
    if settings.activeness {
        let file = if partial_activity {
            "guild_activeness_partial.json"
        } else {
            ACTIVENESS_FILE
        };
        export(file, activeness)?;
    }
    let roster = if remote.is_empty() { &local } else { &remote };
    if settings.dream_days > 0 {
        let mut rankings = Vec::new();
        for day in 1..=settings.dream_days.min(3) {
            let (date, weekday) = dates(u64::from(day));
            let screen = open_rankings(driver, Rankings::Dream, log)?;
            let screen = select_date(driver, screen, date)?;
            rankings.extend(scan_ranks(
                driver,
                screen,
                Rankings::Dream,
                roster,
                &guild,
                weekday,
            )?);
        }
        export(DREAM_FILE, rankings)?;
    }
    if settings.supreme_arena {
        let screen = open_rankings(driver, Rankings::Supreme, log)?;
        export(
            SUPREME_FILE,
            scan_ranks(
                driver,
                screen,
                Rankings::Supreme,
                roster,
                &guild,
                dates(0).1,
            )?,
        )?;
    }
    if settings.afk_stages {
        let screen = open_rankings(driver, Rankings::Afk, log)?;
        let phase = screen
            .lines
            .iter()
            .find_map(|line| {
                let (x, y) = line.center();
                if !(0..=1030).contains(&x) || !(690..790).contains(&y) {
                    return None;
                }
                let value = super::super::key(&line.text);
                let n = digits(value.strip_prefix("phase")?)?;
                let green = (x.saturating_sub(145)..x).any(|x| {
                    (790..805).any(|y| {
                        let [r, g, b] = screen.color.get_pixel(x, y).0;
                        g > r.saturating_add(18) && g > b.saturating_add(15)
                    })
                });
                green.then_some(n)
            })
            .ok_or_else(rank_error)?;
        export(
            AFK_FILE,
            scan_ranks(
                driver,
                screen,
                Rankings::Afk,
                roster,
                &guild,
                &format!("Phase {phase}"),
            )?,
        )?;
    }
    if partial_activity {
        Err(text(
            "The activity export is partial. Review it before importing.",
            "Der Aktivitätsexport ist unvollständig. Prüfe ihn vor dem Import.",
        )
        .into())
    } else {
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::afk_journey::progression::captured;

    #[test]
    fn supabase_roster_url_requires_the_expected_https_origin_and_path() {
        assert!(
            members_url("https://project.supabase.co/rest/v1/guild_states?select=state").is_ok()
        );
        for url in [
            "http://project.supabase.co/rest/v1/guild_states",
            "https://project.supabase.co.example.org/rest/v1/guild_states",
            "https://example.org/rest/v1/guild_states?x=.supabase.co",
            "https://project.supabase.co/rest/v1/another_table",
        ] {
            assert!(members_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn pasted_api_key_moves_from_the_query_to_the_header_value() {
        assert_eq!(
            split_access(
                " https://p.supabase.co/rest/v1/guild_states?select=state&apikey=secret&id=eq.7 "
            ),
            (
                "https://p.supabase.co/rest/v1/guild_states?select=state&id=eq.7".into(),
                Some("secret".into())
            )
        );
        assert_eq!(
            split_access("https://p.supabase.co/rest/v1/guild_states?apikey=secret"),
            (
                "https://p.supabase.co/rest/v1/guild_states".into(),
                Some("secret".into())
            )
        );
        assert_eq!(
            split_access("https://p.supabase.co/rest/v1/guild_states?select=state&apikey="),
            (
                "https://p.supabase.co/rest/v1/guild_states?select=state".into(),
                None
            )
        );
        assert_eq!(
            split_access("https://p.supabase.co/rest/v1/guild_states"),
            ("https://p.supabase.co/rest/v1/guild_states".into(), None)
        );
        assert!(members_url(
            &split_access("https://p.supabase.co/rest/v1/guild_states?apikey=x").0
        )
        .is_ok());
    }

    #[test]
    fn supabase_roster_adds_canonical_name_and_id_to_export() {
        let states: Vec<GuildState> = serde_json::from_value(json!([{
            "state": {"players": [{"name": "Bishamon", "id": "member-id"}]}
        }]))
        .unwrap();
        let member =
            corrected_identity(&["Bishamom".to_owned()], &states[0].state.players).unwrap();
        let mut row = json!({"Activeness": 1970});
        put_identity(&mut row, member);
        assert_eq!(
            row,
            json!({"Name": "Bishamon", "Id": "member-id", "Activeness": 1970})
        );
    }

    #[test]
    #[ignore = "uses English in-game roster and ranking captures"]
    fn scanner_capture_rows() {
        let screen = captured("scanner_members");
        assert!(member_header(&screen));
        let roster = read_members(&screen).unwrap();
        println!("Activeness: {roster:?}");
        assert!(roster
            .iter()
            .any(|(name, value)| name == "Bishamon" && *value == 1970));
        for (name, kind, rank) in [
            ("scanner_dream_yesterday_scroll", Rankings::Dream, 8),
            ("scanner_supreme_rank_live", Rankings::Supreme, 1),
            ("scanner_afk_rank", Rankings::Afk, 1),
        ] {
            let screen = captured(name);
            assert!(kind.verifies(&screen), "{name}");
            let rows = ranking_page(&screen, kind, &[], "Witchhunt").unwrap();
            println!("{name}: {rows:?}");
            assert!(rows
                .iter()
                .any(|(found, row)| *found == rank && (row.is_some() || kind == Rankings::Afk)));
        }
    }
}
