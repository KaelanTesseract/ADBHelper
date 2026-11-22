# Angeln: Karte und Laufweg – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fishing sucht selbstständig alle Angelstellen mit Versuchen auf den Karten der Kartenleiste, läuft hin und angelt dort.

**Architecture:** Reine, testbare Logik (Rasterweg, Bewegungs-Erkennung, Reiter lesen, Sperrliste) kommt in `src/afk_journey/fishing/route.rs`. Das Steuern der Karte am Gerät (Zoom, Wischen, Reiter, Marker, Hinlaufen) kommt in `src/afk_journey/fishing/map.rs` und nutzt den privaten `Driver` aus `fishing.rs` über `super::`. `run()` in `fishing.rs` verbindet Einstieg, Kartenschleife und das unveränderte Minispiel `fish_spot`.

**Tech Stack:** Rust 2021, `image`, Vorlagen-Abgleich (`vision::get_match`, eingebettet per `build.rs`), Windows-OCR (`ocr::recognize`), ADB-Eingaben (`adb::tap_game_point`, `adb::swipe_game_points`).

**Spec:** `docs/superpowers/specs/2026-11-21-fishing-map-route-design.md`

## Global Constraints

- Neue Oberflächentexte nur über `tr!`, jeder Schlüssel in `locales/en.ftl` und `locales/de.ftl` (Test `english_and_german_have_the_same_ids_and_placeholders`).
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` grün vor jedem Commit.
- Jeder Emulator-Lauf nur nach Go des Benutzers; ein Go gilt für genau einen Lauf.
- Aufnahmen und Screenshots nur in `.update-test/` oder im Scratchpad, nie committen. Eingecheckt werden nur kleine Vorlagen-Ausschnitte unter `assets/AFKJourney/fishing/`, wie bei den vorhandenen.
- Commits nur nach OK des Benutzers, nichts pushen.
- Nur Karten der unteren Kartenleiste; gesperrte Reiter überspringen; keine gespeicherten Stellen-Positionen.
- Nichts antippen, was nicht als Marker, Reiter, Zoom-Knopf, Stellenfenster-Knopf oder Angel-Knopf erkannt ist.
- Die WIP-Warnung beim Start von Fishing bleibt.

## Review Focus

- Stellenfenster öffnet sich nicht oder zeigt etwas anderes (Marker war kein Angelmarker): Fenster wird über die bekannte Schließ-Geste geschlossen, Marker in diesem Ausschnitt maskiert, kein weiterer Tipp. → Test in Task 3 (`unknown_panel_masks_marker`).
- Derselbe Marker taucht in zwei überlappenden Ausschnitten auf: zweiter Besuch kostet nur ein Antippen mit 0 Versuchen oder wird über die Sperrliste übersprungen, keine Schleife. → Test in Task 2 (`visit_cap_stops_a_map`).
- Ein Wischen bewegt die Karte nicht, weil ein Popup darüber liegt: Rand-Erkennung darf das nicht als „fertig“ werten, ohne dass `is_map` geprüft wurde. → Task 3 (`prepare`, `find_spot`) prüft `is_map` nach jedem Wischen; Test `sweep_ends_only_after_two_edges` in Task 2.
- Die aktuelle Karte ist schon ausgewählt (hervorgehobener Reiter): Antippen ändert nichts, das darf nicht als Fehler gelten. → Task 3, `open_tab` akzeptiert unverändertes Bild, wenn der Reiter hervorgehoben ist bzw. `is_map` gilt.
- Ein Reiter-Name wird von OCR abgeschnitten („Arcane Affa…“): Reiter werden über Position und Text-Präfix erkannt, nicht über den vollen Namen. → Test in Task 2 (`tabs_keep_clipped_names`).

---

### Task 1: Aufnahme der Karte und Werte festlegen (Vorarbeit, Go nötig)

**Files:**
- Create (privat, nicht committen): `.update-test/fishing-map/` (Bilder), `.update-test/fishing-map/notes.md`
- Create: neue Vorlagen unter `assets/AFKJourney/fishing/` nur falls nötig (`map_tab_lock.png`, ggf. `map_zoom_minus.png`, ggf. neue `map_marker_*.png` für herausgezoomt)
- Modify: dieser Plan, Abschnitt „Werte aus der Aufnahme“ unten

**Interfaces:**
- Produces: die Konstanten-Werte für Task 2 und 3 (Tabelle unten).

- [ ] **Step 1:** Benutzer um Go für eine Aufnahme bitten (Karte öffnen, herauszoomen, wischen, Reiter wechseln, einmal zu einer entfernten Stelle laufen). Ohne Go nicht weiter.
- [ ] **Step 2:** Mit einem ignorierten Test `connected_emulator_map_capture` (in `fishing/map.rs`, env `ADBHELPER_TEST_SERIAL`, Ausgabe nach `ADBHELPER_FISHING_TRACE`) die Schritte fahren und Screenshots speichern. Dieser Test führt nur Zoom, Wischen und Reiter-Tippen aus, keine Marker.
- [ ] **Step 3:** Aus den Bildern die Tabelle „Werte aus der Aufnahme“ füllen und nötige Vorlagen ausschneiden. Prüfen, dass `fishing/map_marker.png`/`map_marker_small.png` herausgezoomt mit ≥ 0.83 greifen; sonst neue Vorlage.
- [ ] **Step 4:** Benutzer kurz berichten, was die Aufnahme gezeigt hat (v. a. ob die Kartenleiste verschiebbar ist und wie man ein Stellenfenster sicher schließt).

**Werte aus der Aufnahme** (vor Task 2 ausfüllen):

| Name | Bedeutung | Wert |
|---|---|---|
| `TAB_STRIP` | Rect der Kartenleiste | `Rect { x: 0, y: 1540, width: 1080, height: 110 }`, Reiter-Mitte y ≈ 1594 |
| `TAB_STRIP_SCROLLS` | Leiste seitlich verschiebbar? | Ja: die Leiste zentriert den gewählten Reiter selbst. Reiter-Positionen nach jedem Wechsel neu lesen. Gesehen: Arcane Affairs, Novaris, Refinery, Mortimer (Schloss). |
| `ZOOM_SLIDER` | Zoom | Regler bei x = 1012, y ≈ 540–1215. Ein Wischen vom Reglerknopf nach (1012, 1230) in 600 ms zoomt ganz heraus. Der Knopf ist der hellste runde Fleck in dieser Spalte. Minus-Knopf (1020, 1223) zoomt pro Tipp nur etwa 3 %. |
| `MAP_VIEW` | Rect der freien Kartenfläche | `Rect { x: 40, y: 115, width: 930, height: 1310 }` ohne linke Seitenknöpfe (`x < 430, y < 560`) und Homestead (`x < 120, y > 1340`). Die linken Knöpfe verdecken Marker, daher Überlappung nötig. |
| `SWIPE_MS` | Wischdauer | 700 ms (bei 500 ms gingen zwei Wischer verloren) |
| `STEP_X`, `STEP_Y` | Wischweg pro Rasterschritt | Finger 600 px → Karte ≈ 550 px, keine Nachlaufbewegung. Waagerecht (250↔850, y 900), senkrecht (540, 500↔1100). |
| `EMPTY_VIEW` | Rand-Erkennung | **Ersetzt `MOVE_THRESHOLD`:** Die Karte lässt sich weit ins leere Dunkel schieben, „bewegt sich nicht“ ist kein Rand. Rand = Anteil Pixel mit Grauwert > 60 in `MAP_VIEW` (ohne Knöpfe) < 0,005. Gemessen: leer 0,001–0,003, Kartenrand 0,009–0,02, Inhalt 0,07–0,22. |
| `MARKER_FAR` | Marker herausgezoomt | Neue Vorlage `fishing/map_marker_far.png` (32×32, Fisch auf Türkis). Trefferwert 0,94; höchster Fehltreffer 0,69 → Schwelle 0,85. Alte Vorlagen greifen herausgezoomt nicht (≤ 0,56). |
| `IS_MAP` | Weltkarte erkennen | OCR „Exploration Progress“ scheitert auf unruhigem Hintergrund (2 von 5 Bildern). Neue Vorlage aus der festen unteren Leiste (Zurück-Pfeil bzw. „Switch“) nötig. |
| `PANEL` | Stellenfenster | Tippen auf einen Marker **zoomt die Karte hinein** und öffnet z. B. „Lucky Spot“ mit Zähler „5/5“. Lupe und Lauf-Knopf (Stiefel) stehen oberhalb des Markers; der Marker verschiebt sich dabei leicht (925,888 → 870,885). Nach dem Fenster neu herauszoomen. |
| `PANEL_CLOSE` | Fenster schließen | Nicht aufgenommen. Wird im Such-Testlauf in Task 3 ermittelt. Bis dahin: Karte schließen und neu öffnen (Zurück-Pfeil, dann Minikarte). |
| `TAB_SWITCH` | Reiter-Wechsel | Wechselt nur die Ansicht („Return to World“ erscheint), Zoom bleibt. Auf der Karte der Figur steht „Arrived at …“, und die Ansicht springt zur Figur. Grüner Rahmen = Karte der Figur, Avatar = angezeigte Karte. |
| `WALK_LIMIT_S` | Obergrenze bis Ankunft | 90. Gemessen: Das Spiel teleportiert selbst ins Zielgebiet, Angel-Knopf nach ≈ 12 s in `ARRIVAL` erkannt (0,96). |

---

### Task 2: Reine Routen-Logik

**Files:**
- Create: `src/afk_journey/fishing/route.rs` (mit `#[cfg(test)] mod tests`)
- Modify: `src/afk_journey/fishing.rs` (Zeile mit Modul-Deklarationen: `mod route;`)

