# ADR-019: TUI runtime state has explicit authorities

> Status: accepted
> Date: 2026-08-16

## Context

The TUI accumulated working features through several parallel coordination
paths. Input adapters sometimes mutated components directly, `AppState::mode`
mirrored the focus stack, per-agent Timeline state was split across three
fields, and consumers independently classified surfaces. Each local behavior
worked, but cross-feature changes required keeping multiple representations in
sync.

## Decision

- Terminal input adapters borrow application state immutably and emit root
  `Action` values. `AppState::dispatch` reducers own all input-originated state
  mutation.
- `FocusManager` is the only current-mode authority; `AppState::mode()` is a
  derived accessor.
- `TimelineStore` owns the active projection, inactive agent projections, and
  session-wide durable entries, including switching and fan-out rules.
- `SurfaceId::spec()` is the static catalog for placement sizing, input profile,
  guidance profile, and outside-click policy.
- The TUI test suite enforces the project-wide 500-line Rust source ceiling so
  architectural seams do not collapse back into oversized modules.

## Consequences

- Input routing is deterministic and cannot partially mutate state before an
  action reaches the effect pipeline.
- Focus push/pop cannot leave a stale mode mirror.
- Agent switching and session-entry projection have one invariant owner.
- Adding a surface requires one exhaustive catalog row; layout and input
  consumers cannot silently assign conflicting families.
- Some reducers still coordinate several feature components through
  `AppState`; further extraction is optional and must retain the same authority
  boundaries.
