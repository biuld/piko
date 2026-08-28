# Welcome Banner (empty stream)

> Status: implemented

## Overview

When the Stream (timeline) has no paintables, piko shows a **bordered,
centered welcome card** instead of a bare placeholder. It is an empty-state
projection only — not a modal, not durable state, and not shown once any
timeline component paints.

## Layout

```text
┌─ Stream ──────────────────────────────────────────┐
│                                                   │
│          ┌───────────────────────────────┐        │
│          │                               │        │
│          │         _ __                  │        │
│          │     ___ (_) /______           │        │
│          │    / _ \/ /  '_/ __ \         │        │
│          │    / .__/_/_/\_\____/         │        │
│          │    /_/                        │        │  ← fixed logo band
│          │                               │        │
│          │     coding agent · vX.Y.Z     │        │
│          │     ~/path/to/cwd             │        │
│          │                               │        │
│          │     ────────────────────      │        │
│          │                               │        │
│          │     Enter    submit prompt    │        │
│          │     /        commands         │        │
│          │     Ctrl+D   quit             │        │
│          └───────────────────────────────┘        │
│                                                   │
└───────────────────────────────────────────────────┘
```

- Card is centered horizontally; vertically biased slightly above mid-stream.
- Outer chrome is a full `Borders::ALL` block **without a title**.
- Inner pad is one cell on each side.
- Logo occupies a **fixed-height / fixed-width band** (max of all styles). Style
  changes only rewrite glyphs inside that band — the card region does not move
  or resize. Tips are left-aligned under a hairline.
- Colors: logo `accent` / wave `accent_alt`, title `accent`, border
  `border`↔`border_muted`, meta `text_secondary` / `dim`, tips `accent_alt` +
  `dim`.

### Logo styles

Three curated wordmarks cycle slowly (see Animation):

| Style  | Glyph family        | Notes                          |
|--------|---------------------|--------------------------------|
| slant  | FIGlet-style slash  | Default brand mark             |
| box    | box-drawing         | Compact, medium terminals      |
| blocks | block elements      | Dense, wide terminals          |

If the preferred style is wider than the available card, the renderer falls
back to the widest style that fits. Below all logo widths, a `✧ piko`
wordmark is used.

## Animation

Driven by the app tick (`spinner_frame`, ~80 ms):

1. **Style cycle** — rotate slant → box → blocks about every 3 s.
2. **Highlight wave** — one logo row is bold/`accent` at a time; others use
   `accent_alt`.
3. **Border breathe** — border color alternates `border` / `border_muted`.
4. **Narrow spark** — wordmark sparkle glyph cycles when logos do not fit.

Animation is decorative brand motion only on the empty stream. It stops as
soon as timeline content appears (welcome unmounts). Borders never use
`accent` (reserved for selection / brand text).

## Behavior / interactions

- Appears only when the timeline render plan produces zero lines.
- Disappears automatically once any paintables exist.
- Non-interactive: no focus, no hit targets, no keybindings of its own.
- Not part of the scrollable stream (painted as a fixed card over empty area).
- Always on for the first version (no settings key).

## Configuration

None in the first version. Possible later knobs:

- `tui.show_banner` — hide the card
- `tui.welcome.animate` — freeze on a single logo style
- `tui.welcome.logo` — pin slant / box / blocks

## Non-goals

- Pre-alternate-screen stdout splash.
- Full multi-frame “movie” intros or audio.
- Help surface or full keybinding reference (card only hints three actions).
- User-editable custom ASCII art.
