# Pointer Hit Validity for Scrollable Content

> Status: reviewed
>
> Package: `piko-tui` (product) · `piko-tui-layout` (hit contract unchanged)
>
> Amends: [`pointer-input.md`](./pointer-input.md). This PRD supersedes that
> PRD's "last painted frame" geometry assumption **for scrollable regions**;
> static composition keeps the per-frame hit map.

## Overview

Pointer hits inside scrollable content (the Timeline first, selectable lists
afterwards) must resolve at **event time against live state**, not against the
absolute-coordinate snapshot of the last painted frame. Interactive identity is
a **stable content id** (the tool call id), never a positional index. Painting
and hit-testing derive from the **same visible-rows model**, so the two cannot
drift.

## Problem

The pointer contract today routes every event through the `HitMap` retained by
the last painted `PreparedFrame` (`packages/tui/src/input/pointer.rs`). That
contract holds for static composition but breaks for scrollable regions in
three concrete ways:

1. **Stale absolute geometry.** Wheel events are coalesced inside one input
   batch without an intervening repaint (`TimelineScrollBatch`). A click or
   hover that follows a scroll in the same batch resolves against pre-scroll
   `y` coordinates: the tool row has already moved, so the click either falls
   through to the Stream default or hits a different tool.
2. **Positional identity.** `HitId::TimelineTool(usize)` binds a hit to a slot
   in `Timeline.components`. Streaming appends, projection rebuilds, branch
   switches, and compaction shift slots, so a hit from even a current frame can
   target the wrong tool.
3. **Stale hover.** Scrolling does not invalidate `AppState::hovered`; the next
   frame highlights a tool that is no longer under the pointer (or is
   off-screen).

Root cause: the map stores **screen-space absolute rects**, which are only valid
for the state that produced them. Scrollable regions change their visible
geometry without a repaint, so their hits must be resolved from live state.

## Contract

Two hit-resolution modes coexist:

| Mode | Regions | Resolution source |
|---|---|---|
| `Snapshot` | Modal layers, chrome, composer, notice, todos toggle, embedded workflows | Per-frame `HitMap` absolute rects (unchanged; z-order/modal authority unchanged) |
| `Live` | Scrollable content: Timeline Stream first; selectable lists (Suggest, Browse/Select, Diagnostics) migrate later | Region-owned resolver over **content coordinates** + live viewport offset |

Live resolution rules:

1. A scrollable region owns a content-space row map: content row → owner
   (`Tool(tool_call_id)` for tool title rows; `None` for everything else).
2. At event time the screen `y` is converted with the **current** viewport
   offset: `content_row = y - content_top + viewport.top_offset()`. The offset
   is read live; it is never baked into the hit map.
3. The resolved element is a stable tool identity: the tool call id, interned
   to a local id so the hitmap's `Copy` contract is preserved, dispatched
   against current state by id, never by index.
4. Scroll alone must not invalidate the resolution path: no recompute, no
   epoch bump, O(1) per event.
5. Content/layout changes (append, rebuild, expansion toggle, theme, width,
   thinking visibility) bump a layout epoch; a plan held from a previous paint
   is recomputed before routing when its epoch is stale.
6. Painting and hit-testing consume the same plan (`lines` + `row_owners` +
   `top_offset`). No independently computed hit geometry is permitted.

## Behavior / interactions

### Click

- A click on a visible tool title row toggles **that tool call**, identified by
  its stable id, regardless of scroll position or how many wheel steps were
  coalesced before the click.
- Clicks on gap rows, message rows, the pending "N new items" banner row, and
  the scrollbar gutter resolve to the Stream default (no-op), unchanged.

### Wheel

- Wheel over the Stream scrolls the timeline by the wheel step. After the
  scroll, the row under the pointer belongs to a new content row; subsequent
  events resolve against the new offset immediately.

### Hover

- `AppState` keeps the last known pointer position. When a viewport change
  (wheel, keyboard scroll, `jump_latest`) occurs, hover is re-derived from that
  position against current geometry; if no position is known, hover clears.
- Hover styling is applied only to the tool whose id is hovered **and** whose
  row is visible in the current plan.

### Identity stability

- Tool hits, hover identity, and the toggle action all use the stable tool
  identity (tool call id via its interned local id).
- A projection rebuild that reorders components never retargets a click to a
  different tool.

## Configuration

No user-facing configuration.

## Non-goals

- No drag, text selection, double-click, or draggable scrollbars (unchanged
  from `pointer-input.md`).
- Static regions do not migrate to live resolution; the per-frame map remains
  authoritative for composition, z-order, and modal barriers.
- This PRD defines the contract for list migration; landing the migration for
  Suggest/Browse/Diagnostics is follow-up work, not part of the Timeline
  landing.

## Acceptance criteria

- [x] A wheel-coalesced batch followed by a click in the same input batch
      resolves the tool visible **after** the total scroll.
- [x] Pure scroll (wheel or keyboard) does not recompute the timeline layout
      plan (epoch unchanged; only the live offset is read).
- [x] Expanding a tool and then clicking another tool in the same batch
      resolves both correctly (epoch bump → one recompute).
- [x] A streaming append between paint and event makes the new tool clickable
      without waiting for the next paint.
- [x] A tool's hit survives a projection rebuild that shifts its source index;
      the click still toggles the same tool call.
- [x] Hover after a scroll re-derives from the last pointer position; no
      wrong-tool highlight, no off-screen highlight.
- [x] Gap rows, message rows, banner row, and scrollbar gutter never resolve
      to a tool.
- [x] Timeline tests cover all of the above; list migration keeps the existing
      pointer-input acceptance (visible candidates track the viewport).

## Reference evidence

- Per-frame hit map: `packages/tui/src/layout/mod.rs`
  (`PreparedFrame`, `build_surface_hitmap_for_frame`).
- Event-time routing: `packages/tui/src/input/pointer.rs`,
  `packages/tui/src/event_loop/input.rs`.
- Timeline plan: `packages/tui/src/features/timeline/layout.rs`.
- Viewport state: `packages/tui/src/features/timeline/viewport.rs`.
- Tool identity: `packages/tui/src/features/timeline/timeline_impl.rs`.
