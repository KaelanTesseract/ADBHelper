# ADBHelper

Native Rust desktop app for AFK Journey on Windows x64.

ADBHelper adapts the AFK Journey workflows from [ADBAutoPlayer](https://github.com/AdbAutoPlayer/AdbAutoPlayer) into a native Rust app.

Download stable versions and release notes from [GitHub Releases](https://github.com/KaelanTesseract/ADBHelper/releases).

**New in 0.4.4:** build a saved Daily routine with drag and drop, AFK rewards,
free hourglasses, optional diamond hourglasses, and AFK stage rewards.
See the [0.4.4 patch notes](https://github.com/KaelanTesseract/ADBHelper/releases/tag/v0.4.4).

The app checks for updates at startup and every five minutes, including while
minimized. A green update icon beside Settings indicates a new release. Click it
to open the patch notes, then choose **Download and install** to
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
The **Daily** card appears first. Its gear opens a routine editor where actions
can be added, removed, and reordered with drag and drop or the Up/Down buttons.
The saved routine runs from top to bottom and uses each action card's settings.
Stop ends the whole routine, and an error prevents later steps from starting.
The default routine collects AFK chest rewards twice and claims up to three
available free Instant AFK hourglasses. Each collection is checked before its
counter is updated. Once the hourglass button displays a diamond price, the
free-hourglass step is complete.

Two additional optional Daily steps are available: **Diamond hourglasses** and
**AFK stage rewards**. Drag them into the routine to enable them; existing saved
routines keep their current steps. The paid step has its own purchase count and
total diamond budget per run in the routine editor (initially one purchase and
50 diamonds). A zero budget disables spending. Run free hourglasses first.
The price is checked twice before each purchase, and the exact diamond balance
is read from Mystical House before and after it. The budget is reserved before
the purchase tap because the game can charge immediately without confirmation.
An unconfirmed charge stops the routine instead of retrying a purchase.
Stage rewards use the original AFK reward entry, recognize claim dialogs and
reward overlays, and maintain their own collection counter. No rewards available
is a completed step; an unrecognized reward response is not counted as a claim.

Each action card has a gear button for its settings popup. PvE team selection,
manual-team skipping and attempts per team are configured separately per action.
Stage selection, Legend Trial towers and Dura's daily Sweep option are available
in the corresponding popup. Changes apply immediately. Close it with **Close**,
Escape or a click outside. During a running task the controls are read-only.
The main Settings page contains language, device connection, and app updates.
ADBHelper defaults to English. Select **Deutsch** under **Settings → Language**
to switch to German. The app remembers your choice across restarts and updates.
GitHub patch notes appear as published.

**Arena** and **Supreme Arena** follow the original AdbAutoPlayer PvP workflows.
They use the formations already configured in the game, not copied PvE teams.
Arena selects the left opponent, uses up to five existing attempts and optionally
claims up to two additional attempts only when their zero-diamond price is proven.
Supreme Arena defaults to five attempts and the left opponent; its popup also
offers the middle/right opponent and a configurable battle limit. It advances
through the three formations with Next, Next, Battle. Available combat Skip is
used; Supreme Arena never purchases extra attempts. Paid or unknown purchase
prompts are declined. PvP victories appear as wins in counters and graphs and
never create stage clears. Local Windows OCR support is checked before starting.

During an active action, an unexpectedly missing AFK Journey process triggers
automatic restart and navigation back to the last mode. Normal AFK stages do not
restart the completed Season phase; Legend Trial skips previously finished towers.
The game provides its saved stage/floor after navigation. Confirmed counters and
run duration survive recovery; an interrupted undecided fight remains unclear.
Arena request/claim checkpoints prevent a restart from resetting the attempt
budget. The current card shows **RECOVERING** until its mode is re-entered. Stop
prevents further restarts. Device/ADB connection failures are not treated as game
crashes. Recovery is limited to three restarts per run and lives in the running
ADBHelper process.

If no battle result appears within 100 seconds, ADBHelper returns to the active
mode when attempts remain. The interrupted fight is recorded as unclear and
the remaining attempt budget carries over.

Live validation of these additions covered both PvP overview navigations, reading
and cancelling the Arena zero-price offer, and one induced game process exit from
the Arena overview followed by automatic restart and return to Arena. No opponent
was selected, no extra attempt claimed, and no PvP battle was started. Combat and
PvP outcome recognition still require a live battle test.

**Season Legend Trial** is a separate dashboard action, ported from AdbAutoPlayer's
`SeasonLegendTrial` workflow. It opens Battle Modes automatically, finds the Legend
Trial entry (scrolling when needed), then checks Lightbearer, Wilder, Graveborn and
Mauler in that order. Today's closed towers are skipped. Choose the included towers
via the action card's gear; all four are enabled initially. Its team settings,
including manual-team skipping, are configured in the same popup. It pushes each available tower until no team
clears the next floor or the current floor/season limit is reached, then proceeds to
the next selected tower. Intermediate rewards and floor reselection are handled.
Stop ends the entire Legend Trial action. Each played tower has its own attempt and
clear counters and appears in the shared mode-comparison chart. No gold is spent.

**Union Campaign** is another dashboard action, based on AdbAutoPlayer's
`UnionCampaignMixin`. It navigates through Battle Modes to Guild Mode and opens
the current campaign floor. It uses its own action settings and pushes floors
until the configured teams run out of attempts or no next floor is recognized.
Combat Skip is used when available. After victory, Continue is retried at most
three times while waiting for loading to finish and the next formation's Records
button to appear. Sweep is never selected or used, and no gold is spent. The mode
has its own attempt/clear counters in Activity and in the shared comparison chart.
Stop ends the entire action. Guild membership and an unlocked Union Campaign are
required.

**Dura’s Trials** is based on AdbAutoPlayer's Dura workflow, with an additional
strict gate for daily sweeps. It reads the game's current **Daily Limit** using
local Windows OCR and identifies the selected stage from its green card border.
After the final first clear, **Current Limit Reached** together with **Max** on
that selected card provides the game's explicit cap confirmation instead.
Sweep is permitted only when two consecutive readings agree on the trial, selected
stage, limit and remaining daily attempts, the selected stage equals the limit,
and its enabled orange Sweep button declares a cost of exactly one daily token.
A lower cleared stage, an exhausted team, a defeat, a disabled button, an unreadable
number or an unknown confirmation dialog never authorizes Sweep. The state is
checked again immediately before each Sweep. Extra stock tokens and purchases are
excluded; the routine stops when daily attempts are gone. Sweep resources are
counted separately and do not inflate battle attempts, clears or win rate.
The separate Sweep dialog must show the same trial and floor. ADBHelper uses the
plus button to set up to six available daily attempts per confirmation. It
checks the displayed quantity, total price, and daily token icon after each
increment and again before confirming. It closes the **Sweep Complete** screen
and records the batch once the daily counter shows the expected decrease.

The Sweep option is enabled initially and can be disabled via the Dura card's gear. Dura result
screens use Back rather than unverified Retry/Repeat or confirmation buttons, so a
resource-consuming replay cannot bypass the cap gate. Windows OCR language support
must be available locally; screenshots are not sent to an OCR service. Unknown
dialog layouts stop the action without blind confirmation. Live resource tests
require explicit authorization and a separate bounded test budget (zero by default).

Live validation on 2026-11-05 used MuMu at 1080 × 1920 with the English game UI:
Gale Treasure floors 21–25 were cleared during testing, then six daily sweeps were
completed at the confirmed cap of 25. Daily attempts decreased from 6 to 0; the
10 extra stock tokens remained unchanged. The test exposed and corrected the
next-formation transition, final paid Retry screen, quantity dialog, last-token
slider position and OCR omissions. Other daily class layouts still need live coverage.

## Run

1. Start the emulator and enable ADB debugging. The app checks `PATH`, then
   automatically searches the standard MuMuPlayer installation folders and
   connects to its default local ADB port (`127.0.0.1:7555`). Set
   `MUMU_ADB_PORT` if the instance uses a different port. For another emulator,
   enter the full path to `adb.exe` in the app's **ADB path** field and choose
   **Apply path**. `ADB_PATH` and `ADBUTILS_ADB_PATH` environment
   variables are also recognized.
2. Enable USB debugging on the Android device or emulator and accept its ADB
   authorization prompt.
3. From the repository root, run:

   ```powershell
   cargo run
   ```

The app lists authorized devices and launches or stops AFK Journey. Stage
automation uses embedded image templates internally; game screenshots are not
displayed in the interface. Collection counts appear separately from battle
attempts, stage clears, and wins.

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

For both AFK Stages modes, ADBHelper brings the game to the foreground and navigates
from the current menu through Battle Modes to the requested stage selector.
It closes known overlays and uses Back to leave other menus, with one game
restart as recovery if navigation gets stuck. Navigation is bounded and cancellable.
The run continues until no available team can clear another stage. It tries the
current team first when the current screen allows it, then copies suggested
teams from Records (default: up to 10 teams, with up to five attempts per
team). It skips locked teams and tries manual formations after automatic
suggestions. Enable **Skip manual teams** in the action's settings popup to exclude
manual suggestions in both modes. Before each mode it returns to the recognized
world or homestead overview and navigates through Battle Modes again.
It will not spend gold. If the battle result times out and attempts remain,
ADBHelper reopens the active mode. Device communication errors end the run
and appear on the action card. Activity shows only the
current game mode and separate attempt/clear counters for each mode visited
within the run. The middle column shows elapsed time, win rate over decided
battles, and average attempts needed for completed stages. Pending and unclear
battles do not count as losses; attempts on unfinished stages are excluded from
the stage average. Counters survive mode switches within an action, freeze at
completion, and reset on the next Start. The statistics include a hoverable
step chart of confirmed stage clears over time and one grouped column chart
comparing attempts and clears across all visited modes on a shared scale.
Success rate and average attempts per completed stage sit side by side.
The comparison chart keeps the same height as more modes are added; hover a
group to see its full mode name and exact counts. No sample data is shown in an idle run.
The device display must use
portrait 9:16 output; screenshots at other 9:16 sizes are resized to the
game's 1080 × 1920 coordinate space.

The Windows build embeds the ADBHelper logo as a multi-resolution executable
icon (16–256 px), in addition to the window/taskbar icon and header logo.

Result recognition checks both Next-button styles and victory-reward styles.
For a shared statistics/result icon it follows the original AdbAutoPlayer
fallback: wait two seconds, check Retry and Power Up, otherwise tap (950, 1800),
then wait up to five seconds for an optional Next button. Missing that optional
button does not fail the stage. Cancellation and ADB errors still propagate.
