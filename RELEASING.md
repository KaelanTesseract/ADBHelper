# Neue ADBHelper-Version veröffentlichen

**Nur nach ausdrücklicher Freigabe des Nutzers pushen oder veröffentlichen.** Das gilt auch für Quellcode-Pushes und Tags. Lokale Änderungen, Tests und Test-EXEs dürfen vorher vorbereitet werden.

1. Änderungen implementieren und `version` in Cargo.toml erhöhen. Cargo.lock aktualisieren.
2. Für **jedes** Release neue englische Patchnotes unter `releases/v<VERSION>.md` schreiben. Sie beschreiben neue Funktionen, konkrete Änderungen, behobene Fehler und relevante Voraussetzungen. Änderungen an der README ebenfalls auf Englisch schreiben.
3. `./scripts/Build-Release.ps1` ausführen (braucht Inno Setup 6 für den Installer). Ohne Patchnotes, mit unformatiertem Code, Clippy-Warnungen, Funden von `cargo deny` oder fehlschlagenden Tests wird kein Release gebaut.
4. Quellcode und Patchnotes committen und pushen. Den passenden Tag `v<VERSION>` erstellen und pushen.
5. Der Windows-Workflow testet und baut den Installer `ADBHelper-Setup-x64.exe` und die einzelne EXE, veröffentlicht beide mit SHA-256-Datei und verwendet genau die neue Patchnotes-Datei als GitHub-Release-Text.
6. Das veröffentlichte Release auf GitHub und die Update-Erkennung in der App prüfen. Keine bestehenden Release-Dateien nachträglich ersetzen; für Änderungen eine neue Versionsnummer veröffentlichen.

Repository: https://github.com/KaelanTesseract/ADBHelper

Die App verwendet öffentliche stabile Releases, SemVer-Vergleich und den GitHub-Asset-Digest. Assetname für den Updater: `ADBHelper-Windows-x64.exe`; der Installer wird nur für Neuinstallationen gebraucht. Eine installierte App trägt nach einem Update beim Start ihre Version in den Windows-Eintrag „Installierte Apps“ ein. Drafts und Vorabversionen werden nicht angeboten. Ein Hintergrund-Thread prüft beim Start und alle fünf Minuten, auch bei minimiertem Fenster. Ein grünes Symbol neben Einstellungen öffnet das Patchnotes-Popup; der Nutzer startet die Installation ausdrücklich mit „Herunterladen und installieren“.

Der separate Rust-Helfer wartet auf das Ende der App, prüft den Download nochmals, ersetzt die EXE und startet sie am ursprünglichen Pfad. Die vorherige Datei bleibt bis zur Initialisierung der neuen GUI gesichert. Ein fehlgeschlagener Austausch oder fehlgeschlagener Prozessstart stellt die alte Datei wieder her. Kein PowerShell-/CMD-Skript wird auf dem Rechner des Anwenders gestartet.
