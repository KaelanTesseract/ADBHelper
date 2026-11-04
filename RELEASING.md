# Neue ADBHelper-Version veröffentlichen

1. Änderungen implementieren und `version` in Cargo.toml erhöhen. Cargo.lock aktualisieren.
2. Für **jedes** Release `releases/v<VERSION>.md` schreiben: neue Funktionen, konkrete Änderungen, behobene Fehler und relevante Voraussetzungen. Keine unveränderten alten Patchnotes wiederverwenden.
3. `./scripts/Build-Release.ps1` ausführen. Ohne Patchnotes oder mit fehlschlagenden Tests wird kein Release gebaut.
4. Quellcode und Patchnotes committen und pushen. Den passenden Tag `v<VERSION>` erstellen und pushen.
5. Der Windows-Workflow testet und baut die einzelne EXE, veröffentlicht sie mit SHA-256-Datei und verwendet genau die neue Patchnotes-Datei als GitHub-Release-Text.
6. Das veröffentlichte Release auf GitHub und die Update-Erkennung in der App prüfen. Keine bestehenden Release-Dateien nachträglich ersetzen; für Änderungen eine neue Versionsnummer veröffentlichen.

Repository: https://github.com/KaelanTesseract/ADBHelper

Die App verwendet öffentliche stabile Releases, SemVer-Vergleich und den GitHub-Asset-Digest. Assetname: `ADBHelper-Windows-x64.exe`. Drafts und Vorabversionen werden nicht angeboten. Updates werden beim Start und alle sechs Stunden abgefragt; der Nutzer startet die Installation ausdrücklich mit „Updaten“.

Der separate Rust-Helfer wartet auf das Ende der App, prüft den Download nochmals, ersetzt die EXE und startet sie am ursprünglichen Pfad. Die vorherige Datei bleibt bis zur Initialisierung der neuen GUI gesichert. Ein fehlgeschlagener Austausch oder fehlgeschlagener Prozessstart stellt die alte Datei wieder her. Kein PowerShell-/CMD-Skript wird auf dem Rechner des Anwenders gestartet.