**Interfaces:**
- Produces:
  - **Umgesetzt (ersetzt `Sweep`):** `pub(super) struct Explorer` mit `new(max_views)`, `record(empty) -> bool` (true = neuer Ausschnitt mit Karte, nach Markern absuchen) und `next_move() -> Option<Heading>`. Das ist eine Flutfüllung über ein Raster ab der Startansicht: Nachbarn werden besucht, solange Karte zu sehen ist, höchstens ein leerer Ausschnitt wird überbrückt (`MAX_GAP = 2`), Obergrenze `MAX_VIEWS_PER_MAP = 80`. Begründung: Die Ansicht startet bei der Figur, und die Karten sind unregelmäßig; eine Ecke als Start gibt es nicht zuverlässig. Die folgenden `Sweep`-Zeilen und -Tests gelten nicht mehr.
  - `pub(super) enum Heading { Up, Down, Left, Right }`
  - `pub(super) struct Sweep` mit `fn new(max_steps: u32) -> Self` und `fn next(&mut self, moved_last: bool) -> Option<Heading>`; Schlangenweg ab oben links: rechts bis Rand, einmal runter, links bis Rand, runter, …; `None`, wenn ein „runter“ nichts bewegt oder `max_steps` erreicht. Erster Aufruf ignoriert `moved_last`.
  - `pub(super) fn swipe_for(heading: Heading) -> ((u32, u32), (u32, u32))` – Fingerweg in Basis-Koordinaten (Karte nach rechts schauen = Finger nach links), Länge `STEP_X`/`STEP_Y`, innerhalb `MAP_VIEW`.
  - `pub(super) fn is_empty_view(frame: &GrayImage) -> bool` – Anteil Pixel > 60 in `MAP_VIEW` ohne Knopfbereiche < `EMPTY_VIEW` (0,005). **Rand-Regel (statt Bewegung):** `Sweep::next(edge)` bekommt `edge = is_empty_view` des neuen Ausschnitts; bei leerem Ausschnitt einen Schritt zurück und Richtung wechseln. Vor dem Schlangenweg erst nach oben, dann nach links schieben, bis der Ausschnitt leer ist, dann einen Schritt zurück (die Ansicht startet bei der Figur, nicht in der Ecke).
  - `pub(super) struct Tab { pub name: String, pub at: (u32, u32) }`
  - `pub(super) fn map_tabs(lines: &[ocr::TextLine], locks: &[(u32, u32)]) -> Vec<Tab>` – OCR-Zeilen innerhalb `TAB_STRIP`, nach x sortiert, ohne Zeilen mit einem Schloss im selben Reiter (|dx| ≤ 120), Name = `ascii_key(text)`.
  - `pub(super) struct Visits` mit `fn new(cap_per_map: u32)`, `fn block(&mut self, map: &str, spot: &str)`, `fn is_blocked(&self, map: &str, spot: &str) -> bool`, `fn visit(&mut self, map: &str) -> bool` (false, wenn Obergrenze erreicht). `MAX_VISITS_PER_MAP = 20`.

