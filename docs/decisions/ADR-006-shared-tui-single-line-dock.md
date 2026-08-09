# ADR-006: Shared TUI single-line dock renderer

> Status: Accepted
> Date: 2026-08-09

## Context

The Notice Row and Pane footer hints are both compact terminal feedback rows,
but had unrelated rendering paths. They need a consistent visual language
without conflating notifications, which carry lifecycle and dismissal state,
with passive focus-local key guidance.

## Decision

Provide a stateless `piko-tui` Single-line Dock renderer that both paths use.
Keep notification projection in `NotificationCenter` and footer ownership in
`PaneSpec`; the renderer receives only final spans, a solved row, and an
optional hover background.

## Consequences

- Narrow-terminal clipping and hover paint are consistent across these rows.
- Shared chrome introduces no protocol, hostd, persistence, or focus-routing
  changes.
- A future dock row can reuse the renderer only when it has exactly one solved
  line and owns its own semantics.
