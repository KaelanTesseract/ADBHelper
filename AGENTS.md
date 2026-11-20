# Release-Regeln

- Pushes, Tags und GitHub-Releases nur nach ausdrücklicher Freigabe des Nutzers für die jeweilige Veröffentlichung. Änderungen und Test-Builds bis dahin ausschließlich lokal vorbereiten.
- Die GUI-Sichtprüfung übernimmt der Nutzer. Nach Änderungen die lokale Test-App nur öffnen; keine automatisierte GUI-Sichtprüfung durchführen.
- Die README und alle neuen Patchnotes auf Englisch schreiben.
- Nach jeder Umsetzung aus der AFK-Journey-Roadmap die Checkliste und den Prüfstand in `ROADMAP.md` aktualisieren.
- Bei jeder veröffentlichten neuen Version die Versionsnummer in Cargo.toml und Cargo.lock erhöhen und neue Patchnotes unter `releases/v<VERSION>.md` schreiben.
- Jedes GitHub-Release muss genau diese Patchnotes enthalten. Nie ein Release ohne Patchnotes oder mit Platzhaltertext veröffentlichen.
- Vor einem Release `scripts/Build-Release.ps1` ausführen. Es bricht ab, wenn Formatierung, Clippy (`-D warnings`) oder Tests nicht bestehen. Neue Clippy-Warnungen sofort beheben.
- Der feste Download-Assetname für den eingebauten Updater ist `ADBHelper-Windows-x64.exe`.
- Die Checkboxen für normale und Season-Stages bleiben nebeneinander. Bei gemeinsamer Auswahl läuft Season vor Normal; Stop beendet die ganze Sequenz.
- Neue Oberflächentexte nur über `tr!("id")` bzw. `tr!("id", name = wert)`, mit Eintrag in `locales/en.ftl` und `locales/de.ftl`. Die Tests in `src/localization/tests.rs` prüfen gleiche Ids und Platzhalter.
