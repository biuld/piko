# Feature: Overlay composite

> IDs: **E1–E6**  
> Layer: L3  
> Parent: [roadmap](../roadmap/README.md) · [overlay feature](../features/overlay.md)

## Problem

Overlays with fixed width/padding break on small viewports; modal focus open
and restore were left to each product path without a shared lifecycle.

## Requirements

### E1 — Responsive envelope (done)

- `overlay_envelope(preferred_width, style, viewport)` clamps width and
  max-height; scales top padding within bounds.
- Feature supplies **preferred** width only.

### E2 — Body scroll (done)

- Panel body scrolls inside max-height; long content does not expand past
  envelope.

### E3 — Focus session contract (done)

- `OverlayFocusSession::{begin, end}` records whether restore should run.
- Docs define open → focus panel control; close → restore prior focus.

### E4 — Host pipeline (done)

- App overlay host on open:
  1. save island / outer focus if needed;
  2. `session.begin()`;
  3. focus panel primary handle (search, confirm, …).
- On close: `session.end()` then restore when true.
- **Consumer path:** GUI `OverlayHost` owns `OverlayFocusSession`;
  `begin_focus_session` / `end_focus_session_if_idle` gate island save/restore
  (palette, host prompts, local confirms). The app snapshot includes the
  opening archipelago, so a secondary workspace restores its own focus rather
  than a hidden primary-workspace island.

### E5 — Viewport on product path (done)

- `OverlayPanelSpec.viewport` set from `window.viewport_size()` on every product
  overlay construction site (palette, host prompt, local confirm, dialogs).
- GUI structural test asserts no `viewport: None` on that path.

### E6 — Tab focus trap (todo)

- Constrain Tab cycle to panel when platform supports it.
- Until then: document limitation; rely on occlude + explicit focus.

## Non-goals

- Overlay **stack priority** — app policy.
- Product modal **business** logic (approvals, catalogs, …).

## Acceptance tests

- Unit: envelope clamps narrow/short viewports (done).
- Unit: focus session begin/end restore flags (done).
- App: open/close restores island focus via session path (E4).
