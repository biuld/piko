# piko documentation

## Current docs

| Document | Description |
|---|---|
| [status.md](status.md) | Current status index and source-of-truth pointers |
| [architecture/hostd-global-plan.md](architecture/hostd-global-plan.md) | Current hostd planning document and prioritized runtime/protocol risks |
| [architecture/tui-host-boundary.md](architecture/tui-host-boundary.md) | Product boundary between TUI and hostd |
| [architecture/hostd-orchd-runtime-boundary.md](architecture/hostd-orchd-runtime-boundary.md) | Internal hostd/orchd boundary correction |
| [debugging-hangs.md](debugging-hangs.md) | Techniques for debugging hangs in the TUI and orchestrator |

## Archive

Historical design and migration notes live in [archive/](archive/). They are
kept for context only and should not be treated as current implementation
guidance unless revalidated against `docs/status.md` and current code.

## Package-level docs

Each package maintains its own docs directory:

| Package | Docs |
|---|---|
| `packages/hostd/` | [Runtime Architecture](../packages/hostd/docs/runtime-architecture.md) |
| `packages/orchd/` | [Architecture](../packages/orchd/docs/architecture.md), [Event Sourcing Observability](../packages/orchd/docs/event-sourcing-observability.md), [Host Interface](../packages/orchd/docs/host-interface.md) |
| `packages/host-tui/` | [Overview](../packages/host-tui/docs/overview.md), [TUI README](../packages/host-tui/docs/README.md), [Surfaces](../packages/host-tui/docs/surfaces.md), [Surface UX Contract](../packages/host-tui/docs/surface-ux-contract.md), [Focus](../packages/host-tui/docs/focus.md), [Focus Interaction](../packages/host-tui/docs/focus-interaction-enhancement.md), [Keymap](../packages/host-tui/docs/keymap.md), [Commands](../packages/host-tui/docs/commands.md), [Timeline](../packages/host-tui/docs/timeline.md), [Selectors](../packages/host-tui/docs/selectors.md), [Autocomplete](../packages/host-tui/docs/autocomplete.md), [Notifications](../packages/host-tui/docs/notifications.md), [Hints](../packages/host-tui/docs/hints.md), [Agent Panel](../packages/host-tui/docs/agent-panel.md), [Approval Panel](../packages/host-tui/docs/approval-panel.md), [Status Panel](../packages/host-tui/docs/status-panel.md), [Reconciliation](../packages/host-tui/docs/reconciliation.md) |

## Key docs to read

1. **Getting started**: [../README.md](../README.md)
2. **Current status**: [status.md](status.md)
3. **hostd plan**: [architecture/hostd-global-plan.md](architecture/hostd-global-plan.md)
4. **TUI subsystem overview**: [packages/host-tui/docs/overview.md](../packages/host-tui/docs/overview.md)
5. **Package docs**: see the package-level docs table above
