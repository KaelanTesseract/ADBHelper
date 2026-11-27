# Heldendatenbank Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ADBHelper keeps a local `heroes.json` with every AFK Journey hero (roster, facts, own progress, current stats, skill numbers per level), filled by a new "Hero database" scan started from the Settings page.

**Architecture:** A pure data module (`hero_db`) owns the file format, loading, saving and merging. Two pure parsers turn OCR text into stats and skill levels. A new scanner mode walks the Resonating Hall heroes like the existing hero scan and, per hero, opens "Hero Stats" and the skill tab. The settings page gets the start button; the action is hidden from the dashboard and the daily routine.

**Tech Stack:** Rust, serde/serde_json, existing ureq `client()`, Windows OCR (`ocr::recognize`), egui.

**Spec:** `docs/superpowers/specs/2026-11-27-hero-database-design.md`

## Global Constraints

- File: `heroes.json` via `crate::settings::file` / `crate::settings::write` (`%APPDATA%\ADBHelper`), atomic write, `"version": 1`.
- Every struct `#[serde(default)]`; missing fields load as defaults, unknown fields are ignored.
- From github.com/arnecls/afjk-characters only facts: faction, class, damage type, range, skill slot, cooldown, unlock. **No description texts.** Fetched at runtime, never shipped.
- Roster from AFKJ Tracker `heroes-template.json` (already fetched by `hero::reference`).
- New UI texts only via `tr!`, in `locales/en.ftl` and `locales/de.ftl`.
- `cargo fmt`, `cargo clippy --release --locked --all-targets -- -D warnings`, `cargo test --release --locked` green after every task.
- Emulator tests are `#[ignore]`, run as release builds, and only after Dennis gives a go each time. Captures stay private in `.update-test/` (never committed).
- The existing AFKJ Tracker export (`afkj_tracker_backup.json` / `_partial.json`) stays byte-for-byte as it is.

## Review Focus

- Stat values with suffixes and units ("2152M", "1.2B", "530.0%", "44.1", "1040") must parse to the right number; an unreadable row is left out, never stored as 0.
- A hero the tracker roster does not know yet (new release) still gets a record keyed by its normalized name.
- Network failure for roster or facts keeps the last stored values instead of wiping them.
- Cancelling mid-scan keeps every hero finished so far (save after each hero).
- A corrupt `heroes.json` is not silently overwritten: it is renamed to `heroes.json.broken` and logged, then a fresh file starts.

---

### Task 1: Data model and file (`hero_db`)

**Files:**
- Create: `src/afk_journey/progression/hero_db.rs`
- Modify: `src/afk_journey/progression.rs` (add `pub mod hero_db;`)

**Interfaces:**
- Produces:
  - `pub struct HeroDatabase { pub version: u32, pub updated: String, pub heroes: BTreeMap<String, HeroRecord> }`
  - `pub struct HeroRecord { name, faction, class, rarity, damage_type, range: String; owned: bool; progress: Option<Progress>; stats: Option<Stats>; skills: Vec<Skill>; sources: BTreeMap<String, String> }`
  - `pub struct Progress { level: Option<u32>, ascension: Option<String>, ex_weapon: Option<u32>, scanned: String }`
  - `pub struct Stats { values: BTreeMap<String, f64>, scanned: String }` (keys are `ascii_key` of the game label, e.g. `"atk"`, `"physdef"`, `"crit"`)
  - `pub struct Skill { slot: String, name: String, level: Option<u32>, cooldown_s: Option<f64>, range: Option<String>, energy: Option<u32>, unlock: Option<String>, levels: Vec<SkillLevel>, text: String, source: String }`
  - `pub struct SkillLevel { level: u32, values: Vec<f64> }`
  - `pub fn hero_id(name: &str) -> String` (lowercase, spaces → `-`, keeps `&`; "Elijah & Lailah" → `"elijah-&-lailah"`, same as the tracker ids)
  - `impl HeroDatabase { pub fn load() -> Result<(Self, Option<String>), String>; pub fn save(&mut self) -> Result<(), String>; pub fn merge_roster(&mut self, template: &serde_json::Value); pub fn merge_facts(&mut self, facts: &[HeroFacts]); pub fn record(&mut self, name: &str) -> &mut HeroRecord }`
  - `pub struct HeroFacts { id: String, faction, class, damage_type, range: String, skills: Vec<SkillFacts> }`, `pub struct SkillFacts { slot: String, cooldown_s: Option<f64>, unlock: Option<String> }`

