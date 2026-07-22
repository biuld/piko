# Feature: Archipelago runtime closure

> IDs: **A1–A4**, **C1–C3**  
> Layer: L1  
> Parent: [roadmap](../roadmap/README.md) · [features](../features/archipelago.md) · [archipelago.md](archipelago.md)

## Problem

Docs described a full workspace runtime (`ArchipelagoWorkspace` + focus table +
unified routing) while some apps only used a frame switcher. That is
speculative abstraction risk.

## Decision (locked)

**Archipelago body = island workspace.** Every exclusive place hosts islands
(`IslandNode` + `IslandFocusTable` + `focus_order`), including secondary places
such as preferences (nav + detail panel as real islands).

Frame chrome (TitleBar / StatusBar) remains app-composed slots around the body.

## Requirements

### A1 — Semantics (done)

- Normative docs state body-is-islands without “optional decorative surfaces”.
- Public APIs that imply workspace runtime are used on the product path
  (`ArchipelagoWorkspace`, router, focus tables) — not speculative.

### A2 — Workspace is source of truth (done)

- App declares `ArchipelagoWorkspace { id, island_tree, focus_order }`.
- Assemble/layout and Tab order read from that declaration (or a single derived
  view of it).
- No parallel hard-coded column trees that drift from the workspace.
- Primary consumer path: workspace tree for assemble prune; workspace
  `focus_order` for Settings Tab cycle.

### A3 — Chrome route path (done)

- Archipelago enter/leave/toggle and island focus intents go through
  `ArchipelagoMessage` / `ChromeRoute` / `route_chrome_message` (or a one-line
  app wrapper that only calls those).
- Remount / island focus restore runs only on
  `ArchipelagoTransition::Changed`.
- App wrapper around `route_archipelago_nav` (TogglePair / Enter / Leave / Go);
  focus restore gated on `Changed`.

### A4 — Secondary archipelago islands (done)

- Preferences-style bodies use `IslandView` entities with a dedicated focus
  table (not paint-only surfaces).
- **Consumer path:** Settings Nav + Panel are real `IslandView` entities with
  `settings_focus_table` + `SETTINGS_FOCUS_ORDER` / workspace focus_order.

### C1–C3 — Transitions

- **C1/C2 done:** `ArchipelagoTransition::{Unchanged, Changed { from, to }}`.
- **C3 todo (optional):** add `restore_kind` if animations/logging need it.

## Non-goals

- Product form fields or section catalogs.
- App-specific dock-fit constants inside chrome.
- Animated transitions (app policy).

## Acceptance tests

- Unit: router no-ops return `Unchanged`.
- Unit: sample workspace trees match declared focus_order.
- App integration: enter secondary archipelago focuses its first island; leave
  restores prior archipelago island focus.
- No public chrome type documented as required but unused on a real path without
  a `todo` marker in [roadmap](../roadmap/README.md).
