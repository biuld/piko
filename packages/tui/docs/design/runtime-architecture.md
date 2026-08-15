# Design: TUI Prepared-Frame Runtime

> Status: implemented
>
> PRD: [../features/runtime-architecture.md](../features/runtime-architecture.md)
> ADRs: [ADR-018](../../../../docs/decisions/ADR-018-tui-prepared-frame-runtime.md) ·
> [ADR-019](../../../../docs/decisions/ADR-019-tui-runtime-authorities.md)

## Goal

Make a painted frame the single geometry authority for pointer routing and
prevent terminal input bursts from starving paint.

## Runtime flow

```text
host drain / state update
        |
        v
prepare_frame(app, terminal rect)
  - compose shell + plane + modal layout once
  - prepare Timeline lines and visible tool hits once
  - build HitMap from the solved layout and prepared feature hits
        |
        v
PreparedFrame { product, hit_map, timeline }
        |                         |
        v                         v
paint prepared data        route pointer against hit_map
        ^                         |
        +------ bounded batch ----+
```

`PreparedFrame` is a product type in `piko-tui`; `piko-tui-layout` remains a
generic geometry engine. It contains no host or protocol state.

## Mutation flow

Both keyboard and pointer adapters borrow `AppState` immutably. They normalize
terminal input into root `Action` values. `AppState::dispatch` is the mutation
boundary for input-owned state; its pointer reducer owns hover, paired button
state, caret placement, component selection, and action chaining. Host events
and effect results retain their existing application boundaries. The test-only
pointer helper may immediately reduce an action to preserve concise integration
tests, but production routing never mutates state.

`AppState::mode()` derives from `FocusManager::active_mode()`. There is no
parallel mode field to synchronize on push, pop, or clear.

## Timeline ownership

`TimelineStore` owns the active projection, inactive per-agent projections,
and durable session-wide entries. It provides atomic operations for agent
selection, lazy seeded projection creation, and session-entry fan-out. The
agent panel retains viewed-agent identity; `AppState` coordinates that identity
with one store operation instead of swapping three public collections.

## Surface catalog

`SurfaceId::spec()` returns the static `SurfaceSpec` consumed by navigation,
layout sizing, keyboard routing, guidance, and outside-click policy. The spec
contains placement sizing, input profile, guidance profile, and pointer
barrier behavior. Feature state still computes dynamic row budgets and content
sizes; the catalog declares the capability family.

## Prepared Timeline

Timeline preparation already needs to flatten components into terminal lines
to derive content height and tool title rows. The prepared plan is retained in
the frame and passed to paint instead of invoking `render_plan` a second time.

The cache lifetime is one frame in this slice. A later revision-based cache can
retain component lines across frames, keyed by content, width, theme, thinking
visibility, and expansion revisions. Scroll offset alone must not invalidate
component formatting.

## Pointer routing

Production routing receives `&HitMap<Region, HitId>`. A test/convenience helper
may prepare a map from state, but the main loop never calls that helper.

Coordinate-sensitive activation ends the current batch after dispatch. This
ensures a subsequent click uses geometry painted after any selection, modal,
or expansion change. Timeline wheel events are safe to coalesce because they
change only the viewport offset and retain the Stream host rectangle.

## Event budget

The main loop processes at most a small fixed number of immediately available
terminal events or a short wall-clock budget before returning to paint. Host
input is independently capped by a fixed line count per frame. Neither ingress
path drains an unbounded queue.

Consecutive wheel events whose hit resolves to `Region::Stream` accumulate a
signed number of rows using Timeline's existing three-row wheel step. The
accumulator flushes before a non-Timeline event and at the end of the batch.
Consecutive move events may keep only the latest coordinate because hover is
presentation state.

## Validation

- Prepared-frame tests prove Timeline hits and paint share one plan.
- Batch tests prove Timeline wheel deltas coalesce without changing direction.
- Existing pointer integration tests validate modal and feature semantics.
- Existing render tests validate visual output.
- `architecture_tests::rust_sources_respect_file_size_ceiling` recursively
  enforces the 500-line source ceiling during the normal TUI test suite.

## Deliberate extension points

- Longer-lived Timeline formatting caches may build on `PreparedFrame` with
  explicit content/width/theme revisions.
- A dynamic surface registry remains out of scope; `SurfaceSpec` is an
  exhaustive static product catalog.
- Feature reducers can be extracted from `AppState` when a domain grows, while
  retaining the same root `Action` and `Effect` boundary.
