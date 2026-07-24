# Feature: Theme system

> IDs: **F1–F4**  
> Layer: L4  
> Parent: [roadmap](../roadmap/README.md) · [theme feature](../features/theme.md)

## Problem

Hard-coded dark tokens and forced Dark mode block real Appearance switching
and visual regression under variants.
Domain role colors (e.g. chat author roles) leak product semantics into the kit
core.

## Requirements

### F1 — Application-global theme snapshot (done)

- Theme is an immutable [`ThemeSnapshot`] (palette + tokens) reachable via
  application-global handle APIs: `theme_snapshot()`, `tokens()`,
  `tokens_from(&snapshot)`.
- Apps install the active palette with `set_chrome_palette` /
  `apply_chrome_theme(cx, palette)` (GPUI component Theme overlay + snapshot).
- GPUI Component 0.5 has one application-global Theme. Independent per-window
  palettes are explicitly unsupported until the upstream theme API is scoped.
- Context-free paint helpers read a UI-thread-local mirror of that Theme;
  separate application/test threads do not share palette state.

### F2 — Palette variants (done)

- At least: **dark**, **light** (`ChromePalette`).
- Same component code; different token tables (`ChromeTokens::dark` /
  `ChromeTokens::light`).
- The light table follows Fleet Light semantics: `#EEEFF0` window chrome,
  `#FFFFFF` island surfaces, `#F8F8F9` elevated/hover surfaces, `#090909`
  foreground, `#6E747B` muted foreground, and `#2A7DEB` focus rings.
- Palette reference: [Fleet Light for Zed](https://github.com/skarline/zed-fleet-themes/blob/main/themes/fleet.json).
- Alpha-bearing source borders are composited over the white island surface
  because the compact chrome token table stores opaque RGB values.
- High-contrast remains a future extension.

### F3 — Chrome vs domain palette (done)

- **Chrome core:** canvas, surface, elevated, fg, muted, border, ring, accent,
  success/warning/danger/info; `RoleAccent` is semantic-only.
- **App extension:** domain role colors (authors, tool classes, …) live in the
  consuming app (e.g. GUI `theme::domain`), not required for a generic
  multi-pane client.

### F4 — Render helpers use theme handle (done)

- Surfaces, list/tree paint, native Markdown, and text helpers resolve colors
  through `tokens()` / `theme_snapshot()` (active handle), not a hard-coded
  dark-only table.
- Explicit snapshot path: `tokens_from(&ThemeSnapshot)`.

## Non-goals

- Per-control ad-hoc colors in features (still forbidden).
- Full design-token export pipeline (can follow later).
- High-contrast and independent per-window palettes.

## Acceptance tests

- Unit: dark and light token tables differ on surface/fg.
- Unit: Fleet Light semantic token values remain stable.
- Unit: `set_chrome_palette` updates `theme_snapshot()` / `tokens()`.
- Unit: `tokens_from` ignores the active palette when given an explicit snapshot.
- A binary that depends on chrome only need not link domain role tokens.