- [ ] **Step 1: Failing tests schreiben**

```rust
#[test]
fn sweep_walks_a_snake_and_ends_at_the_bottom_edge() {
    let mut s = Sweep::new(100);
    assert_eq!(s.next(true), Some(Heading::Right));
    assert_eq!(s.next(true), Some(Heading::Right));
    assert_eq!(s.next(false), Some(Heading::Down)); // rechter Rand
    assert_eq!(s.next(true), Some(Heading::Left));
    assert_eq!(s.next(false), Some(Heading::Down)); // linker Rand
    assert_eq!(s.next(false), None); // unterer Rand
}

#[test]
fn sweep_ends_only_after_two_edges() {
    let mut s = Sweep::new(100);
    s.next(true);
    assert_eq!(s.next(false), Some(Heading::Down), "ein Rand allein beendet nicht");
}

#[test]
fn sweep_stops_at_the_step_cap() {
    let mut s = Sweep::new(3);
    assert!(s.next(true).is_some() && s.next(true).is_some() && s.next(true).is_some());
    assert_eq!(s.next(true), None);
}

#[test]
fn identical_frames_did_not_move_and_shifted_ones_did() {
    // künstliches Streifenmuster 1080x1920, einmal um STEP_X verschoben
    let a = stripes(0); let b = stripes(STEP_X);
    assert!(!moved(&a, &a, MAP_VIEW));
    assert!(moved(&a, &b, MAP_VIEW));
}

#[test]
fn tabs_keep_clipped_names_and_skip_locked_ones() {
    let lines = vec![line("Arcane Affa", 120), line("Novaris", 400), line("Refinery", 680), line("Mortimer", 960)];
    let tabs = map_tabs(&lines, &[(940, TAB_STRIP.y + 40)]);
    assert_eq!(tabs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["arcaneaffa", "novaris", "refinery"]);
}

#[test]
fn visit_cap_stops_a_map() {
    let mut v = Visits::new(2);
    assert!(v.visit("novaris") && v.visit("novaris"));
    assert!(!v.visit("novaris"));
    v.block("novaris", "spot a");
    assert!(v.is_blocked("novaris", "spot a") && !v.is_blocked("refinery", "spot a"));
}
```

