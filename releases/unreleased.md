# Unreleased

- **Fishing preview** finds available fishing spots on the current region map, uses the game's automatic walking route, and starts the standard fishing minigame. Catch reliability is still being tested.
- **Log files**: ADBHelper now writes one log file per day to `%APPDATA%\ADBHelper\logs` and keeps the last seven days. **Settings → Log files → Open log folder** opens the folder, so the file can be attached to a bug report.
- **Guild Manager access**: the API URL and the API key are now separate fields. A key pasted inside the URL moves to the key field automatically. Both are stored in the Windows Credential Manager, so they survive a restart, and the key is sent only as the `apikey` header instead of a URL parameter. **Forget saved access** removes them.
- **Automatic wait times**: ADBHelper measures how fast the emulator delivers screenshots and stretches waits and timeouts on slower PCs or emulators, up to three times the normal length. After a game restart caused by a stuck screen it waits a little longer, and returns to normal once actions run smoothly again. There is nothing to configure; the log file records the current factor after each action.
- **More languages**: interface texts now live in Fluent files. A translated `<code>.ftl` in `%APPDATA%\ADBHelper\languages` adds a language under **Settings → Language** without a new build; missing texts fall back to English.
- **Installer**: new users can install ADBHelper with **ADBHelper-Setup-x64.exe**: per user, no administrator rights, Start menu entry, uninstall through Windows Settings. The standalone EXE stays available, and existing copies keep updating as before.
- **Fishing warning**: starting Fishing now shows a notice that the action is still work in progress.
- **Third-party notices** are linked from **Settings** and installed next to the app.
- **Signed updates**: updates are now installed only when the release carries a valid signature from the maintainer's key for exactly that version, in addition to the SHA-256 check. Releases are signed with minisign; the public key is `minisign.pub` in the repository.
- **Fewer update checks**: ADBHelper checks at startup and then every six hours instead of every five minutes. **Settings → App updates** still checks on demand.
