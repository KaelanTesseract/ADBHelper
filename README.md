# ADBHelper

Native Rust desktop app for AFK Journey on Windows x64.

**Ideengeber und Vorlage:** [ADBAutoPlayer](https://github.com/AdbAutoPlayer/AdbAutoPlayer).
ADBHelper überträgt dessen AFK-Journey-Abläufe in eine native Rust-App.

Download stable versions and patchnotes from [GitHub Releases](https://github.com/KaelanTesseract/ADBHelper/releases).
The app checks for updates at startup and every five minutes, including while
minimized. A green update icon beside Settings indicates a new release. Click it
to open the patchnotes popup, then choose **Herunterladen und installieren** to
download, verify, replace and restart the executable. Updates require write access to the executable's folder;
no GitHub login is needed. See [RELEASING.md](RELEASING.md) for publishing.

The AFK Journey recognition templates originate from the MIT-licensed
[AdbAutoPlayer](https://github.com/AdbAutoPlayer/AdbAutoPlayer) project. The original
copyright notice is preserved in LICENSE.

This is the new native Rust application. The current testable slice supports
AFK Journey only. The Windows release is a single EXE with its AFK Journey
image templates embedded and starts without opening a console window.
The dashboard offers one AFK Stages action with checkboxes for normal and Season
stages. Both are selected initially. When both are selected, Season runs first,
then normal stages after all configured Season team attempts are exhausted.
Completed or unavailable modes are skipped. Technical errors and Stop end the
whole sequence. With neither checkbox selected, Start is disabled.
Each mode uses automatic navigation and suggested teams. Mode-specific counters,
charts and the run timer persist across the transition. Team settings apply to both.
The standalone rewards card
is hidden; that routine is reserved for integration into a combined action.

## Run

1. Start the emulator and enable ADB debugging. The app checks `PATH`, then
   automatically searches the standard MuMuPlayer installation folders and
   connects to its default local ADB port (`127.0.0.1:7555`). Set
   `MUMU_ADB_PORT` if the instance uses a different port. For another emulator,
   enter the full path to `adb.exe` in the app's **ADB path** field and choose
   **Use this ADB path**. `ADB_PATH` and `ADBUTILS_ADB_PATH` environment
   variables are also recognized.
2. Enable USB debugging on the Android device or emulator and accept its ADB
   authorization prompt.
3. From the repository root, run:

   ```powershell
   cargo run
   ```

The app lists authorized devices and launches or stops AFK Journey. Stage
automation uses embedded image templates internally; game screenshots are not
displayed in the interface. The rewards routine is reserved for a future action.

To build a standalone debug executable, run:

```powershell
cargo build
```

To build the Windows tester release, run:

```powershell
./scripts/Build-Release.ps1
```

The resulting `target/release/adbhelper.exe` contains the ADBHelper logo and AFK
Journey templates and does not open a console window. The tester still needs
an Android emulator with ADB available; MuMuPlayer is detected automatically.

Both AFK Stages actions bring the game to the foreground and navigate
from the current menu through Battle Modes to the requested stage selector.
It closes known overlays and uses Back to leave other menus, with one game
restart as recovery if navigation gets stuck. Navigation is bounded and cancellable.
The run continues until no available team can clear another stage. It tries the
current team first when the current screen allows it, then copies suggested
teams from Records (default: up to 10 teams, with up to five attempts per
team). It skips locked teams and tries manual formations after automatic
suggestions. Enable **Manuelle Teams überspringen** in Settings to exclude
manual suggestions in both modes. Before each mode it returns to the recognized
world or homestead overview and navigates through Battle Modes again.
It will not spend gold. If no result screen is detected or device
communication fails, it stops and reports the reason on the action card. Activity shows only the
current game mode and separate attempt/clear counters for each mode visited
within the run. The middle column shows elapsed time, win rate over decided
battles, and average attempts needed for completed stages. Pending and unclear
battles do not count as losses; attempts on unfinished stages are excluded from
the stage average. Counters survive mode switches within an action, freeze at
completion, and reset on the next Start. The statistics include a hoverable
step chart of confirmed stage clears over time and a shared-scale bar comparison
of attempts and clears by mode. No sample data is shown in an idle run.
The device display must use
portrait 9:16 output; screenshots at other 9:16 sizes are resized to the
game's 1080 × 1920 coordinate space.

OCR, other AFK Journey workflows, and a platform installer are still to be
ported; this is not feature parity yet.

The Windows build embeds the ADBHelper logo as a multi-resolution executable
icon (16–256 px), in addition to the window/taskbar icon and header logo.

Result recognition checks both Next-button styles and victory-reward styles.
For a shared statistics/result icon it follows the original AdbAutoPlayer
fallback: wait two seconds, check Retry and Power Up, otherwise tap (950, 1800),
then wait up to five seconds for an optional Next button. Missing that optional
button does not fail the stage. Cancellation and ADB errors still propagate.