(`stripes` und `line` sind kleine Hilfsfunktionen im Testmodul; `line(text, x)` setzt y in die Mitte von `TAB_STRIP`.)

- [ ] **Step 2:** `cargo test fishing::route` → FAIL (Typen fehlen).
- [ ] **Step 3:** Typen und Funktionen wie unter Interfaces in `route.rs` umsetzen, Konstanten aus Task 1 als `const` in `route.rs`.
- [ ] **Step 4:** `cargo test fishing::route` → alle 6 PASS; `cargo clippy --all-targets -- -D warnings` sauber.
- [ ] **Step 5:** Benutzer um Commit-OK bitten; bei OK: `git add src/afk_journey/fishing/route.rs src/afk_journey/fishing.rs` und `git commit -m "Add the pure route logic for the fishing map sweep"`.

---

### Task 3: Karte am Gerät steuern

**Files:**
- Create: `src/afk_journey/fishing/map.rs`
- Modify: `src/afk_journey/fishing.rs` – `mod map;`; `Driver::swipe`; `is_map`, `open_map`, `next_marker`, `walking_button`, `zero_attempts`, `ARRIVAL`, `MAP_MARKERS` werden `pub(super)` bzw. nach `map.rs` verschoben, alle `#[allow(dead_code)]` und „Parked“-Kommentare entfallen; `approach_spot` wird durch `map::walk_to` ersetzt und gelöscht.

