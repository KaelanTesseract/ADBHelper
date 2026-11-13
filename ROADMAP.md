# ADBHelper – AFK-Journey-Roadmap

Vergleich mit [ADB AutoPlayer, Stand 10. November 2026](https://github.com/AdbAutoPlayer/AdbAutoPlayer/tree/832fa24c8daa6aab62ca425fce1b9569b212eff2/src-tauri/src-python/adb_auto_player/games/afk_journey). Diese Liste nach jeder Umsetzung aktualisieren. Einen Punkt nach der lokalen Implementierung abhaken und ergänzen, wie er geprüft wurde. Veröffentlichungen separat vermerken.

## 1. Daily-Belohnungen

- [x] Freundesbelohnungen über Send & Receive
- [x] Postbelohnungen über Read All
- [x] Battle-Pass-Belohnungen (Noble Path)
- [x] Daily-Questbelohnungen einschließlich verfügbarer Meilensteine
- [x] Gildenquestbelohnungen

Alle fünf Schritte lassen sich einzeln im Daily-Editor hinzufügen. Im Emulator wurden Belohnungen für Noble Path, Daily Quests samt Meilensteinen und Gildenquests abgeholt. Für Send & Receive und Read All wurde die Erkennung der Bestätigung mit Emulator-Aufnahmen geprüft. In einem zweiten Emulatorlauf gingen alle fünf Schritte ohne verfügbare Belohnungen regulär weiter. Der Stand ist im lokalen Testbuild verfügbar.

Der Wechsel von Season Noble Path II zu Guild Quests nutzt den sichtbaren Zurück-Pfeil. Wenn das Spiel zur Weltansicht zurückkehrt, öffnet ADBHelper das Hauptmenü. Die Erkennung der Weltansicht wurde mit einer Aufnahme geprüft. Der Wechsel und beide aufeinanderfolgenden Daily-Schritte liefen im Emulator durch.

## 2. Arcane Labyrinth

- [ ] Einen vollständigen Lauf mit der vorhandenen Aufstellung abschließen.
- [ ] Wiederholungen bis zum eingestellten Ziel für Lucky-Flip-Schlüssel ergänzen.

## 3. Homestead

- [ ] Homestead Orders Helper: Minenressourcen abholen und lieferbare Aufträge abschließen.
- [ ] Fehlende Materialien herstellen. Stückzahl oder Ausdauer als Stopplimit anbieten.

## 4. Helden und Einkäufe

- [x] Collect Affinity
- [x] Level Up All Heroes
- [x] Equip New Equipment
- [x] Emporium für rabattierte Invite Letter, Affinitätsartikel und heldengebundene Temporal Essences
- [x] Single Pull in der Noble Tavern
- [x] Essence Swap

Alle sechs Aktionen sind optional im Daily-Editor verfügbar. Emporium prüft Produkt, Preis und Währung gegen getrennte Budgets pro Lauf. Die Budgets starten bei null. Single Pull verwendet einen vorhandenen Invite Letter. Essence Swap verwendet bestätigte Essenzkäufe aus demselben Lauf.

Die Invite-Letter-Einstellung ist als Diamantenbudget beschriftet. Die Daily-Kachel zeigt Preis und Budget, wenn das Angebot das Limit überschreitet. Mit einer Emulator-Aufnahme wurde geprüft, dass ein Angebot über 210 Diamanten bei 1 und 209 abgelehnt wird und bei 210 die Budgetprüfung besteht.

Die Kaufbestätigung liest den Diamantenstand im Guild Store aus einem engeren Ausschnitt. Aufnahmen des Kaufs bestätigten die Abbuchung von 210 Diamanten (157.976 → 157.766) und das Überspringen des ausverkauften Angebots.

Collect Affinity tippt pro Held zweimal in die Bildschirmmitte. Im Emulator stieg Sinbads Affinität mit ausgerüstetem Skin um zwei Punkte. Die Aktion erkennt auch Profile auf Maximalstufe wie Lily Mays Profil und vergleicht beim Wechsel den gerenderten Namenszug in zwei aufeinanderfolgenden Aufnahmen. Bei stark wechselndem Hintergrund kann der Titel des Helden den Vergleich zusätzlich bestätigen. Nach dem Schließen der Modellansicht bestätigen die Profilbedienelemente die Rückkehr, auch wenn die Animation den Namenszug verdeckt.

Im Emulator wurden die ersten 98 Helden und nach der ergänzten Titelerkennung die letzten 32 von Cryonaia bis Chippy geprüft. Mit der Rückkehrprüfung anhand der Bedienelemente lief der erneute Test über alle 47 Profile von Dunlingr bis Chippy. Aufnahmen von Lily May, Lyca, Baelran, Cryonaia, Sinbad und Dunlingr decken die gemeldeten Profilvarianten ab.

Equip New Equipment erkennt die Season-Equipment-Übersicht anhand der Klassensymbole und bestätigt Quick Equip auch über die Meldung „No equipment to replace“. Im Emulator wurden alle sechs Klassen geöffnet. Der abschließende Lauf bestätigte drei Ausrüstungswechsel und der Wiederholungslauf meldete keinen weiteren Wechsel.

Wenn die Vollbilderkennung die Klassenzeile überspringt, liest Equip New Equipment die Überschrift des Ausrüstungspanels separat. Beim erneuten Emulator-Test wurde der Ausrüstungswechsel für Warrior bestätigt. Der Wiederholungslauf endete ohne weitere Änderung.

Die Live-Vorschau bestätigte die Navigation zur Resonanzhalle, den Aufwertungsbutton mit lesbarer Resonanzstufe, Helden und ihre exklusive Ausrüstung, alle drei Shopangebote sowie All-Hero Recruitment mit vorhandenem Invite Letter. Ein vollständiger Live-Test der Aufwertungen, Käufe, Rekrutierungen und des Essenztauschs steht aus. Der Stand ist im lokalen Testbuild verfügbar.

## 5. Events und Hilfe

- [ ] Ravaged Realm
- [ ] Sunlit Showdown
- [ ] Frostfire Showdown
- [ ] Titan Reaver Proxy Battle
- [ ] Synergy & CC

Verfügbare Events zuerst umsetzen. Titan Reaver und Synergy & CC benötigen die Erkennung von Einladungen im Chat.

## 6. Minispiele und Quests

- [ ] Matching Cards
- [ ] Fishing
- [ ] Auto-Progress Quests nach Matching Cards

## 7. Scanner

- [ ] AFKJ Tracker Scan mit Heldenkader-Export
- [ ] Guild Manager Scan für Ranglisten und Gildenaktivität
- [ ] Supabase-Anbindung für Guild Manager Scan nach dem lokalen Export

## Routinen und bestehende Aktionen

- [x] Eine gespeicherte Daily-Routine mit frei sortierbaren Schritten
- [x] Tabs für Belohnungen, Kämpfe, Helden und Shop im Daily-Editor. Alle 23 Schritte sind genau einem Tab zugeordnet. 140 automatisierte Tests bestanden.
- [x] Daily rechnet frühere Sanduhrenkäufe anhand der Preisstufe auf Tagesanzahl und Diamantenbudget an. Im Emulator wurde das Angebot für 300 Diamanten bei einem Limit von vier Käufen und 650 Diamanten bis zum Kauf geprüft. Bei erreichtem Limit lief der nächste Schritt an. Emporium überspringt den Schritt anhand der Kaufaufgabe im Spiel.
- [ ] Mehrere separat benennbare Routinen nach dem Vorbild der drei Custom Routines
- [x] AFK Stages und Season AFK Stages, Season Legend Trial, Union Campaign, Dura’s Trials, Arena und Supreme Arena
- [x] AFK Stages nutzt in der Weltansicht den sichtbaren Apex-Stage-Direkteinstieg. Im Emulator wurden aus der gemeldeten Weltansicht die Season-Aufstellung und danach die normale Aufstellung geöffnet.
- [x] Dura’s Trials wählt Battle oder Sweep anhand der sichtbaren Schaltfläche. Raid Treasure wurde anhand einer Battle-Aufnahme geprüft. Im Emulator wurden bei Arcane Treasure, Stufe 30, sechs Sweeps in einem Durchgang ausgeführt und der Verbrauch von sechs täglichen Versuchen bestätigt.
- [x] Dream Realm mit konfigurierbarer Kampfanzahl, standardmäßig maximal zwei pro Lauf
- [x] Dream Realm verlässt die Rangliste über den sichtbaren Zurück-Pfeil und erkennt automatisch abgeholte Belohnungen am verschwundenen Geschenksymbol. Im Emulator wurde die Rückkehr nach bereits erfolgter Abholung geprüft. Anschließend hat ein Daily-Test einen kostenlosen Kampf abgeschlossen. Geschenksymbol und Belohnungsfenster wurden anhand von Aufnahmen geprüft.
- [x] AFK-Belohnungen, kostenlose und Diamanten-Sanduhren, AFK-Stufenbelohnungen und Season-Phase-Ranglistenbelohnungen
- [x] Season-Phase-Ranglistenbelohnungen überspringen Phasen ohne First-Clear-Bereich. Im Emulator wurden District, Server und Guild in Phase 2 geprüft und Phase 1 wurde übersprungen.

## Release-Stand

- 0.4.8 behebt Fehler in Daily, Dura’s Trials und der Ausrüstungserkennung. 130 automatisierte Tests sind bestanden. [GitHub-Release](https://github.com/KaelanTesseract/ADBHelper/releases/tag/v0.4.8)
- 0.4.7 mit Daily-Tabs, Dream Realm sowie neuen Belohnungs-, Helden- und Shop-Aktionen. Windows-Build und 140 automatisierte Tests sind bestanden. [GitHub-Release](https://github.com/KaelanTesseract/ADBHelper/releases/tag/v0.4.7)
