# TUI Runtime Architecture

> Status: reviewed
>
> Package: `piko-tui` (product runtime) · `piko-tui-layout` (generic geometry)

## Overview

The TUI prepares one authoritative frame snapshot for each paint. The snapshot
contains the solved shell/layout geometry, the interactive hit map derived from
that geometry, and any expensive feature render plans needed by both paint and
pointer input. Terminal events are processed against the last painted snapshot
with a bounded per-frame budget so input bursts cannot starve rendering.

## Problem

The shell, modal, and component abstractions provided consistent visual
composition, but the earlier runtime recomputed layout and hit geometry for
each pointer event. A trackpad emits bursts of wheel events; rebuilding a long
Timeline for every event delayed paint until the burst drained and made the
viewport appear to freeze and jump.

The same gap affected every feature: without a retained frame snapshot, paint
and pointer routing performed separate layout work and could not share cached
geometry.

## User journeys

1. The user scrolls a long Timeline with a trackpad. The viewport continues to
   repaint while inertial wheel events arrive instead of freezing until the
   terminal queue becomes empty.
2. The user clicks a row or tool title. The hit is resolved against the same
   geometry that was most recently painted.
3. Host events and animation ticks arrive during pointer activity. They remain
   responsive because one input burst cannot monopolize the main loop.

## In scope

- One prepared frame per paint, containing `FramePlan` and `HitMap`.
- Paint and pointer input consume the same prepared frame geometry.
- Timeline line layout and tool hit regions are prepared together.
- Pointer events use the last painted hit map; they do not rebuild layout.
- Terminal input processing has a per-frame event/time budget.
- Host input processing has a per-frame line-count budget.
- Each cycle is serialized: input, then host, then tick, then at most one
  paint. Composer keys are applied before host work in the same cycle.
- Host-only Timeline paints are coalesced to a short interval; a cycle with
  input always paints.
- Timeline component lines are cached across frames by content revision, width,
  theme, thinking visibility, and hover.
- Consecutive Timeline wheel events may be coalesced into one line delta.
- Keyboard and pointer adapters emit actions without mutating `AppState`.
- The focus stack is the sole current-mode authority.
- Per-agent Timeline projections and session-wide Timeline entries have one
  owning store.
- Surface placement and interaction capabilities come from one static catalog.
- Rust source files are automatically checked against the 500-line ceiling.
- Existing keyboard, modal authority, and pointer action semantics remain
  unchanged.

## Out of scope

- Pixel-precise trackpad deltas unavailable through Crossterm's wheel events.
- A dynamic surface/plugin registry.
- General incremental rendering for every TUI component.
- Changing hostd or protocol authority.

## Behavior / interactions

- A frame snapshot is replaced only when a new frame is painted.
- All pointer events processed before the next paint resolve through the last
  painted snapshot.
- A bounded batch yields to paint even when terminal events remain queued.
- A bounded host batch yields to paint even when host events remain queued.
- Keyboard and paste are applied in the input slot of the current cycle, then
  host and tick run, then the cycle paints. Host-only cycles may wait up to one
  paint interval so stream tokens cannot monopolize the loop.
- One host drain rebuilds Timeline presentation once, not once per line.
- Consecutive wheel events over Timeline accumulate their existing three-row
  wheel steps; direction changes preserve the net ordered delta.
- Non-Timeline wheel events retain their component-specific behavior.
- Clicks and other geometry-changing actions end the current input batch so a
  new frame is painted before another coordinate-sensitive event is handled.
- Input normalization can inspect state but can only return `Action`; hover,
  press/release pairing, caret movement, and component selection mutate state
  inside reducers.

## Configuration

No user-facing configuration. Frame and event budgets are runtime constants.

## Acceptance criteria

- [x] Production pointer routing accepts a retained `HitMap` and does not call
      `compose_frame` or `build_surface_hitmap` per event.
- [x] Timeline paint consumes the same prepared render plan used to derive its
      tool hit regions.
- [x] A continuous terminal event queue cannot prevent the next paint
      indefinitely.
- [x] A continuous host event queue cannot prevent the next paint indefinitely.
- [x] Pending composer input is applied before host work in the same cycle and
      that cycle paints.
- [x] Consecutive Timeline wheel events produce one accumulated scroll action.
- [x] Existing pointer behavior and modal barriers remain covered by tests.
- [x] Keyboard and pointer production routers accept immutable `AppState` and
      all resulting mutations pass through `AppState::dispatch`.
- [x] `AppState` has no mirrored `mode`; callers derive it from `FocusManager`.
- [x] `TimelineStore` owns active/inactive projections and session-entry fan-out.
- [x] `SurfaceSpec` supplies sizing, input, guidance, and outside-click policy.
- [x] The TUI test suite fails when any Rust source exceeds 500 lines.
- [x] `cargo test -p piko-tui` and workspace clippy pass.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Geometry authority | Last painted `PreparedFrame` | Pointer targets match what the user sees |
| Event burst policy | Bounded by count and elapsed time | Guarantees paint progress without dropping ordered input |
| Trackpad model | Coalesce discrete Timeline wheel steps | Crossterm exposes direction, not pixel magnitude |
| Surface registration | Keep static enums in this slice | Fix frame lifecycle without an unrelated rewrite |
| State mutation boundary | Reducers only | Input adapters remain deterministic and testable |
| Timeline ownership | One `TimelineStore` | Switching and session-entry fan-out cannot drift |
| Surface policy | Static `SurfaceSpec` catalog | Consumers share one classification without a plugin registry |
