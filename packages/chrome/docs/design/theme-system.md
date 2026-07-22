# Feature: Theme system

> IDs: **F1–F4**  
> Layer: L4  
> Parent: [roadmap](../roadmap/README.md) · [theme feature](../features/theme.md)

## Problem

Process-global dark tokens and forced Dark mode block real Appearance
switching, multi-window themes, and visual regression under variants.
Domain role colors (e.g. chat author roles) leak product semantics into the kit
core.

## Requirements

### F1 — Context theme snapshot (done)

- Theme is an immutable [`ThemeSnapshot`] (palette + tokens) reachable via
  process handle APIs: `theme_snapshot()`, `tokens()`, `tokens_from(&snapshot)`.
- Apps install the active palette with `set_chrome_palette` /
  `apply_chrome_theme(cx, palette)` (GPUI component Theme overlay + snapshot).

### F2 — Palette variants (done)

- At least: **dark**, **light** (`ChromePalette`).
- Same component code; different token tables (`ChromeTokens::dark` /
  `ChromeTokens::light`).
- High-contrast remains a future extension.

### F3 — Chrome vs domain palette (done)

- **Chrome core:** canvas, surface, elevated, fg, muted, border, ring, accent,
  success/warning/danger/info; `RoleAccent` is semantic-only.
- **App extension:** domain role colors (authors, tool classes, …) live in the
  consuming app (e.g. GUI `theme::domain`), not required for a generic
  multi-pane client.

### F4 — Render helpers use theme handle (done)

- Surfaces, list/tree paint, markdown style, and text helpers resolve colors
  through `tokens()` / `theme_snapshot()` (active handle), not a hard-coded
  dark-only table.
- Explicit snapshot path: `tokens_from(&ThemeSnapshot)`.

## Non-goals

- Per-control ad-hoc colors in features (still forbidden).
- Full design-token export pipeline (can follow later).
- High-contrast / multi-window live hot-reload polish beyond a testable
  snapshot switch.

## Acceptance tests

- Unit: dark and light token tables differ on surface/fg.
- Unit: `set_chrome_palette` updates `theme_snapshot()` / `tokens()`.
- Unit: `tokens_from` ignores process palette when given an explicit snapshot.
- A binary that depends on chrome only need not link domain role tokens.
