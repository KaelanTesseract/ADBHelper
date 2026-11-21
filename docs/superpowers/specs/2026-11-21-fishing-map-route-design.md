# Angeln: Karte und Laufweg – Design

Stand: 2026-11-21. Baut auf `fix/fishing` auf (Minispiel läuft zuverlässig am
offenen Angel-Bildschirm, Lauf 8: 10 Fänge in 11 Würfen).

## Absicht

Heute startet „Fishing“ nur, wenn der Spieler den Angel-Bildschirm selbst
geöffnet hat, und angelt genau diese eine Stelle. Ziel: Nach einem Start
angelt ADBHelper selbstständig **alle Angelstellen mit verbleibenden
Versuchen** auf den Karten der Kartenleiste und hört auf, wenn keine mehr
übrig sind. Kein Benutzer soll dafür etwas über Karten oder Stellen wissen
müssen.

## Ziele

- Karten der unteren Kartenleiste der Weltkarte (z. B. Arcane Affairs,
  Novaris, Refinery, Mortimer …) nacheinander durchsuchen. Die Karte wird
  gewechselt, indem man auf ihren Reiter tippt.
- Gesperrte Reiter (Schloss-Symbol) überspringen.
- Pro Karte ganz herauszoomen und die Karte in Rasterschritten verschieben,
  bis jeder Ausschnitt einmal gesehen wurde. Angelstellen-Marker sind auch
  maximal herausgezoomt sichtbar; die Karte ist dann weiterhin verschiebbar
  (Aussage des Benutzers).
- Jeden gefundenen Marker antippen, im Stellenfenster den Zähler lesen; bei
  verbleibenden Versuchen „Hinlaufen“ tippen, nach Ankunft den Angel-Knopf
  tippen und das vorhandene Minispiel (`fish_spot`) laufen lassen.
- Danach zurück zur Karte und weitersuchen.
- Startet der Benutzer schon im Angel-Bildschirm oder steht die Figur an
  einer Stelle (Angel-Knopf sichtbar), wird dort zuerst geangelt.

## Nicht-Ziele

- Keine Suche auf „Starter Story“, „Veil of Memories“, „Peaks of Time“ oder
  anderen Bereichen außerhalb der Kartenleiste.
- Keine gespeicherte Liste von Stellen-Positionen (bricht bei Spiel-Updates).
- Keine Teleport- oder Reiseplanung über das Antippen von Reiter und Marker
  hinaus.
- Kein Ändern am Minispiel selbst.

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es „funktioniert“)

- ADBHelper tippt Dinge auf der Karte an, die keine Angelstellen sind, und
  löst damit Laufwege, Kämpfe oder Dialoge aus.
- Ein Wischen wird vom Spiel als Tippen gewertet.
- ADBHelper bleibt in einer Schleife hängen (immer dieselbe Stelle, ewiges
  Wischen) statt mit klarer Meldung aufzuhören.
- Aufnahmen der Karte landen im öffentlichen Repo (nur `.update-test` bzw.
  Scratchpad, siehe Regel zu privaten Aufnahmen).

## Rahmenbedingungen

- Rust/egui wie bisher, Erkennung über Vorlagen (NCC, 1080×1920-Basis) und
  Windows-OCR. Eingaben über `adb::tap_game_point`, `adb::swipe_game_points`.
- Neue Oberflächentexte nur über `tr!` mit en- und de-Eintrag.
- Clippy `-D warnings`, `cargo fmt`, alle Tests grün.
- Jeder Emulator-Lauf nur nach Go des Benutzers; ein Go gilt einmal.
- Die WIP-Warnung beim Start von Fishing bleibt.

## Ansätze

1. **Karte rasterweise absuchen (gewählt).** Pro Karte herauszoomen, in eine
   Ecke schieben, dann schlangenförmig in festen Schritten verschieben und in
   jedem Ausschnitt nach Markern suchen. Braucht keine gespeicherten
   Positionen, übersteht Spiel-Updates und neue Karten. Kostet etwas Zeit
   pro Karte.
