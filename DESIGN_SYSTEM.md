# ADBHelper Design System

## Current Meely visual refinement

The user requested closer matching to the Meely reference. These overrides replace the original Graphit appearance documented below; semantic component roles remain shared.

- Canvas `#111115`, panel `#141419`, section `#17171C`, field `#1D1D23`, active fill `#142328`, border `#2A2A31`.
- Primary text `#ECECF1`, secondary text `#A5A5B0`, turquoise `#47E3E0`.
- Cards have 2 px corners; controls 3 px; compact badges 1 px and 22 px height.
- Active surfaces use a smooth mesh light pool at the inner bottom edge: alpha up to 30/255, 40 px vertical falloff, horizontal soft falloff. A fine 0.7 px `#235F69` edge replaces the expanded outer ring.
- Active card light is painted behind its content. Selected navigation uses a light pool and short underline without a boxed button background. Enabled checkboxes highlight their icon only.
- Section headings 20 px, body 13 px, utility 10 px, activity values 30 px. Card contents align left. Dashboard columns: compact actions, run statistics, then current mode with separate attempts and cleared-stage counters for every mode visited in the run.
- Run statistics: elapsed time including navigation, win rate over decided battles, and average attempts for completed stages. Unknown or unfinished battles are not losses. Unfinished stages are excluded from the average. Counters reset for a new action run, survive mode changes within that run, and remain visible after completion.
- Graphs: confirmed stages as a step curve over elapsed run time, plus horizontal attempt/clear bars by mode on a shared scale. Hover reveals actual values. Empty charts use explanatory text. The curve does not interpolate fractional clears; its data freezes at completion and resets with a new run. Drawing is native egui in `src/charts.rs`.
- Stop keeps its red treatment. The current embedded logo is `assets/adbhelper-logo-v2.png`.
- The dashboard exposes one compact AFK-Stages card with small normal/Season checkboxes and shared Start/Stop controls. Both start selected; Season runs before normal. Selection is locked while running, and Start is disabled for an empty selection. Mode changes preserve counters and the timer, showing the next target while navigating. The rewards card remains hidden. The same logo is embedded in the Windows executable as a multi-resolution icon through `build.rs`.

## Original Graphit source specification

This app uses the user's Graphit Artifact system, adapted for an AFK Journey automation dashboard. The source rules and exact color values are in the Graphit README and `Rust / egui` section. Implementation tokens live in `src/design_system.rs`.

## Principles

- Quiet, near-monochrome surfaces. Colour communicates state, never decoration.
- Flat floating cards with 1 px outlines; no shadows, gradients, or blur.
- Resting text and controls are secondary grey; active values and labels become white.
- Neutral controls use `bg-active` when selected. The app adds a restrained turquoise glow around active elements per the user's request.
- The Graphit source uses Montserrat 400/500/600. The current build uses egui's bundled fallback because Montserrat font files are not present in the workspace.

## Exact source tokens

| Token | Value |
| --- | --- |
| `bg-viewport` | `#0E0E0E` |
| `bg-panel` | `#181818` |
| `bg-section` | `#1A1A1A` |
| `bg-field` | `#262626` |
| `bg-inset` | `#0D0D0D` |
| `bg-active` | `#333333` |
| `border-subtle` | `#383838` |
| `text-primary` | `#FFFFFF` |
| `text-secondary` | `#939393` |
| `text-tertiary` | `#7D7D7D` |
| `text-muted` | `#737373` |
| `text-disabled` | `#8E8E8E` |
| `accent-primary`, `focus-ring` | `#006DFD` |
| User active glow | `#47E3E0` at low opacity |

The Artifact also defines viewport grid/axis, selection, operation chip, badge, and XYZ axis colours. These remain available in `ColorTokens` for future UI elements.

## Layout and scale

- Spacing steps: 4, 8, 12, 16, and 24 px.
- Radius steps: 4, 6, 10, and 14 px; circles use full radius.
- Rows: 24 px; fields: 26 px; controls: 32 px; section headers: 40 px; top bar: 48 px; panel width: 250 px.
- Panels use 12 px inner padding; screen-edge inset is 24 px where the dashboard layout allows it.
- Dense control labels use 12–13 px; helper text 10 px; values 11 px.

## Component states

- Rest: transparent/near-black control fill and `text-secondary`.
- Hover: `bg-field` fill with `text-primary`.
- Active/selected: `bg-active` fill and `text-primary`, outlined by a subtle turquoise glow.
- Focus: 2 px blue focus ring (`focus-ring`).
- Disabled: `text-muted` / `text-tertiary`.
- Glow applies to active navigation, running automation, enabled settings toggles, and active connection/status indicators. It does not replace the neutral selected fill.

## Components

- `panel_frame` / `card`: `bg-panel`, 1 px `border-subtle`, radius-lg, 12 px padding.
- `primary_button`: restrained `bg-field` fill and neutral text; hover/active use Graphit control states.
- `stop_button`: red action treatment for stopping the running Season automation.
- `status_badge`: compact section fill; active badges receive the turquoise glow.
- `active_checkbox`: enabled settings receive the same restrained glow as other active states.
- `paint_active_glow`: reusable, layered low-opacity strokes; no blur or shadow effects.
- Navigation tabs use `Button::selected` for the `bg-active` treatment plus `paint_active_glow`.

## Typography and assets

Use Montserrat from the supplied Graphit system when font files are available. Text hierarchy: heading 13 px, label 13 px, body 12 px, value 11 px, kbd 10 px, axis 15 px. Use sentence case and thin line icons.
