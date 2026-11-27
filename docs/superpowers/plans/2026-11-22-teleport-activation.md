# Teleporter aktivieren – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Neue Aktion „Teleports“, die auf allen freien Kartenreitern inaktive Teleporter findet, hinläuft und so aktiviert.

**Architecture:** Die Kartensuche des Angelns (`fishing/map.rs`, `route.rs`) bekommt die gesuchte Marker-Art als Parameter. Ein neues Modul `fishing/teleports.rs` nutzt dieselbe Suche mit Teleporter-Vorlagen, eigene Fenster- und Ankunftserkennung und eine Sperrliste pro Lauf. Die App bekommt `StageAction::Teleports` / `GameMode::Teleports` wie Fishing.

**Tech Stack:** Rust, egui, ADB, NCC-Vorlagen (`vision`), Windows-OCR (`ocr`), Fluent (`tr!`).

**Spec:** `docs/superpowers/specs/2026-11-22-teleport-activation-design.md`

## Global Constraints

- Neue Oberflächentexte nur über `tr!` mit Eintrag in `locales/en.ftl` und `locales/de.ftl`.
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` grün.
- Jeder Emulator-Lauf nur nach Go des Benutzers, ein Go gilt einmal; Emulator-Tests mit `cargo test --release`.
- Aufnahmen nur in `.update-test/` oder Scratchpad, nie im Repo; Vorlagen-Ausschnitte (PNG in `assets/AFKJourney/teleports/`) sind erlaubt.
- Nichts antippen, was kein Teleporter, Lauf-Knopf oder bekanntes Popup ist.
- Commit nur nach OK des Benutzers, kein Push.

## Review Focus

- Aktiver und inaktiver Teleporter sehen ähnlich aus → nur inaktive dürfen gefunden werden (Test in Task 3 mit beiden Vorlagen-Bildern als Rauschen/Treffer).
- Teleporter, der nach dem Hinlaufen inaktiv bleibt → gesperrt, kein zweiter Versuch im selben Lauf (Test `skips_end_every_map`).
- Karte ohne Teleporter oder alle schon aktiv → `Ok(0)` ohne Fehler (Ablauf in Task 3, im Emulator-Lauf Task 5 geprüft).
- Figur steht beim Antippen bereits auf dem Teleporter → Marker verdeckt, wird nicht gefunden, kein Hänger.
- Angeln verhält sich nach der Verallgemeinerung gleich (vorhandene Fishing-Tests und ein Angel-Emulatorlauf in Task 5).

---

### Task 1: Aufnahme (Go nötig)

**Files:** nur private Bilder in `.update-test/teleports/`; Werte in die Tabelle unten.

- [ ] **Step 1:** Benutzer um Go bitten. Dann mit dem vorhandenen Diagnose-Weg (Screenshots per adb, `fishing_frame_diagnostic`) aufnehmen: Karte ganz herausgezoomt mit mindestens einem inaktiven und einem aktiven Teleporter; Fenster nach Antippen eines inaktiven Teleporters; Ankunft am Teleporter (Bilder alle ~1 s bis 10 s nach Ankunft); ein Hinlaufen zu einem weiter entfernten Teleporter mit Zeitmessung.
- [ ] **Step 2:** Vorlagen ausschneiden: `assets/AFKJourney/teleports/inactive.png`, `assets/AFKJourney/teleports/active.png` (gleicher Maßstab wie herausgezoomt), ggf. `teleports/walk.png`, falls der Lauf-Knopf anders als `fishing/map_walk.png` aussieht. NCC-Werte messen: inaktiv gegen inaktiv/aktiv/Kartenhintergrund.
- [ ] **Step 3:** Tabelle füllen:

| Wert | Bedeutung | Gemessen |
|---|---|---|
| `INACTIVE_CONFIDENCE` | Schwelle inaktiver Teleporter | |
| Fenster | Titel-OCR, Lauf-Knopf-Lage relativ zum Marker | |
| `ACTIVATED` | Woran Ankunft/Aktivierung erkennbar ist (Text, Symbol) | |
| `WALK_LIMIT_SECS` | Obergrenze Laufzeit (≥ 90) | |

### Task 2: Kartensuche mit wählbarer Marker-Art

**Files:**
- Modify: `src/afk_journey/fishing/map.rs`, `src/afk_journey/fishing.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy)] pub(super) struct MarkerKind { pub paths: &'static [&'static str], pub confidence: f32 }`
  - `pub(super) const SPOTS: MarkerKind` (= bisherige `MARKERS`, 0.85)
  - `pub(super) fn markers(frame: &Frame, kind: MarkerKind) -> Result<Vec<Point>, ActionError>`
  - `pub(super) fn explore(driver: &Driver<'_>, frame: Frame, origin: MapPoint, kind: MarkerKind) -> Result<Vec<MapPoint>, ActionError>`
  - `pub(super) fn go_to(driver: &Driver<'_>, frame: Frame, origin: MapPoint, target: MapPoint, kind: MarkerKind) -> Result<Option<(Point, Frame)>, ActionError>`

- [ ] **Step 1:** Signaturen umstellen, Fishing (`fish_map`, Survey-Test, Diagnose) übergibt `map::SPOTS`.
- [ ] **Step 2:** `cargo test` → alle bisherigen Tests PASS (Verhalten unverändert); Clippy sauber.
- [ ] **Step 3:** Commit-OK erfragen; `git commit -m "Let the map search look for any kind of marker"`.

### Task 3: Teleporter-Modul

**Files:**
- Create: `src/afk_journey/fishing/teleports.rs` (`mod teleports;` in `fishing.rs`, `pub fn run_teleports` in `fishing.rs` ruft `teleports::run`)
- Create: `assets/AFKJourney/teleports/*.png` (aus Task 1)

**Interfaces:**
- Consumes: Task 2; `map::{open_map, read_tabs, anchor, origin, close_map, is_map}`, `route::Tab`, `fishing::Driver`.
- Produces:
  - `pub(super) const INACTIVE: map::MarkerKind` (`["teleports/inactive.png"]`, Schwelle aus Task 1)
  - `pub(super) struct Skips` mit `new(max_failures_per_map: u32)`, `fn skip(&mut self, map: &str, at: MapPoint)`, `fn is_skipped(&self, map: &str, at: MapPoint) -> bool` (gleich, wenn Abstand ≤ `map::SAME_SPOT`), `fn map_given_up(&self, map: &str) -> bool` (nach 3 Fehlschlägen)
  - `#[derive(Debug, PartialEq, Eq)] pub(super) enum TeleportPanel { Walk(Point), Unknown }`
  - `pub(super) fn teleport_panel(title: Option<&str>, walk: Option<Point>) -> TeleportPanel`
  - `pub(super) fn walk_to_teleport(driver: &Driver<'_>, size: Point, walk: Point, log: &mut impl FnMut(String)) -> Result<map::Arrival, ActionError>` – tippt, wartet bis `WALK_LIMIT_SECS` auf `ACTIVATED`; sonst bekanntes Popup über `recovery`, `Blocked`
  - `pub(super) fn run(serial: &str, cancelled: &AtomicBool, log: &mut impl FnMut(String), report: &mut dyn FnMut(ActivityEvent)) -> Result<u32, ActionError>`

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn skips_end_every_map() {
    let mut skips = Skips::new(3);
    skips.skip("novaris", (100, 200));
    assert!(skips.is_skipped("novaris", (140, 230)));
    assert!(!skips.is_skipped("refinery", (100, 200)));
    skips.skip("novaris", (900, 900));
    assert!(!skips.map_given_up("novaris"));
    skips.skip("novaris", (1500, 0));
    assert!(skips.map_given_up("novaris"));
}

#[test]
fn only_a_teleport_panel_with_a_walk_button_is_used() {
    assert_eq!(teleport_panel(None, Some((500, 900))), TeleportPanel::Unknown);
    assert_eq!(teleport_panel(Some("Teleport"), None), TeleportPanel::Unknown);
    assert_eq!(teleport_panel(Some("Teleport"), Some((500, 900))), TeleportPanel::Walk((500, 900)));
}
```

Titeltext aus Task 1 einsetzen, falls er nicht „Teleport“ enthält.

- [ ] **Step 2:** `cargo test teleports` → FAIL (nicht definiert).
- [ ] **Step 3:** Umsetzen. `run` folgt `fishing::run`/`fish_map`: Karte öffnen, Reiter lesen; je Reiter Log `teleports-searching-map`, `anchor`, `explore(…, INACTIVE)`; je Punkt (nicht gesperrt, Karte nicht aufgegeben): Karte neu öffnen, `anchor`, `go_to(…, INACTIVE)`, antippen, Fenster lesen, `walk_to_teleport`; `Arrived` → `report(ActivityEvent::Collected(GameMode::Teleports, 1))`, Log `teleports-activated`, Pfeil-Bezug = Punkt; `Blocked`/`Unknown` → `skips.skip`, Log `teleports-skipped`, Rest der Karte neu absuchen (`explore` ab frischem `anchor`). Ende: Log `teleports-none-left`, `Ok(anzahl)`.
- [ ] **Step 4:** Test mit Vorlagen: ein ignorierter Test `captured_teleport_icons` (env `ADBHELPER_TEST_TELEPORTS`, private Aufnahmen) prüft `markers(frame, INACTIVE)` findet die inaktiven und keinen aktiven Teleporter.
- [ ] **Step 5:** `cargo test` PASS, Clippy sauber.

### Task 4: Aktion in der App

**Files:**
- Modify: `src/activity.rs` (`GameMode::Teleports`, Label `tr!("activity-teleports")`, in der Liste neben `Fishing`), `src/routine.rs` (`StageAction::Teleports`, id `27`, Label), `src/app/mod.rs` (Zuordnung zu `GameMode::Teleports`), `src/app/run.rs` (Aufruf `crate::afk_journey::fishing::run_teleports`), `src/app/dashboard.rs` (Karte nach Fishing, Zähler `tr!("app-dashboard-activated")`), `src/app/action_settings.rs` (Beschreibung), `locales/en.ftl`, `locales/de.ftl`.

Texte (en / de):
- `activity-teleports` = Teleports / Teleporter
- `app-dashboard-activate-teleports` = Walks to every inactive teleport on the maps / Läuft alle inaktiven Teleporter auf den Karten ab
- `app-dashboard-activated` = activated / aktiviert
- `app-action-settings-teleports` = Searches the maps in the map strip for teleports that are not active yet and walks to each one. Start it anywhere in the game world. / Sucht auf den Karten der Kartenleiste nach Teleportern, die noch nicht aktiv sind, und läuft zu jedem hin. Starte sie irgendwo in der Spielwelt.
- `teleports-searching-map` = Searching { $map } for inactive teleports. / Suche inaktive Teleporter auf { $map }.
- `teleports-activated` = Teleport activated. / Teleporter aktiviert.
- `teleports-skipped` = A teleport could not be reached and is skipped for this run. / Ein Teleporter war nicht erreichbar und wird in diesem Lauf übersprungen.
- `teleports-none-left` = No inactive teleports are left. / Keine inaktiven Teleporter mehr übrig.

- [ ] **Step 1:** `cargo test localization` → PASS (gleiche Schlüssel en/de, jeder `tr!` vorhanden), danach alle Tests PASS, Clippy sauber.
- [ ] **Step 2:** App starten (Release), Karte „Teleports“ im Dashboard und Text in den Aktionseinstellungen sichtbar (Bildschirmfoto nur nach Go).

### Task 5: Emulator-Lauf, Abschluss

- [ ] **Step 1:** Go erfragen; ignorierter Test `connected_emulator_teleport_run` (wie `connected_emulator_fishing_run`) mit `ADBHELPER_FISHING_TRACE` im Release laufen lassen; Ergebnis (Karten, gefundene, aktivierte, übersprungene Teleporter, Dauer) berichten. Fehler per systematic-debugging, jeder weitere Lauf braucht ein Go.
- [ ] **Step 2:** Nach Go einen kurzen Angel-Lauf, um Task 2 am Gerät zu bestätigen.
- [ ] **Step 3:** Aufnahmen/Traces im Scratchpad löschen; Vault-Notiz `D:\Vault\Vault\Projekte\ADBHelper\2026-11-22 Teleporter.md` (geändert / warum / offen).
- [ ] **Step 4:** Commit-OK erfragen; `git add src locales assets/AFKJourney/teleports docs/superpowers` und `git commit -m "Add the Teleports action: walk to every inactive teleport"`.
