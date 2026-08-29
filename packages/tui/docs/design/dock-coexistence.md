# Dock Stack Design

> Status: draft
> Feature: [dock-coexistence.md](../features/dock-coexistence.md)

## Module boundary

`features/dock_stack` owns `BandId`, the static registry, offers, grants, and
the height solver. `layout::plane_metrics` collects offers; `navigation` turns
active grants into fixed plane leaves; feature renderers paint those leaves.

```text
Stream (grow)
Boundary (fixed 1, always blank)
Suggest? (ephemeral)
Guidance (fixed 1)
Composer (anchor)
```

## Registry

```rust
BandId = Boundary | Suggest | Guidance | Composer
```

- Boundary and Guidance use `Protect`.
- Suggest uses `Transient`.
- Composer uses `Anchor`.
- There is no durable shrink class and no Todo band.

`DOCK_BOUNDARY_HEIGHT` remains `1`. Rendering performs no paint operation for
this region: no bordered Block, provider title, selection count, or affix.

## Solver

The solver calculates `dock_max = body_height - stream_min(body_height)`,
starts from preferred heights, shrinks transient bands first, then the anchor,
and uses emergency fitting only when protected minimums cannot fit.

## Extension rule

A new plane band requires a `BandId`, registry row, provider offer, region
mapping, renderer, and solver tests. Product state that is explicitly opened as
a modal overlay does not belong in Dock Stack.

## Tests

Pure solver tests cover registry order, inactive grants, shrink behavior, and
Stream floor. Render tests assert Boundary stays blank both idle and while
Suggest is active.