- [ ] **Step 1: Write the failing tests** in `hero_db.rs`:
  - `hero_ids_match_the_tracker`: `hero_id("Elijah & Lailah") == "elijah-&-lailah"`, `hero_id("Frieren") == "frieren"`.
  - `roster_adds_every_hero_and_keeps_scanned_data`: a template `{"heroes":[{"id":"frieren","name":"Frieren","class":"Mage","faction":"Celestial","rarity":"Cele/Hypo"}]}` into a database that already holds `frieren` with stats → name/class/faction/rarity set, stats unchanged, `owned` unchanged.
  - `facts_fill_only_empty_fields_and_never_texts`: `merge_facts` sets `damage_type`, `range`, and per matching `slot` the `cooldown_s`/`unlock`; a skill `text` already read from the game stays.
  - `an_unknown_hero_gets_its_own_record`: `record("Thane")` creates `heroes["thane"]` with `name == "Thane"`.
  - `old_files_load`: `serde_json::from_str::<HeroDatabase>(r#"{"heroes":{"frieren":{"name":"Frieren"}}}"#)` works, `version` defaults to 1.
- [ ] **Step 2:** `cargo test --release --locked hero_db` → fails (module missing).
- [ ] **Step 3: Implement.** `load()` returns `(db, warning)`: missing file → empty db; parse error → rename to `heroes.json.broken`, empty db, `Some(warning)`. `save()` sets `updated` (RFC 3339 UTC via `std::time::SystemTime`, format by hand like other scan files or with the `time` handling already in the crate) and writes pretty JSON with `crate::settings::write`.
- [ ] **Step 4:** `cargo test --release --locked hero_db` → PASS; fmt, clippy clean.
- [ ] **Step 5: Commit** `feat(heroes): hero database file and merging`.

### Task 2: Facts from afjk-characters

**Files:**
- Create: `src/afk_journey/progression/hero_facts.rs`
- Modify: `src/afk_journey/progression.rs`

**Interfaces:**
- Consumes: `HeroFacts`, `SkillFacts` (Task 1); `scanner::client()` (make it `pub(in crate::afk_journey::progression)`).
- Produces: `pub fn parse_facts(id: &str, source: &serde_json::Value) -> Option<HeroFacts>`; `pub fn fetch_facts(log: &mut impl FnMut(String)) -> Vec<HeroFacts>`.

- [ ] **Step 1: Failing test** `facts_take_numbers_not_texts` with an inline `source.json` shaped like the repo (`source.faction`, `source.class`, `source.damage_type`, `source.range`, `source.skills[] { section: "Ultimate", meta: { "Cooldown": "15s" }, unlock: "Lv. 11", description: {...} }`): result has `cooldown_s == Some(15.0)`, `unlock == Some("Lv. 11")`, and the struct has no field that could hold `description`.
- [ ] **Step 2:** run → fails.
- [ ] **Step 3: Implement.** `fetch_facts` reads `https://raw.githubusercontent.com/arnecls/afjk-characters/main/data/roster.json`, then each `data/heroes/<id>/source.json` (2 MB body limit like `hero::reference`). Any failure: log one line `tr!("hero-db-facts-unavailable")` and return what was read so far. Slot names: `Ultimate` → `ult`, `Skill 1`/`Skill I` → `s1`, `Skill 2`/`Skill II` → `s2`, `Exclusive Weapon` → `ex`, `Enhance Force` → `enhance`, others lowercased `ascii_key`.
- [ ] **Step 4:** test passes; one `#[ignore]` network test `facts_download` prints the hero count (expect ≥ 130).
- [ ] **Step 5: Commit** `feat(heroes): facts from afjk-characters (no texts)`.

### Task 3: Reading "Hero Stats"

**Files:**
- Create: `src/afk_journey/progression/hero_stats.rs`

**Interfaces:**
- Consumes: `Stats` (Task 1), `Driver`, `Screen`, `ocr`.
- Produces: `pub fn stat_value(text: &str) -> Option<f64>`; `pub fn parse_stat_rows(lines: &[ocr::TextLine]) -> BTreeMap<String, f64>`; `pub(super) fn read_stats(driver: &Driver<'_>, profile: &Screen) -> Result<Option<Stats>, ActionError>`.

- [ ] **Step 1: Failing tests:**
  - `stat_values_keep_their_size`: `"2152M"` → `2_152_000_000.0`, `"1.2B"` → `1_200_000_000.0`, `"31,311K"` → `31_311_000.0`, `"530.0%"` → `530.0`, `"44.1"` → `44.1`, `"1040"` → `1040.0`, `"--"` → `None`.
  - `rows_pair_label_and_value_on_one_line`: lines "ATK" at y 1030 x 80 and "2152M" at y 1032 x 960 → `{"atk": 2.152e9}`; a label without a value on its row is left out.
