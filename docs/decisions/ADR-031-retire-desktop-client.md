# ADR-031: Retire the desktop client

> Status: accepted
> Date: 2026-09-13
> Supersedes: [ADR-022](ADR-022-desktop-client-reintroduction.md) (removed)
> Related: [ADR-004](ADR-004-tui-only-product-client.md)

## Context

ADR-022 reintroduced a first-party desktop client (`piko-desktop`, F-42) on
top of the sibling `island-rs` GPUI infrastructure, with `piko-client-core`
as the headless projection library. The desktop surface stayed partial
(D-59 Slices 1–6 implemented; visual acceptance never recorded in V-59).
Maintaining two product surfaces again splits implementation and
verification effort, and the TUI now covers the session-history and agent
control-plane surfaces that desktop work was meant to address. The effort
is being refocused on the terminal workflow.

## Decision

Remove the first-party desktop client:

- Delete the `piko-desktop` crate and its workspace membership.
- Delete the desktop-specific documents: F-42 (desktop GUI shell),
  F-43 (desktop agent workspace), F-44 (conversation canvas presentation),
  D-59–D-61, V-59, and ADR-022.
- F-45 (Timeline conversation blocks), F-46 (assistant turn chips), and
  F-47 (composer attachments) with D-62/D-63/D-64 are kept as design
  records for potential future re-use; they are not scheduled against any
  client surface.
- `piko-client-core` and `piko-comms` remain; the TUI consumes both.
- The sibling `island-rs` repository is outside piko's control surface and
  is left as-is.

## Consequences

- The TUI is again piko's only first-party interactive client, superseding
  ADR-022's reintroduction. ADR-004's rationale stands, updated by the
  runtime and history work that has since landed.
- Reintroducing a desktop client later would start from a new PRD and ADR;
  kept F-45/F-46/F-47 presentation designs are candidates to revisit but
  carry no schedule commitment.
- No `[gui]`/`[desktop]` settings namespaces ever shipped on hostd, so no
  settings cleanup is required.
