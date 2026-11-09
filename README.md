<p align="center">
  <img src="assets/adbhelper-logo-v2.png" alt="ADBHelper logo" width="120" />
</p>

<h1 align="center">ADBHelper</h1>

<p align="center">
  AFK Journey automation for Windows. Built with Rust.
</p>

<p align="center">
  <a href="https://github.com/KaelanTesseract/ADBHelper/releases/latest">Download</a>
  &nbsp;·&nbsp;
  <a href="#get-started">Get started</a>
  &nbsp;·&nbsp;
  <a href="#actions">Actions</a>
  &nbsp;·&nbsp;
  <a href="https://github.com/KaelanTesseract/ADBHelper/releases">Release notes</a>
</p>

ADBHelper runs AFK Journey actions on an Android device or emulator through ADB. Use individual actions or combine them into a daily routine, with progress counters and graphs in one dashboard.

- **Daily routines:** choose your actions and arrange them with drag and drop.
- **Automatic navigation:** open supported game modes from the current game menu.
- **Team selection:** use your current team or suggested formations for supported PvE actions.
- **Progress tracking:** view attempts, clears, wins, collections, and daily Sweeps separately.
- **Native Windows app:** one EXE with embedded recognition templates and no console window. Interface available in English and German.

## Get started

### Requirements

- Windows x64 with local Windows OCR support.
- An Android device or emulator with ADB debugging enabled and authorized.
- AFK Journey in **portrait 9:16**. Use the game's **English UI** for the supported recognition templates.
- ADB available on your computer. Standard MuMuPlayer installations are detected automatically.

### First run

