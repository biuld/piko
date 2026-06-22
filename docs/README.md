# piko documentation

## Current docs

| Document | Description |
|---|---|
| [feature-parity.md](feature-parity.md) | Feature parity status vs pi-mono and implementation overview |
| [runtime-streaming-redesign.md](runtime-streaming-redesign.md) | Redesign plan for pi-style structured runtime streaming and TUI message lifecycle |
| [session-persistence.md](session-persistence.md) | Design for durable session persistence across root and attached agent transcripts |
| [host-tui-interaction-redesign.md](host-tui-interaction-redesign.md) | Redesign proposal for unidirectional data flow in Host TUI workflows |
| [timeline-ordering-contract-design.md](timeline-ordering-contract-design.md) | Design contract for deterministic timeline item ordering |
| [antigravity-integration-and-reconciliation.md](antigravity-integration-and-reconciliation.md) | Antigravity streaming provider integration and reconciliation design |
| [debugging-hangs.md](debugging-hangs.md) | Techniques for debugging hangs in the TUI and orchestrator |
| [sandbox_design_analysis.md](sandbox_design_analysis.md) | Sandbox policy contract, threat model, and build instructions |

## Package-level docs

Each package maintains its own docs directory:

| Package | Docs |
|---|---|
| `packages/orchestrator/` | [Architecture](../packages/orchestrator/docs/architecture.md), [Actor Kernel](../packages/orchestrator/docs/actor-kernel.md), [Actors (overview)](../packages/orchestrator/docs/actors/README.md), [AgentActor](../packages/orchestrator/docs/actors/agent-actor.md), [Orchestrator Facade](../packages/orchestrator/docs/actors/main-actor.md), [EventStore](../packages/orchestrator/docs/actors/state-actor.md), [Events & State](../packages/orchestrator/docs/events-and-state.md), [Host Integration](../packages/orchestrator/docs/host-integration.md), [Model Step Executor](../packages/orchestrator/docs/model-step-executor.md), [Reconciliation Boundary](../packages/orchestrator/docs/reconciliation-boundary.md), [Tools (overview)](../packages/orchestrator/docs/tools/README.md), [Tool Inventory](../packages/orchestrator/docs/tools/inventory.md), [Tool Providers](../packages/orchestrator/docs/tools/providers.md), [ToolSets](../packages/orchestrator/docs/tools/toolsets.md) |
| `packages/host-runtime/` | [Tool Providers](../packages/host-runtime/docs/tools.md), [Session Reconciliation](../packages/host-runtime/docs/session-reconciliation.md) |
| `packages/host-tui/` | [Overview](../packages/host-tui/docs/overview.md), [TUI README](../packages/host-tui/docs/README.md), [Surfaces](../packages/host-tui/docs/surfaces.md), [Surface UX Contract](../packages/host-tui/docs/surface-ux-contract.md), [Focus](../packages/host-tui/docs/focus.md), [Focus Interaction](../packages/host-tui/docs/focus-interaction-enhancement.md), [Keymap](../packages/host-tui/docs/keymap.md), [Commands](../packages/host-tui/docs/commands.md), [Timeline](../packages/host-tui/docs/timeline.md), [Selectors](../packages/host-tui/docs/selectors.md), [Autocomplete](../packages/host-tui/docs/autocomplete.md), [Notifications](../packages/host-tui/docs/notifications.md), [Hints](../packages/host-tui/docs/hints.md), [Agent Panel](../packages/host-tui/docs/agent-panel.md), [Approval Panel](../packages/host-tui/docs/approval-panel.md), [Status Panel](../packages/host-tui/docs/status-panel.md), [Reconciliation](../packages/host-tui/docs/reconciliation.md) |

## Key docs to read

1. **Getting started**: [../README.md](../README.md)
2. **Architecture**: [packages/orchestrator/docs/architecture.md](../packages/orchestrator/docs/architecture.md)
3. **Host/Engine integration**: [packages/orchestrator/docs/host-integration.md](../packages/orchestrator/docs/host-integration.md)
4. **TUI subsystem overview**: [packages/host-tui/docs/overview.md](../packages/host-tui/docs/overview.md)
5. **Feature parity**: [feature-parity.md](feature-parity.md)
