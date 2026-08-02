# D-XX: Design name

> Status: draft | proposed | accepted | superseded
> Implements: [F-XX](../features/F-XX-slug.md)
> Decisions: [ADR-NNN](../decisions/ADR-NNN-slug.md), if any

## Goal

State the vertical slice this design delivers.

## Constraints and non-goals

- Relevant platform, performance, compatibility, and scope constraints.

## Proposed design

Describe ownership, state transitions, data flow, and important interfaces.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | |
| `piko-hostd` | |
| `piko-orchd` | |
| `piko-llmd` | |
| `piko-sandbox` | |

Remove unaffected rows. Add a package only when the design establishes a
durable boundary.

## Reusable infrastructure

State one of:

- No `island-rs` change required.
- Link the corresponding `island-rs` feature/design and define the piko-side
  integration contract.

## Failure and cancellation

Describe errors, stale work, cancellation, cleanup, and recovery.

## Verification

- Unit tests for pure behavior.
- Integration tests for adapters.
- Differential tests against codex-rs mapped to the Feature PRD acceptance
  criteria.

## Alternatives considered

Record meaningful alternatives and why they were not selected.

## Rollout

List small, independently verifiable implementation slices.
