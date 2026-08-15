# ADR-018: TUI painted frames are the pointer geometry authority

> Status: accepted
> Date: 2026-08-16

## Context

`piko-tui-layout` models a `HitMap` as a per-frame artifact, but `piko-tui`
rebuilt the layout and hit map for every pointer event. Timeline hit geometry
reused full line layout, including Markdown parsing and syntax highlighting.
Trackpad wheel bursts therefore repeated expensive work and the unbounded event
drain delayed the next paint.

Independent paint and pointer preparation also weakened the intended rule that
the solved, visible frame is the sole geometry authority.

## Decision

- `piko-tui` prepares an owned `PreparedFrame` before painting.
- The prepared frame contains the solved product frame, the derived hit map,
  and expensive feature plans shared by paint and hit testing.
- Pointer routing consumes the last painted prepared frame and never rebuilds
  layout in the production event path.
- Terminal events are processed with a bounded per-frame budget. Safe
  high-frequency presentation events may be coalesced.
- `piko-tui-layout` stays product-agnostic; prepared product projections remain
  in `piko-tui`.

## Consequences

- Pointer targets correspond to the most recently painted geometry.
- Trackpad bursts no longer multiply Timeline formatting work per event or
  indefinitely starve painting.
- Features with expensive shared paint/hit geometry have a clear place to
  retain a render plan.
- The runtime owns an additional ephemeral frame snapshot. Coordinate-sensitive
  state changes must yield to a repaint before later pointer activation.
- Longer-lived revision caches remain a separate follow-up. Reducer-only input
  mutation was subsequently adopted by ADR-019 without changing frame
  authority.