2. **Nur herauszoomen.** Ein Blick pro Karte. Verworfen: Laut Benutzer passt
   die Karte auch maximal herausgezoomt nicht ganz auf den Bildschirm.
3. **Feste Positionsliste je Karte.** Am schnellsten, aber bricht bei jedem
   Update und muss gepflegt werden. Verworfen.

Der Benutzer hat bestätigt: Kartenwechsel per Reiter, nur die Karten der
Leiste, Marker auch herausgezoomt sichtbar, Karte dann trotzdem verschiebbar.
Damit bleibt Ansatz 1.

## Ablauf

1. **Einstieg.** Spiel muss laufen (wie heute). Ist der Angel-Bildschirm
   offen → zuerst dort angeln. Ist in der Welt der Angel-Knopf zu sehen
   (`RODS` in `ARRIVAL`) → antippen, angeln. Danach bzw. sonst: Karte öffnen
   (`open_map`, vorhandener Code).
2. **Kartenleiste lesen.** Reiter der unteren Leiste erkennen (Name per OCR,
   Schloss per Vorlage), gesperrte verwerfen. Reihenfolge: von links nach
   rechts. Falls die Leiste selbst seitlich verschiebbar ist, gehört das
   Verschieben der Leiste dazu.
3. **Pro Karte:**
   - Reiter antippen, warten bis die Karte gewechselt hat.
   - Ganz herauszoomen (Minus-Knopf, bis sich das Bild nicht mehr ändert).
   - In die obere linke Ecke schieben (wischen, bis sich das Bild nicht mehr
     ändert).
   - Schlangenförmig absuchen: Schrittweite so, dass sich Ausschnitte
     überlappen und kein Marker im Randbereich verloren geht. Rand erkannt,
     wenn ein Wischen das Bild kaum noch verändert.
   - In jedem Ausschnitt alle Marker (`next_marker`) antippen und das
     Stellenfenster lesen (`walking_button`). Bei 0 Versuchen: Fenster
     schließen, Marker für diesen Ausschnitt maskieren, weiter.
   - Bei Versuchen > 0: „Hinlaufen“, Ankunft abwarten (Angel-Knopf, bis 65 s),
     angeln, danach Karte neu öffnen, denselben Reiter wählen und die Suche
     dieser Karte von vorn beginnen (die Position der Karte ist nach dem
     Laufen eine andere; leere Stellen kosten nur ein kurzes Antippen).
4. **Ende.** Wenn jede Karte einmal komplett abgesucht wurde, ohne eine Stelle
   mit Versuchen zu finden: Meldung „keine Angelstellen mit Versuchen mehr“,
   Rückgabe der Fangzahl.

## Schutz vor Hängern

- Obergrenzen: Wischschritte pro Karte, Stellenbesuche pro Lauf, Laufzeit
  bis Ankunft (65 s wie bisher).
- Eine Stelle, die nach dem Hinlaufen keinen Angel-Knopf zeigt oder beim
  Angeln sofort 0 Versuche meldet, wird für den Lauf gesperrt
  (Bildschirmposition reicht nicht, da sich die Karte verschiebt; Merkmal
  z. B. Kartenname + Stellenname/-text aus dem Fenster).
- Jede Kartenaktion prüft, ob noch die Weltkarte zu sehen ist
  („Exploration Progress“); sonst Abbruch mit klarer Meldung.

## Diagnose

`ADBHELPER_FISHING_TRACE` zeichnet auch die Kartenschritte auf (Reiter,
Wischen, Marker, Zählerwert, Entscheidung). Aufnahmen bleiben privat.

## Vorarbeit: Aufnahme der Karte (nach Go)

Vor dem Bauen eine kurze, private Aufnahme am Emulator, um festzulegen:

