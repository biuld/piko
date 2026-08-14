# Single-line Dock Design

## Selected Feature

This design implements [single-line-dock.md](../features/single-line-dock.md).

## Ownership

`ui::components::dock_line` is a stateless renderer. Callers construct their
own `Line` spans and select an optional hover background; the component paints
the one-row paragraph. This keeps NotificationCenter responsible for notice
projection and `PaneSpec` responsible for footer geometry.

```text
NotificationCenter ─────────→ Notice renderer ──┐
InteractionHints → PaneSpec → Pane renderer ────┼→ dock_line::render
                                                └→ one solved terminal row
```

## Rendering contract

- `render` receives the already-solved `Rect` and never allocates more space.
- It paints one `Paragraph` without wrapping, so narrow terminals clip the
  trailing text consistently for notices and hints.
- `hint_line` returns the common keyboard glyph plus the standard dim hint
  style. Notice-specific severity glyphs remain constructed at the notice
  call site, because severity belongs to the Notification model, not shared
  chrome.
- Pane hint rendering selects the first non-empty line. `PaneFooter::Hints`
  correspondingly consumes one row whenever non-empty.

## Verification

- A hovered notice still paints its hover background.
- A Pane hint begins with the keyboard glyph and uses the dim token.
- A multi-line hint declaration consumes one row and paints its first
  non-empty line only.
