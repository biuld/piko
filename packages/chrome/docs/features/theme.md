# Feature: Theme

## Overview

Shared visual system for multi-pane chrome: surfaces, spacing, type roles,
semantic colors, and icons. Apps consume tokens and helpers; they do not invent
one-off density or type sizes for chrome chrome.

## Behavior

- Semantic surfaces (canvas, island surface, elevated, dim, ring, border).
- Compact density scale (spacing, header heights, tool rails).
- Named text roles applied through helpers (not ad-hoc font sizes at call sites).
- Icon sizes aligned to type roles; vendored SVG paths served by the kit assets.
- **Palette variants:** dark and light via `ChromePalette` + `ThemeSnapshot`.
- Active snapshot installed with `apply_chrome_theme(cx, palette)` (or
  `set_chrome_palette` for process-only tests).
- Helpers (`tokens`, surfaces, list/tree paint, markdown) read the active
  snapshot handle — not a silent dark-only constant.

## App responsibilities

- Register kit assets at application start.
- Apply the kit theme (`apply_chrome_theme`) at start and when appearance
  changes; persist product preference if desired.
- Keep product domain role colors in the app (not chrome core).
- Prefer consuming roles/metrics over hard-coded pixel pairs.

## Non-goals

- Product string catalogs.
- Domain role colors (chat authors, tool classes) as a chrome-core dependency.
- Full design-token export pipeline / high-contrast palette (optional later).
