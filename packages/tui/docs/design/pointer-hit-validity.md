# Design: Pointer Hit Validity for Scrollable Content

> Status: accepted
>
> PRD: [`../features/pointer-hit-validity.md`](../features/pointer-hit-validity.md)

## Goal

Make pointer hits inside scrollable content resolve against **live state at
event time**, with stable ids, so that scroll batches, streaming appends, and
component rebuilds can never make a tool hit stale or mis-targeted. The
Timeline is the reference landing; the same contract generalizes to every
selectable list.

## Why the per-frame snapshot cannot be fixed with invalidation alone

Invalidating the per-frame map on every scroll is correct but wasteful and
fragile: every action that changes geometry (scroll, append, toggle, resize,
theme, thinking visibility) becomes a place where the cache can go stale, and
the common case (pure scroll) pays a full layout recompute per wheel notch.

The robust model is to stop storing **screen-space absolute rects** for
scrollable regions entirely. A scrollable region stores **content-space
ownership**; the viewport offset is read live when an event arrives. Screen
geometry is derived, never cached, for these regions. Absolute snapshots remain
appropriate only for static composition (modals, chrome), whose geometry cannot
change without an intervening repaint.

## Architecture

### Two resolution modes

```text
PreparedFrame (per paint)
  ├─ HitMap<Region, HitId>          snapshot mode: static regions, z-order,
  │                                 modal authority (unchanged)
  └─ TimelineRenderPlan             live mode: content-space row map
       ├─ lines                     shared with paint (Paragraph)
       ├─ row_owners: Vec<Option<RowOwner>>   content row → owner
       ├─ content_area / stream_rect
       ├─ top_offset                paint snapshot only
       └─ epoch                     plan inputs version
```

`route_pointer_with_hitmap` becomes plan-aware: for `Region::Stream` it uses
the live resolver; for every other region it uses the per-frame map exactly as
today. Modal top-layer authority is decided first and unchanged.

### Timeline row ownership

`TimelineRenderPlan` replaces `tool_regions: Vec<(Rect, HitId)>` with
`row_owners: Vec<Option<RowOwner>>`, one entry per content row:

```rust
pub(crate) enum RowOwner {
    Tool(u64), // interned tool identity — see "Stable identity"
}
```

`render_plan` already walks every component and knows each tool title row
(`start + TOOL_TITLE_ROW_OFFSET`); it now writes `Some(RowOwner::Tool(id))`
for title rows and `None` for every other row (gaps, message rows, body rows,
banner). The `top_offset` bake into screen rects is deleted.

### Event-time resolution

```text
event (x, y) inside stream_rect
  → if y outside [content_area.top, content_area.bottom)
       → Stream default                     (banner row, scrollbar gutter)
  → content_row = y - content_area.top + viewport.top_offset()   // LIVE
  → row_owners.get(content_row)
       Some(RowOwner::Tool(id)) → HitId::TimelineTool(id)
       None / out of range      → HitId::Stream (default)
```

`viewport.top_offset()` is read from `ScrollViewport` at event time. Scroll
mutates only `offset_from_bottom`; it never invalidates the plan, so the common
path is O(1) with no recompute.

### Layout epoch

`Timeline` owns `layout_epoch: u64`, bumped by every mutation that can change
`lines` or `row_owners`:

- component mutations: `push_component`, `push`, `upsert_tool`, projection
  sync/rebuild, `clear`, error insertion;
- `toggle_tool` (expansion changes body height);
- `thinking_visible` toggle, theme change, content width change.

`render_plan` snapshots the epoch into the plan. Before routing an event into
the Stream, the pointer path compares
`app.timeline().layout_epoch != plan.epoch`; on mismatch it recomputes the plan
once (the line cache makes unchanged components free) and routes against the
fresh plan. This is the only recompute path, and it is bounded to one per input
batch.

The epoch comparison happens **before** any hit resolution in the batch, so a
sequence like wheel → expand → click resolves each event against the geometry
that state had at that moment.

### Stable identity

`HitId` and `piko-tui-layout::HitMap` require `E: Copy + Eq + Hash`, so the
element id cannot carry a `String` without widening the engine contract. The
Timeline therefore **interns** tool identity:

- `Timeline` keeps `hit_ids: HashMap<String, u64>` (tool call id → local id)
  and a monotonic `next_hit_id`; `ToolEntry` records its interned id, and
  `sync_projection` preserves it across rebuilds exactly like `expanded` is
  preserved today (per-tool state is already snapshotted by id).
- `HitId::TimelineTool(usize)` → `HitId::TimelineTool(u64)` (interned id).
- `TimelineAction::ToggleTool(usize)` → `ToggleTool(u64)`; dispatch resolves
  the interned id to the tool call id, then toggles that component and mirrors
  `expanded` into the `tool_calls` registry (same behavior as today, by id
  instead of slot).
- Hover identity uses the same interned id; painting highlights a tool only
  when its id matches and its row is visible.

Alternative considered: relax the engine bound to `E: Clone + Eq + Hash` and
carry `ComponentId::ToolCallId(String)` directly. More honest identity, but a
cross-crate contract change (`HitMap`, `InteractionState`, hit-test copies)
that the PRD deliberately avoids; revisit if other elements ever need rich
ids.

