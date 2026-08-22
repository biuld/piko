# ADR-022: Reintroduce a first-party desktop client

> Status: accepted
> Date: 2026-08-22
> Supersedes: [ADR-004](ADR-004-tui-only-product-client.md)

## Context

ADR-004 removed the GPUI desktop client to concentrate effort on the
terminal workflow. F-42 now specifies a desktop GUI shell (floating
sidebar, Timeline, bottom-floating Composer) as a P0 product surface. The
removal left behind `piko-client-core`, the headless protocol projection
library, and the host JSON-lines transport, both of which remain viable
foundations. A second look at the old GUI code is not the way back: its
archipelago was piko-private and mixed product chrome with reusable
mechanics.

## Decision

Reintroduce a first-party desktop client as a new `piko-desktop` crate.

- The desktop shell implements F-42 and reuses `piko-client-core` as the
  sole host-projection store; it does not fork Timeline reduction.
- Product-independent GPUI infrastructure (window chrome, layout, focus,
  overlays, lists, forms, scrolling, theme) comes from `island-rs`, per
  AGENTS.md package-boundary guidance. `piko-desktop` owns only piko
  domain IDs and intents, host projections and transport, localization,
  and product composition.
- The desktop client connects to hostd over the same JSON-lines stdio
  contract as the TUI; the spawn/read client moves into a shared crate so
  the wire has one client implementation.
- hostd remains authoritative for all durable and user-visible product
  state. The desktop client stores only presentation preferences and
  recoverable drafts, in its own client-local file — no `[gui]` hostd
  settings namespace is restored.
- The TUI remains a supported first-party client; nothing in this decision
  changes its authority or behavior.

## Consequences

- Both first-party frontends consume one projection library and one wire
  client, so host projection changes land once.
- Desktop work is gated on island-rs providing reusable chrome/layout/
  focus primitives; gaps discovered while landing F-42 become island
  features, not piko-private components.
- Presentation preferences exist only where the desktop client runs them;
  they never drift into host settings or cross-client sync.
- ADR-004 is superseded: its TUI-only scope ends, while its retained
  consequences (hostd authority, shared projection library) stay in force.
