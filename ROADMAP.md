# ADBHelper – AFK-Journey-Roadmap

Vergleich mit [ADB AutoPlayer, Stand 10. November 2026](https://github.com/AdbAutoPlayer/AdbAutoPlayer/tree/832fa24c8daa6aab62ca425fce1b9569b212eff2/src-tauri/src-python/adb_auto_player/games/afk_journey). Diese Liste nach jeder Umsetzung aktualisieren. Einen Punkt nach der lokalen Implementierung abhaken und ergänzen, wie er geprüft wurde. Veröffentlichungen separat vermerken.

## 1. Daily-Belohnungen

- [x] Freundesbelohnungen über Send & Receive
- [x] Postbelohnungen über Read All
- [x] Battle-Pass-Belohnungen (Noble Path)
- [x] Daily-Questbelohnungen einschließlich verfügbarer Meilensteine
- [x] Gildenquestbelohnungen

Alle fünf Schritte lassen sich einzeln im Daily-Editor hinzufügen. Im Emulator wurden Belohnungen für Noble Path, Daily Quests samt Meilensteinen und Gildenquests abgeholt. Für Send & Receive und Read All wurde die Erkennung der Bestätigung mit Emulator-Aufnahmen geprüft. In einem zweiten Emulatorlauf gingen alle fünf Schritte ohne verfügbare Belohnungen regulär weiter. Der Stand ist im lokalen Testbuild verfügbar.

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

Im Emulator wurde ein kostenloser Affinitätszuwachs beobachtet. Die Live-Vorschau bestätigte die Navigation zur Resonanzhalle, den Aufwertungsbutton mit lesbarer Resonanzstufe, Helden und ihre exklusive Ausrüstung, die Klassenausrüstung, alle drei Shopangebote sowie All-Hero Recruitment mit vorhandenem Invite Letter. Ein vollständiger Live-Test der Aufwertungen, Ausrüstungswechsel, Käufe, Rekrutierungen und des Essenztauschs steht aus. Der Stand ist im lokalen Testbuild verfügbar.

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
- [ ] Mehrere separat benennbare Routinen nach dem Vorbild der drei Custom Routines
- [x] AFK Stages und Season AFK Stages, Season Legend Trial, Union Campaign, Dura’s Trials, Arena und Supreme Arena
- [x] Dream Realm mit konfigurierbarer Kampfanzahl, standardmäßig maximal zwei pro Lauf
- [x] AFK-Belohnungen, kostenlose und Diamanten-Sanduhren, AFK-Stufenbelohnungen und Season-Phase-Ranglistenbelohnungen

## Release-Stand

- 0.4.7 mit Daily-Tabs, Dream Realm sowie neuen Belohnungs-, Helden- und Shop-Aktionen. Windows-Build und 140 automatisierte Tests sind bestanden. [GitHub-Release](https://github.com/KaelanTesseract/ADBHelper/releases/tag/v0.4.7)