### Hover reconciliation

`AppState` gains `pointer_position: Option<(u16, u16)>`, updated on every
`PointerAction::Move`. `hovered` becomes a derived cache:

- `Moved` events resolve and set position + hovered (as today).
- Any viewport change (wheel, keyboard scroll, `jump_latest`) re-derives hover
  from `pointer_position` against current geometry before the next paint; with
  no known position, hover clears.
- Re-derivation runs through the same live resolver, so it cannot drift.

## Migration to lists

Suggest, Browse/Select, and Diagnostics viewports currently track visible
source indices per frame. They migrate to the same model:

- content-space ownership (`content row → Row(owner_id)`);
- live offset read at event time;
- stable element ids resolved at dispatch time.

Migration is incremental and lands after the Timeline; the PRD contract governs
both so list behavior does not regress (wheel moves selection, row hits track
the candidates actually visible).

## Costs and invariants

- Common case (scroll only): O(1) per event; no plan recompute, no epoch bump.
- Content changed between paint and event: one plan recompute per batch,
  amortized by the line cache.
- Memory: `row_owners` is O(content rows), the same order as the already-cached
  `lines`.
- Invariant: **paint and hit-testing consume the same plan.** Any future layout
  change (banner, block, inset, wrap) updates one computation.

## Test plan

- Wheel-coalesced batch + click in the same `drain_input` batch resolves the
  post-scroll tool (production-path regression; today's tests rebuild the map
  per event and miss this).
- Scroll alone leaves `layout_epoch` unchanged and does not recompute the plan.
- Expand + second click in one batch resolves both via one epoch-guarded
  recompute.
- Streaming append between paint and event makes the new tool clickable.
- Projection rebuild shifts source indices; click still toggles the same tool
  call id.
- Hover re-derivation after wheel/keyboard scroll at a stored pointer position.
- Gap, message, banner, and scrollbar-gutter rows never resolve to a tool.
- Suggest/Browse/Diagnostics keep their existing pointer acceptance after
  migration.

## Open questions

1. `HitId::TimelineTool` payload: interned `u64` (recommended) vs relaxing the
   engine `E: Copy` bound to `Clone` and carrying the real
   `ComponentId::ToolCallId`. The `Copy` bound is the deciding constraint: the
   shared `HitMap`/`InteractionState` contract and `resolve_target`'s
   `.copied()` all assume cheap copies, and the PRD keeps `piko-tui-layout`
   unchanged. Interning reuses the exact per-tool-state preservation pattern
   `sync_projection` already uses for `expanded`. Relaxing the bound is the
   more principled long-term option (real identity in the element, no parallel
   id space) at the cost of a cross-crate mechanical change and per-hover
   `String` clones; revisit if any other element needs a rich id.
2. Hover re-derivation timing: at paint (`prepare_frame`, recommended) vs in
   the scroll reducer. Hover only affects pixels, and every viewport change
   already forces a repaint in this event loop, so paint-time is effectively
   immediate and covers wheel, keyboard scroll, `jump_latest`, and future
   scrollbars from one path. Reducer-time needs a hook at every
   viewport-mutating action and duplicates resolution. Refinement: crossterm
   wheel events carry coordinates, so `pointer_position` should also be
   updated from wheel events to make re-derivation exact.
3. Epoch granularity: single counter bumped by all mutators (recommended) vs
   per-input fingerprints. The decisive point is that components content
   cannot be fingerprinted cheaply (hashing the component list per event
   defeats the purpose), so a `components_rev` counter is needed regardless;
   once it exists, the other inputs (theme, width, thinking visibility) are
   repaint-driven and cannot go stale mid-batch in practice. A single counter
   is therefore both simpler and sufficient; guard mutator discipline with a
   test that exercises each mutation path and asserts the epoch changed.

## Implementation status

Landed:

- `Timeline` interns stable tool hit ids (`hit_ids` / `next_hit_id`);
  `HitId::TimelineTool(u64)` and `TimelineAction::ToggleTool(u64)` resolve by
  id, and `layout_epoch` is bumped by every geometry-changing mutation
  (append, upsert, projection rebuild, expansion toggle, clear, thinking
  visibility).
- `TimelineRenderPlan` carries `row_owners` (content-space ownership),
  `stream_rect`, `visible_height`, and `epoch`; the `top_offset` bake into
  screen rects is gone. `TimelineRenderPlan::resolve` converts a screen
  coordinate with the **live** viewport offset at event time.
- The per-frame hit map carries only the Stream default; tool hits are
  resolved live in `input/pointer.rs` (`stream_target` / `top_modal_hit`).
- `PreparedFrame::refresh_timeline` recomputes the retained plan only on epoch
  mismatch; `drain_input` refreshes per mouse event, tracks
  `AppState::pointer_position`, and re-derives hover at the end of the batch
  (`reconcile_hover_after_viewport_change`).
- Regression tests: `features/timeline/layout.rs` (epoch stability, identity
  across rebuilds, live-offset resolution, banner/gap rows) and
  `app/tests/pointer_validity_tests.rs` (wheel-batch + click, expand + click,
  streaming append, hover re-derivation, modal guard).