**Interfaces:**
- Consumes: alles aus Task 2.
- Produces:
  - `impl Driver { fn swipe(&self, size: Point, from: Point, to: Point) -> Result<(), ActionError> }` (Dauer `SWIPE_MS`, danach `settle`).
  - `pub(super) fn read_tabs(driver: &Driver<'_>) -> Result<Vec<route::Tab>, ActionError>` (inkl. Verschieben der Leiste, falls `TAB_STRIP_SCROLLS`).
  - `pub(super) fn open_tab(driver: &Driver<'_>, tab: &route::Tab) -> Result<Frame, ActionError>` – tippt, wartet bis `is_map`; Fehler `fishing-could-not-verify-the-world-map` sonst.
  - `pub(super) fn prepare(driver: &Driver<'_>, map: Frame) -> Result<Frame, ActionError>` – Minus-Knopf bis `!moved` (max 8), dann Richtung oben links wischen bis zweimal `!moved` (max 30), nach jedem Schritt `is_map` prüfen.
  - `pub(super) enum Found { Spot { walk: Point, key: String }, MapDone }`
  - `pub(super) fn find_spot(driver: &Driver<'_>, map_name: &str, visits: &mut route::Visits) -> Result<Found, ActionError>` – `Sweep` mit max 60 Schritten; je Ausschnitt `next_marker` mit Maske, Marker antippen, `walking_button` lesen; Schlüssel = Kartenname + `ascii_key` des Fenstertitels/Zählers; 0 Versuche, gesperrt oder unbekanntes Fenster → `PANEL_CLOSE`, Marker maskieren, weiter.
  - `pub(super) enum Arrival { Arrived, Blocked }`
  - `pub(super) fn walk_to(driver: &Driver<'_>, walk: Point, log: &mut impl FnMut(String)) -> Result<Arrival, ActionError>` – tippt „Hinlaufen“, wartet bis `WALK_LIMIT_S` auf `RODS` in `ARRIVAL`, tippt ihn bei Ankunft; sonst bekannte Popups über `recovery` schließen und `Blocked`.
  - Jeder Kartenschritt (Reiter, Zoom, Wischen mit `moved`-Ergebnis, Marker, Zählertext, Entscheidung, Ankunft) schreibt eine Zeile über `trace::note`, Bilder über `trace::frame`.

**Umgesetzt abweichend (nach Such-Testläufen 1–6):**
- Statt `prepare`/`find_spot`: `anchor` (Reiter tippen, ganz herauszoomen, Ortspfeil `map_arrow.png` suchen) und `explore` (Flutfüllung über Ausschnitte, nur zu Seiten, an denen die Karte den Rand berührt; Wege über bekannte Kartenstücke). Stellen werden als Kartenpunkte relativ zum Ortspfeil gemerkt und mit `go_to` wieder angefahren (Treffer im Lauf 6: 68–83 px Abweichung).
- Kartenrand: Manche Karten (Refinery) laufen nicht ins Dunkle aus, sondern bleiben stehen. `route::moved` (mittlere Helligkeitsänderung < 5) erkennt das; `Explorer::stuck` geht zurück, `go_to` korrigiert die erwartete Position.
- „Return to World“ unten rechts zählt nicht als Kartenfläche. Nach dem Schließen einer fremden Karte tippt `close_map` „Return to World“ und bestätigt „Leave the current map and return to the entrance?“ (Haken 865,1270). `open_map` beantwortet vorher eine offene Beenden-Frage mit „Nein“.
- Stellenfenster: Zähler ist herausgezoomt nicht lesbar; `PANEL_CLOSE` = Karte schließen und neu öffnen.
- Ignorierter Test heißt `connected_emulator_map_survey`.

- [ ] **Step 1: Failing test für die einzige reine Entscheidung schreiben** – die Fenster-Auswertung wird als `fn panel_decision(title: Option<&str>, counter: Option<&str>, walk: Option<Point>) -> PanelDecision` (`Walk(Point)`, `Empty`, `Unknown`) aus `walking_button` herausgelöst:

```rust
#[test]
fn unknown_panel_masks_marker() {
    assert_eq!(panel_decision(None, None, None), PanelDecision::Unknown);
    assert_eq!(panel_decision(Some("Fishing Spot 0/10"), None, Some((500, 900))), PanelDecision::Empty);
    assert_eq!(panel_decision(None, Some("3/10"), Some((500, 900))), PanelDecision::Walk((500, 900)));
    assert_eq!(panel_decision(None, Some("3/10"), None), PanelDecision::Unknown, "ohne Knopf nichts antippen");
}
```

