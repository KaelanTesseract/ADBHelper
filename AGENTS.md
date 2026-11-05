# Release-Regeln

- Pushes, Tags und GitHub-Releases nur nach ausdrücklicher Freigabe des Nutzers für die jeweilige Veröffentlichung. Änderungen und Test-Builds bis dahin ausschließlich lokal vorbereiten.
- Bei jeder veröffentlichten neuen Version die Versionsnummer in Cargo.toml und Cargo.lock erhöhen und neue deutsche Patchnotes unter `releases/v<VERSION>.md` schreiben.
- Jedes GitHub-Release muss genau diese Patchnotes enthalten. Nie ein Release ohne Patchnotes oder mit Platzhaltertext veröffentlichen.
- Vor einem Release `scripts/Build-Release.ps1` ausführen und die Tests bestehen lassen.
- Der feste Download-Assetname für den eingebauten Updater ist `ADBHelper-Windows-x64.exe`.
- Die Checkboxen für normale und Season-Stages bleiben nebeneinander. Bei gemeinsamer Auswahl läuft Season vor Normal; Stop beendet die ganze Sequenz.
