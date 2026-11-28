# Third-party notices

ADBHelper itself is distributed under the [MIT License](LICENSE). It contains
or builds on the following third-party material. Full license texts are in
[`licenses/`](licenses/).

## AFK Journey recognition templates

The images in [`assets/AFKJourney/`](assets/AFKJourney/) are small crops of the
AFK Journey game interface. ADBHelper compares them with device screenshots to
find buttons and screens.

| Source | Files | License |
| --- | --- | --- |
| [ADB AutoPlayer](https://github.com/AdbAutoPlayer/AdbAutoPlayer), copied unchanged | 496 of 529 | MIT, Copyright (c) 2024 AdbAutoPlayer (kept in [LICENSE](LICENSE)) |
| Cropped for ADBHelper from the game interface | 33 of 529 | Same terms as ADBHelper |

The ADBHelper crops are in `arena/`, `daily/`, `dream_realm/`, `duras_trials/`,
`fishing/` (map markers), `navigation/apex_stage_shortcut.png`, and
`progression/`.

AFK Journey and its interface artwork belong to the game's publisher.
ADBHelper is an independent tool and is not affiliated with or endorsed by the
publisher. The crops are used only to recognize the interface.

## Application logo

`assets/adbhelper-logo-v2.png` was generated for ADBHelper with an image
generation tool; the prompt is in
[`assets/adbhelper-logo-v2-prompt.md`](assets/adbhelper-logo-v2-prompt.md).
It is part of ADBHelper and covered by its license.

## Interface fonts

The window uses egui's built-in fonts from the
[`epaint_default_fonts`](https://crates.io/crates/epaint_default_fonts) crate,
embedded in the executable.

| Font | Copyright | License |
| --- | --- | --- |
| Hack | Copyright (c) 2018 Source Foundry Authors; Bitstream Vera: Copyright (c) 2003 by Bitstream, Inc. | MIT and Bitstream Vera License, [`licenses/fonts-Hack.txt`](licenses/fonts-Hack.txt) |
| Ubuntu Light | Copyright 2011 Canonical Ltd. | Ubuntu Font Licence 1.0, [`licenses/fonts-Ubuntu-Font-Licence-1.0.txt`](licenses/fonts-Ubuntu-Font-Licence-1.0.txt) |
| Noto Emoji | Copyright 2013 Google Inc. | SIL Open Font License 1.1, [`licenses/fonts-OFL-1.1.txt`](licenses/fonts-OFL-1.1.txt) |
| emoji-icon-font, egui-icons (subset) | Copyright (c) 2014 John Slegers | MIT, [`licenses/fonts-emoji-icon-font-MIT.txt`](licenses/fonts-emoji-icon-font-MIT.txt), [`licenses/fonts-egui-icons.txt`](licenses/fonts-egui-icons.txt) |

## Root certificates

HTTPS connections (update check, optional Guild Manager roster) trust
Mozilla's root certificate list from the
[`webpki-roots`](https://crates.io/crates/webpki-roots) crate, under the
Community Data License Agreement - Permissive 2.0:
[`licenses/webpki-roots-CDLA-Permissive-2.0.txt`](licenses/webpki-roots-CDLA-Permissive-2.0.txt).

## Hero portraits

The installer brings hero portraits for the Hero Box (`portraits` next to
`ADBHelper.exe`); heroes it does not cover are downloaded once to
`%APPDATA%\ADBHelper\heroes\portraits\web`. They come from
[arnecls/afjk-characters](https://github.com/arnecls/afjk-characters), which
takes them from the [AFK Journey Fandom wiki](https://afk-journey.fandom.com/).
The pictures are artwork of AFK Journey; all rights belong to its publisher.
They are not part of this repository, carry no license, and are shown only
inside ADBHelper.

## Rust crates

All other code comes from Rust crates under the permissive licenses allowed in
[`deny.toml`](deny.toml) (mainly MIT and Apache-2.0). `cargo deny check`
enforces this before every release. List every crate and its license with:

```powershell
cargo deny list -l crate
```