- [ ] **Step 2:** `cargo test fishing::map` → FAIL.
- [ ] **Step 3:** `panel_decision` und die Funktionen aus Interfaces umsetzen; `walking_button` nutzt `panel_decision`.
- [ ] **Step 4:** `cargo test` (alles) PASS, Clippy sauber; die vorhandenen ignorierten Tests kompilieren weiter.
- [ ] **Step 5:** Ignorierten Test `connected_emulator_map_sweep` anlegen: öffnet die Karte, liest Reiter, `prepare` + `find_spot` je Karte, **läuft nicht hin**, schreibt gefundene Stellen und Zähler in den Trace. Benutzer um Go bitten, einmal laufen lassen, Trace mit dem Benutzer abgleichen (stimmen gefundene Stellen mit dem Spiel überein?). Trace danach im Scratchpad löschen.
- [ ] **Step 6:** Commit-OK erfragen; bei OK: `git add src/afk_journey/fishing/map.rs src/afk_journey/fishing.rs assets/AFKJourney/fishing/*.png` und `git commit -m "Drive the fishing map: tabs, zoom, sweep and walking"`.

---

### Task 4: Einstieg, Kartenschleife, Texte

**Files:**
- Modify: `src/afk_journey/fishing.rs` (`run`), `locales/en.ftl`, `locales/de.ftl`

**Interfaces:**
- Consumes: `map::{read_tabs, open_tab, prepare, find_spot, walk_to, Found, Arrival}`, `route::Visits`, `fish_spot`.
- Produces: `pub fn run(...) -> Result<u32, ActionError>` mit unveränderter Signatur.

Ablauf in `run` (Reihenfolge ist die Entscheidung, Code schreibt der Builder):

```text
Spiel läuft? sonst Fehler (vorhandene Prüfung, Text angepasst)
Angel-Bildschirm offen?  -> fish_spot
sonst Angel-Knopf in ARRIVAL? -> antippen, fish_spot
loop:
  open_map; tabs = read_tabs
  found_any = false
  für jeden tab: open_tab; log "fishing-searching-map"(name)
     loop: prepare; match find_spot
        Spot -> visits.visit(map) sonst break; walk_to
                Arrived -> fish_spot (Stream starten, falls nicht aktiv); found_any = true; open_map; open_tab
                Blocked -> visits.block(map, key); open_map; open_tab
        MapDone -> break
  wenn !found_any: log "fishing-no-spots-with-attempts-left"; return Ok(count)
```

Der Live-Stream läuft nur während `fish_spot` (vor jedem Aufruf starten, danach fallen lassen), damit Kartenschritte normale Screenshots nutzen.

- [ ] **Step 1:** Neue Texte in en/de: `fishing-searching-map = Searching { $map } for fishing spots.` / `Suche Angelstellen auf { $map }.`; `fishing-no-spots-with-attempts-left = No fishing spots with attempts are left.` / `Keine Angelstellen mit Versuchen mehr übrig.`; `fishing-spot-skipped = A fishing spot could not be reached and is skipped for this run.` / `Eine Angelstelle war nicht erreichbar und wird in diesem Lauf übersprungen.`. Geänderte Texte: `fishing-open-the-fishing-screen-first` entfällt (Schlüssel löschen), `app-action-settings-uses-available-fishing-spots-on-the` und `app-dashboard-walk-to-fishing-spots-and-fish` beschreiben „sucht alle Angelstellen mit Versuchen auf den Karten und angelt dort“. Unbenutzte Schlüssel (`fishing-no-reachable-fishing-spot-with-attempts`) löschen, falls kein `tr!` mehr darauf zeigt.
- [ ] **Step 2:** `cargo test localization` → PASS (gleiche Schlüssel en/de, jeder `tr!`-Schlüssel vorhanden).
- [ ] **Step 3:** `run` wie oben umsetzen.
- [ ] **Step 4:** `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` → alles PASS.
- [ ] **Step 5:** Benutzer um Go für einen Volllauf bitten (Start irgendwo in der Welt, nicht im Angel-Bildschirm). Mit `ADBHELPER_FISHING_TRACE` laufen lassen; Ergebnis berichten (Karten, Stellen, Fänge, Abbruchgrund). Fehler über systematic-debugging klären, jeder weitere Lauf braucht ein neues Go.
- [ ] **Step 6:** Traces/Screenshots im Scratchpad löschen; Vault-Notiz `D:\Vault\Vault\Projekte\ADBHelper\2026-11-xx Angeln Karte und Laufweg.md` (geändert / warum / offen) und Projektnotiz aktualisieren.
- [ ] **Step 7:** Commit-OK erfragen; bei OK: `git add src/afk_journey/fishing.rs locales/en.ftl locales/de.ftl docs/superpowers` und `git commit -m "Fishing finds and walks to every spot with attempts on the map tabs"`.