- [ ] **Step 2:** run → fails.
- [ ] **Step 3: Implement.** Screen steps (1080×1920 base): tap the "i" beside power at (920, 1686); wait for the "Hero Stats" title (OCR "herostats" in y 860–1000); OCR rows in y 1000–1880; swipe (540, 1600) → (540, 1000) 600 ms and repeat until a row key `ultimatespower` was read or 6 swipes; merge rows (first value wins); tap back (115, 1795). Ignore section headers (`rivalrystats`, `basicstats`, `offensivestats`, `defensivestats`, `otherstats`).
- [ ] **Step 4:** tests pass; capture test `captured_hero_stats` (`#[ignore]`, env `ADBHELPER_TEST_HERO_DB=.update-test/hero-db`, files `stats-1.png` … ) asserts `atk`, `physdef`, `crit`, `proficiency` present. Captures are taken in Task 6's emulator run.
- [ ] **Step 5: Commit** `feat(heroes): read Hero Stats`.

### Task 4: Reading skills

**Files:**
- Create: `src/afk_journey/progression/hero_skills.rs`

**Interfaces:**
- Consumes: `Skill`, `SkillLevel` (Task 1).
- Produces: `pub fn skill_levels(text: &str) -> Vec<SkillLevel>`; `pub fn skill_header(lines: &[ocr::TextLine]) -> Option<(String, Option<u32>, Option<f64>, Option<String>, Option<u32>)>` (name, level, cooldown s, range, energy); `pub(super) fn read_skills(driver: &Driver<'_>) -> Result<Vec<Skill>, ActionError>`.

- [ ] **Step 1: Failing tests:**
  - `level_one_and_upgrades_are_split`: the Zoltraak text "… dealing 220% + 30% damage per second and 1400% + 180% true damage … Level 2: Increases damage per second to 240% + 30%, and the extra true damage to 1500% + 180%." → `[SkillLevel{level:1, values:[220,30,1400,180]}, SkillLevel{level:2, values:[240,30,1500,180]}]`.
  - `header_reads_name_level_and_numbers`: lines "Zoltraak", "Level: 5", "0s", "Global", "350" in the header band → `("Zoltraak", Some(5), Some(0.0), Some("Global"), Some(350))`.
- [ ] **Step 2:** run → fails.
- [ ] **Step 3: Implement.** From the hero profile: tap the skill tab (1010, 760); if the toggle reads "Full" tap (950, 1322) once so the text shows numbers (toggle then reads "Lite"); skill icons at y 1080, x 194, 340, 480, 610, 744, 882 (slots `ult`, `s1`, `s2`, `s3`, `s4`, `ex`; a slot whose icon is missing is skipped); per icon: tap, read header band y 1210–1340 and text band y 1370–1700, swipe the text (540, 1650) → (540, 1400) until no new line appears (max 5), join the text; `levels = skill_levels(text)`, `source = "game"`. Return with back (115, 1795) to the profile.
- [ ] **Step 4:** tests pass; capture test `captured_hero_skill` (`#[ignore]`, same env, `skill-full.png`) asserts the header and at least one level.
- [ ] **Step 5: Commit** `feat(heroes): read skills with numbers per level`.

### Task 5: Hero database scan mode

**Files:**
- Create: `src/afk_journey/progression/scanner/hero_db_scan.rs`
- Modify: `src/activity.rs` (new `GameMode::HeroDatabase`, label "Hero database"), `src/afk_journey/progression/scanner.rs` (`run` branch), `src/routine.rs` (new `Action::HeroDatabase`, not offered in the daily editor), `src/afk_journey/afk_stages.rs:1188` (same match arm as `HeroScan`), `src/app/run.rs:44,160,302`, `src/afk_journey/progression/scanner/hero.rs`

**Interfaces:**
- Consumes: Tasks 1–4; `hall::open_first_hero`, `hall::wait_for_affinity_profile`, `hall::hero_name`, `hall::NEXT_HERO`.
- Produces: `pub(super) fn scan(driver: &Driver<'_>, max_age_days: Option<u32>, log: &mut impl FnMut(String), report: &mut dyn FnMut(ActivityEvent)) -> Result<u32, ActionError>`; `GuildSettings`-like option passing: `scanner::run(..., hero_db_age: Option<u32>, ...)`.

- [ ] **Step 1: Failing test** `fresh_heroes_are_skipped`: pure helper `pub(super) fn needs_scan(record: &HeroRecord, today: &str, max_age_days: Option<u32>) -> bool`; `None` → always true; stats scanned "2026-11-27", today "2026-11-30", max 5 → false; max 2 → true; no stats → true.
- [ ] **Step 2:** run → fails.
- [ ] **Step 3: Implement.** Load db (log warning if any), `merge_roster(reference("heroes-template.json"))` (on error keep stored roster, log once), `merge_facts(fetch_facts(log))`. Walk heroes exactly like `hero::scan` (same end conditions: chippy/hammie, back at first nameplate, `heroes.len() + 30` guard). Per hero: `record(name)`, `owned = true`, progress from `ascension`/`exclusive_level` (make them `pub(super)`), and if `needs_scan`: `read_stats`, then `read_skills`; `db.save()` after each hero; `report(Collected(GameMode::HeroDatabase, 1))`. Normal `hero::scan` additionally merges roster + progress into `heroes.json` after its export (no stats/skills); its export files stay unchanged.
- [ ] **Step 4:** tests pass, full suite green.
- [ ] **Step 5: Commit** `feat(heroes): hero database scan`.