- Lage und Aussehen der Kartenleiste, ob sie seitlich verschiebbar ist,
  Aussehen eines gesperrten Reiters.
- Wie weit man herauszoomen kann, ob der Minus-Knopf an der bekannten Stelle
  (1020,1223) liegt und wie man das Ende des Zooms erkennt.
- Wie weit ein Wischen die Karte verschiebt (Weg pro Pixel, Trägheit) und
  welche Wischdauer kein Tippen auslöst.
- Ob die Marker-Vorlagen herausgezoomt noch greifen oder neue Vorlagen
  (z. B. `map_marker_small`) nötig sind.
- Was ein Tippen auf leere Kartenfläche auslöst.
- Ein Kartenwechsel per Reiter (die Figur ist danach auf der neuen Karte,
  Aussage des Benutzers) und ein Hinlaufen zu einer weiter entfernten Stelle,
  um die Laufzeit-Obergrenze festzulegen.

Aus der Aufnahme werden Vorlagen und Konstanten gewonnen; die Bilder selbst
werden nicht eingecheckt.

## Was der Benutzer entschieden hat

- Kartenwechsel durch Antippen des Reiters.
- Nur die Karten der Kartenleiste.
- Rasterweises Absuchen (Marker herausgezoomt sichtbar, Karte verschiebbar).

## Was beim Bauen festgelegt wird (meine Entscheidungen)

- Ziel: alle Stellen mit Versuchen, Ende wenn keine mehr da sind.
- Einstieg an offener Stelle bzw. sichtbarem Angel-Knopf vor der Karte.
- Nach jeder geangelten Stelle Suche der aktuellen Karte von vorn.
- Schrittweite, Überlappung, Zoom-/Rand-Erkennung, Obergrenzen und die
  genaue Sperr-Kennung einer Stelle legt der Builder anhand der Aufnahme
  fest.
- Ob der Live-Stream die ganze Zeit läuft oder nur während des Minispiels.
- Kartenwechsel: Das Antippen des Reiters bringt die Figur auf diese Karte;
  „Hinlaufen“ wird nur innerhalb der aktuellen Karte benutzt. Die
  Laufzeit-Obergrenze wird nach der Aufnahme festgelegt (mindestens 65 s).
- Störung beim Laufen (Kampf, Dialog, Popup): Kommt der Angel-Knopf nicht
  innerhalb der Obergrenze, wird ein bekanntes Popup über die vorhandene
  Wiederherstellung geschlossen, die Stelle für den Lauf gesperrt und mit der
  nächsten weitergemacht. Nichts Unbekanntes wird angetippt.
- Sperr-Kennung: Kartenname plus Text aus dem Stellenfenster, falls dort ein
  eindeutiger Name steht; sonst zusätzlich eine Obergrenze an Besuchen pro
  Karte, damit keine Endlosschleife entsteht.
- Ende: „Keine Stellen mit Versuchen mehr“ ist ein normales Ende
  (`Ok(Fangzahl)` mit Logzeile). Ein Abbruch unterwegs (Karte verschwunden,
  Obergrenze erreicht) ist ein Fehler mit klarer Meldung; bis dahin gefangene
  Fische sind bereits über `ActivityEvent::Collected` gezählt.
- Tests: Die reine Logik (Rasterplanung, Rand-Erkennung per Bildvergleich,
  Lesen von Reitern und Zähler, Sperrliste) bekommt Unit-Tests mit
  künstlich erzeugten Bildern bzw. Daten, ohne Emulator und ohne private
  Aufnahmen. Der Ablauf am echten Spiel wird mit Emulator-Läufen nach Go
  geprüft; dafür gibt es wie beim Minispiel einen ignorierten Test.
- Neue Texte (Kartenwechsel, „keine Stellen mehr“, Fehler) in en/de.
- Die Startprüfung „erst Angel-Bildschirm öffnen“ entfällt; Dashboard- und
  Einstellungstexte von Fishing werden entsprechend angepasst.
