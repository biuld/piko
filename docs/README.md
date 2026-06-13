# piko documentation

## Current docs

| Document | Description |
|---|---|
| [missing-features.md](missing-features.md) | Feature parity checklist vs pi-mono coding agent |
| [implementation-gaps.md](implementation-gaps.md) | Action roadmap with phased priorities |

> ⚠️ These docs were last updated 2026-06-01 and reference the old `engine-protocol` / `engine-native` / `engine-remote` package names. The current architecture uses `orchestrator-protocol` / `orchestrator` plus a separate `session` package. The feature parity analysis is still directionally useful, but package references need updating.

## Package-level docs

Each package maintains its own docs directory:

| Package | Docs |
|---|---|
| `packages/orchestrator/` | [Architecture](packages/orchestrator/docs/architecture.md), [Actor Kernel](packages/orchestrator/docs/actor-kernel.md), [Events & State](packages/orchestrator/docs/events-and-state.md), [Host Integration](packages/orchestrator/docs/host-integration.md), [Model Step Executor](packages/orchestrator/docs/model-step-executor.md) |
| `packages/host-tui/` | [Overview](packages/host-tui/docs/overview.md), [Commands](packages/host-tui/docs/commands.md), [Focus](packages/host-tui/docs/focus.md), [Keymap](packages/host-tui/docs/keymap.md), [Surfaces](packages/host-tui/docs/surfaces.md), [Timeline](packages/host-tui/docs/timeline.md), [Autocomplete](packages/host-tui/docs/autocomplete.md), [Notifications](packages/host-tui/docs/notifications.md), [Hints](packages/host-tui/docs/hints.md), [Selectors](packages/host-tui/docs/selectors.md) |

## Key docs to read

1. **Getting started**: [../README.md](../README.md)
2. **Architecture**: [packages/orchestrator/docs/architecture.md](../packages/orchestrator/docs/architecture.md)
3. **Host/Engine integration**: [packages/orchestrator/docs/host-integration.md](../packages/orchestrator/docs/host-integration.md)
4. **Feature parity**: [missing-features.md](missing-features.md)
5. **Action plan**: [implementation-gaps.md](implementation-gaps.md)