1. Download **[ADBHelper-Windows-x64.exe](https://github.com/KaelanTesseract/ADBHelper/releases/latest/download/ADBHelper-Windows-x64.exe)**.
2. Start your emulator or connect your Android device, then open AFK Journey.
3. Launch ADBHelper. In **Settings**, find and select your authorized device. Accept the Android USB debugging prompt if shown.
4. Open an action's **gear button** to configure it, then click **Start**.
5. Use **Stop** to end the current action or the entire Daily routine.

No Rust installation is needed to use the downloaded EXE.

## Actions

### Dashboard actions

| Action | What it does |
| --- | --- |
| **Daily** | Runs your chosen actions in a saved order. Open its gear to build the routine. |
| **AFK Stages** | Pushes normal stages, Season stages, or both. When both are selected, Season runs first. |
| **Season Legend Trial** | Pushes the selected Lightbearer, Wilder, Graveborn, and Mauler towers. Closed towers are skipped. |
| **Union Campaign** | Pushes available guild campaign floors. Requires guild membership and an unlocked campaign. |
| **Dura's Trials** | Pushes free first-clear stages toward the daily limit. Optionally uses daily Sweeps at a verified maximum. |
| **Arena** | Plays daily Arena battles with your in-game team. Can claim up to two additional attempts when their zero-diamond price is confirmed. |
| **Supreme Arena** | Plays daily challenges with your three in-game formations and your selected left, middle, or right opponent. |

AFK Stages, Legend Trial, Union Campaign, and Dura's Trials use their own PvE team settings. Arena and Supreme Arena use formations already configured in the game.

### Reward steps for Daily

Add these steps through the **Daily** gear popup:

| Step | What it does |
| --- | --- |
| **AFK rewards** | Collects the AFK chest twice for daily progress. |
| **Free hourglasses** | Claims up to three available free Instant AFK hourglasses. Stops when the game shows a diamond price. |
| **Diamond hourglasses** | Purchases Instant AFK rewards within your configured purchase count and total diamond budget. Optional. |
| **AFK stage rewards** | Collects available stage-progress rewards. A step with no rewards available completes normally. |
| **Season Phase Ranking rewards** | Collects available District, Server, and Guild first-clear rewards across the available phases. Each scope has its own counter. |

## Daily routines

Open the **Daily** gear to add actions, drag them into order, or use the **Up**, **Down**, and **Remove** controls. The routine is saved automatically and runs from top to bottom using each action's settings.

The default routine contains **AFK rewards → Free hourglasses**. Additional combat and reward steps are optional; new actions are not added to an existing routine automatically.

**Diamond hourglasses require an explicit opt-in:** add the step and configure its purchase count and total budget per run. Initial limits are **one purchase / 50 diamonds**; a zero budget disables spending. Place free hourglasses first. An unconfirmed charge stops the run rather than retrying a purchase.

For Dura's Trials, Sweeps require a confirmed maximum stage and verified daily-token costs. ADBHelper sets up to **six available daily Sweeps** per batch and checks the counter decrease before counting them. Extra stock tokens and purchases are excluded.

## Settings

| Where | Options |
| --- | --- |
| **Action gear** | PvE teams and attempts, manual-team skipping, Legend Trial towers, Dura Sweep preference, and PvP options, depending on the action. |
| **Daily gear** | Included actions, execution order, and diamond-hourglass limits. |
| **Settings page** | Device connection, ADB path, interface language, and app updates. |

English is the default interface language. Select **Settings → Language → Deutsch** for German. The language choice and Daily routine are saved between launches. Action controls are read-only while a run is active.

## Progress and recovery

- **Activity** shows the current game mode and separate counters for each mode visited during the run.
- **Statistics** include elapsed time, success rate, average attempts, progress over time, and a shared mode-comparison chart.
- Confirmed wins, clears, collections, and Sweeps are tracked separately. Unclear battles do not count as wins or losses.
- When ADB disconnects or the game exits, ADBHelper attempts to reconnect or restart and return to the current mode. Confirmed counters and attempt budgets survive recovery within the running app.
- If a battle result is missing after **100 seconds**, the action reopens its mode when attempts remain. Recovery is bounded, with at most **three game restarts per action**.

**Stop** ends the whole routine. An unrecoverable error prevents later steps from starting. Counters remain visible after completion and reset on the next run.

## Updates

ADBHelper checks for updates at startup and every **five minutes**, including while minimized. A **green update icon beside Settings** opens the new release's patch notes. Choose **Download and install** to download, verify, replace, and restart the app.

Updates need write access to the EXE's folder. No GitHub login is required.

See **[published releases](https://github.com/KaelanTesseract/ADBHelper/releases)** for the changes included in each download.

## Connection help

If no device appears, check that the emulator is running, ADB debugging is enabled, and the device has accepted the authorization prompt. For other emulator installations, enter the full path to **adb.exe** under **Settings → ADB path** and apply it.

<details>
<summary>ADB detection and custom ports</summary>

ADBHelper supports `ADB_PATH` and `ADBUTILS_ADB_PATH`, searches `PATH`, and checks standard MuMuPlayer installation folders.

MuMuPlayer's default connection is `127.0.0.1:7555`. Set `MUMU_ADB_PORT` if your instance uses another port.

Other portrait 9:16 resolutions are normalized internally to the game's 1080 × 1920 coordinate space. Screenshots are used for recognition and are not displayed in the dashboard. Windows OCR runs locally.

</details>

## Build from source

Use Windows with Rust's MSVC toolchain, the Visual Studio C++ Build Tools, and the Windows SDK. From the repository root:

```powershell
# Run the desktop app
cargo run --locked

# Run automated tests
cargo test --release --locked
```

Build the Windows EXE and its SHA-256 file with:

```powershell
./scripts/Build-Release.ps1
```

Output: **dist/ADBHelper-Windows-x64.exe** and **dist/ADBHelper-Windows-x64.exe.sha256**. Emulator and captured-image diagnostics are opt-in and excluded from the normal test run.

For publication steps, see **[RELEASING.md](RELEASING.md)**.

## Support

Report issues through the **[issue tracker](https://github.com/KaelanTesseract/ADBHelper/issues)**. Include the ADBHelper version, affected action, emulator or device, error message, and a screenshot of the game at the point of failure.

## Credits and license

Workflow inspiration and original recognition templates come from **[ADBAutoPlayer](https://github.com/AdbAutoPlayer/AdbAutoPlayer)**. ADBHelper adapts its AFK Journey workflows into a native Rust application.

Distributed under the **[MIT License](LICENSE)**. The original template copyright notice is preserved in the license file.
