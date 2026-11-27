# Teleporter aktivieren – Design

Stand: 2026-11-22. Baut auf der Kartensuche des Angelns auf (`fix/fishing`,
`src/afk_journey/fishing/map.rs`, `route.rs`).

## Absicht

Teleporter auf den Karten werden erst aktiv, wenn die Figur einmal dort war.
Aktive Teleporter erlauben schnelles Reisen und schnelleres Erkunden der Karte,
auch für spätere Aktionen wie das Angeln. Neue Aktion „Teleporter“: Nach einem
Start sucht ADBHelper auf den Karten der Kartenleiste alle **inaktiven**
Teleporter, läuft nacheinander hin und hört auf, wenn keiner mehr übrig ist.

## Ziele

- Alle freien Reiter der Kartenleiste (wie beim Angeln, gesperrte Reiter
  überspringen) nacheinander absuchen.
- Pro Karte ganz herauszoomen und Ausschnitt für Ausschnitt nach dem Symbol
  inaktiver Teleporter suchen (Bild vom Benutzer: grauer Obelisk).
- Jeden gefundenen inaktiven Teleporter über den Ortspfeil wiederfinden,
  antippen, im Fenster den Lauf-Knopf tippen, Ankunft abwarten, prüfen, dass er
  jetzt aktiv ist.
- Ende mit der Zahl neu aktivierter Teleporter.

## Nicht-Ziele

- Keine Karten außerhalb der Kartenleiste.
- Keine gespeicherten Teleporter-Positionen.
- Kein Erkunden von Nebel/Kartenrand über das Ablaufen der Teleporter hinaus.
- Keine Änderung am Angeln.

## Anti-Ziele

- Antippen von Dingen, die keine Teleporter sind (Kämpfe, Dialoge, Quests).
- Teleportieren statt Hinlaufen, wenn dadurch nichts aktiviert wird.
- Hängenbleiben an einem unerreichbaren Teleporter.
- Aufnahmen landen im öffentlichen Repo.

## Rahmenbedingungen

- Rust/egui, Vorlagen (NCC, 1080×1920) und Windows-OCR wie bisher.
- Neue Texte nur über `tr!` mit en/de.
- Clippy `-D warnings`, `cargo fmt`, alle Tests grün.
- Jeder Emulator-Lauf nur nach Go; Emulator-Tests als Release-Build.
- Neue Aktion in Dashboard, Aktionseinstellungen, Routine und Aktivitätszähler
  wie die vorhandenen Aktionen.

## Ansätze

1. **Karte rasterweise absuchen wie beim Angeln (gewählt).** Nutzt
   `open_map`, `read_tabs`, `anchor`, `explore`, `go_to`, `close_map`. Übersteht
   Updates, kostet etwa eine Minute pro Karte.
2. **Feste Positionsliste je Karte.** Schnell, bricht bei Updates. Verworfen.
3. **Nur in der Spielwelt um die Figur suchen.** Findet nur Nahes. Verworfen.

Der Benutzer hat Weg 1 gewählt.

## Ablauf

1. Spiel muss laufen. Karte öffnen, Reiter lesen.
2. Pro Reiter: `anchor`, dann `explore` mit der Teleporter-Vorlage statt der
   Angelstellen-Vorlage → Liste von Kartenpunkten.
3. Pro Punkt: Karte neu öffnen, `anchor`, `go_to`, Symbol antippen, Fenster
   lesen, Lauf-Knopf tippen, Ankunft abwarten, Ortspfeil-Bezug auf diesen Punkt
   setzen (wie beim Angeln nach dem Laufen).
4. Ende: Meldung „Keine inaktiven Teleporter mehr“ und Rückgabe der Anzahl.

## Was eine Aufnahme klären muss (nach Go)

- Aussehen eines inaktiven und eines aktiven Teleporters auf der ganz
  herausgezoomten Karte (Vorlage, Unterscheidung).
- Das Fenster nach Antippen eines Teleporters: Titel, Lauf-Knopf (gleiches
  Symbol wie bei Angelstellen?), ob ein inaktiver Teleporter „Teleport“
  anbietet.
- Woran man die Ankunft erkennt (Meldung „Teleporter aktiviert“, Symbol wird
  farbig, Figur steht still).
- Laufzeit zu einem weiter entfernten Teleporter (Obergrenze).

## Was beim Bauen festgelegt wird (meine Entscheidungen)

- Gemeinsamer Kartenteil: `explore`/`go_to` bekommen die Marker-Vorlagen als
  Parameter, damit Angeln und Teleporter denselben Code nutzen; das
  Angel-Verhalten bleibt gleich (Tests sichern das).
- Unerreichbar (Ankunft nicht innerhalb der Obergrenze, kein Lauf-Knopf,
  Fenster unbekannt): überspringen, Meldung im Protokoll, weiter. Nach einem
  Fehlschlag wird der Rest der Karte neu abgesucht statt über den Pfeil
  weitergerechnet (wo die Figur steht, ist unbekannt).
- Obergrenzen: Ausschnitte pro Karte (wie Angeln), Läufe pro Karte, Laufzeit.
- Aktivitätszähler zählt aktivierte Teleporter; neue Texte „Suche Teleporter
  auf …“, „Teleporter aktiviert“, „Keine inaktiven Teleporter mehr“,
  „Teleporter nicht erreichbar, übersprungen“ in en/de.
- Tests: reine Logik mit erzeugten Bildern/Daten, Ablauf über einen
  ignorierten Emulator-Test.
- Kartenwechsel: Antippen eines Reiters bringt die Figur auf diese Karte
  („Going to Novaris“, im Angel-Lauf beobachtet); der Ortspfeil ist auf jeder
  Karte sichtbar. Hinlaufen findet daher immer innerhalb der Karte statt, auf
  der die Figur gerade ist. Teleportieren zu aktiven Teleportern wird nicht
  benutzt.
- Übersprungene Teleporter: Sperrliste pro Lauf als Kartenpunkte relativ zum
  Ortspfeil-Bezug der Karte; nach höchstens 3 Fehlschlägen pro Karte geht es mit
  der nächsten Karte weiter. So endet jede Schleife. Ein Ende mit übersprungenen
  Teleportern ist ein normales Ende (`Ok(Anzahl)`), die Übersprungenen stehen im
  Protokoll.
- Störungen beim Laufen (Kampf, Dialog, Quest): nichts davon wird angetippt.
  Kommt die Ankunft nicht innerhalb der Obergrenze, wird nur ein bekanntes Popup
  über die vorhandene Wiederherstellung geschlossen und der Teleporter
  übersprungen.
- Vorlage: Das Bild des Benutzers zeigt das Symbol, ist aber kein Ausschnitt im
  Maßstab der herausgezoomten Karte. Die Aufnahme kommt deshalb vor dem Bauen;
  Vorlagen werden daraus geschnitten, die Bilder bleiben privat.
- Routine: Die Aktion ist standardmäßig aus und für gelegentliche Läufe
  gedacht; „keine inaktiven Teleporter mehr“ ist ein normales Ende mit 0, kein
  Fehler. Platz in der Routine nach Fishing.