### Task 6: Settings page button and option

**Files:**
- Modify: `src/app/settings_page.rs`, `src/app/saved_settings.rs` (field `hero_db_max_age_days: Option<u32>`, default `Some(7)`), `src/app/mod.rs` (field on `PlayerApp`, `RunSettings`), `locales/en.ftl`, `locales/de.ftl`

**Interfaces:**
- Consumes: `StageAction::HeroDatabase` (Task 5), `PlayerApp::start_action`-style entry used by dashboard buttons in `run.rs`.

- [ ] **Step 1: Failing test** in `saved_settings.rs`: `hero_db_age_is_saved` (round trip with `Some(3)` and `None`).
- [ ] **Step 2:** run → fails.
- [ ] **Step 3: Implement.** Settings page section "Hero database" (`tr!("app-settings-hero-db-title")`): short explanation (`app-settings-hero-db-explain`: reads stats and skills of every owned hero into heroes.json; takes long), checkbox "Only heroes without data or older than N days" with a `DragValue` 1..=60, button "Scan hero database" (disabled while busy, needs a device), last-scan line from `HeroDatabase::load().updated`, and a button "Open folder" that reveals `heroes.json`. Progress and errors use the existing activity/log path.
- [ ] **Step 4:** tests green; GUI hand check after Dennis' go: button visible only on Settings, not on Dashboard or in the daily editor.
- [ ] **Step 5: Commit** `feat(heroes): start hero database scan from Settings`.

### Task 7: Emulator check and captures

- [ ] Ask Dennis for a go. Run the hero database scan on 3 heroes (temporary env `ADBHELPER_HERO_DB_LIMIT=3` read only in the `#[ignore]` test `connected_emulator_hero_db_three_heroes`), save stats/skill captures privately to `.update-test/hero-db/`, run the capture tests from Tasks 3–4, inspect `heroes.json`.
- [ ] Delete scratchpad traces, update the vault note, commit only after Dennis' OK.

---

## Ergänzung: Hero Box (Tasks 8–11, nach Spec-Ergänzung 2026-11-27)

### Task 8: Felder und Kennzeichen
- `HeroRecord.stars: Option<u32>`, `HeroRecord.portrait: Option<String>`; `pub fn is_supreme_plus(ascension: &str) -> bool` (Supreme+ und Paragon 1–4 → true, Supreme/Mythic+ → false). Tests: `supreme_plus_includes_paragon`, `old_files_without_stars_load`.

### Task 9: Übersichtsbild und Zwischenablage
- `src/hero_box_image.rs`: `pub fn compose(heroes: &[BoxHero]) -> RgbaImage` (Raster, 8 Spalten, Kachel 128×128 + Sternzeile, S+-Rahmen gold, Sterne als gezeichnete Formen, „S+“ mit kleiner Pixelschrift), `pub fn dib(image: &RgbaImage) -> Vec<u8>` (BITMAPINFOHEADER + BGRA bottom-up), `pub fn copy_to_clipboard(image) -> Result<(), String>` (Windows: CF_DIB; sonst Err „unsupported“). windows-sys Features `Win32_System_DataExchange`, `Win32_System_Memory`, `Win32_System_Ole` ergänzen (keine neue Abhängigkeit).
- Tests: `compose_places_heroes_in_rows` (9 Helden → 2 Zeilen, Größe), `supreme_plus_gets_a_gold_frame` (Pixelfarbe am Rand), `dib_is_bottom_up_bgra` (Header 40 Byte, Breite/Höhe, erstes Pixel = letzte Zeile).

### Task 10: Tab „Hero Box“
- `Page::HeroBox` neben Settings; `src/app/hero_box.rs`: Kopf mit Scan-Start/Stopp (gleiche Logik wie Settings, als gemeinsame Funktion), Knopf „Bild erstellen“, Raster aller Helden (Porträt-Textur aus Datei, sonst grau mit Name), S+-Rahmen, Sterne. Daten werden beim Öffnen des Tabs und nach Scan-Ende neu geladen. Texte via `tr!` en/de.

### Task 11: Porträt und Sterne lesen (nach Emulator-Blick mit Go)
- Lesebereiche festlegen, Porträt zuschneiden und speichern, Sterne zählen; Tests mit privaten Aufnahmen.
