# Unreleased

- **Fishing preview** finds available fishing spots on the current region map, uses the game's automatic walking route, and starts the standard fishing minigame. Catch reliability is still being tested.
- **Log files**: ADBHelper now writes one log file per day to `%APPDATA%\ADBHelper\logs` and keeps the last seven days. **Settings → Log files → Open log folder** opens the folder, so the file can be attached to a bug report.
- **Guild Manager access**: the API URL and the API key are now separate fields. A key pasted inside the URL moves to the key field automatically. Both are stored in the Windows Credential Manager, so they survive a restart, and the key is sent only as the `apikey` header instead of a URL parameter. **Forget saved access** removes them.
- **Automatic wait times**: ADBHelper measures how fast the emulator delivers screenshots and stretches waits and timeouts on slower PCs or emulators, up to three times the normal length. After a game restart caused by a stuck screen it waits a little longer, and returns to normal once actions run smoothly again. There is nothing to configure; the log file records the current factor after each action.
