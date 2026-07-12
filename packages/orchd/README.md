# orchd

piko's agent execution library: transcript mutation, Model Steps, tool
execution, and Execution lifecycle.

hostd drives orchd through the runtime API. orchd does not own authentication,
Conversation Sessions, Interaction Turns, durable storage, or TUI rendering.

## Documentation

- [Single-Agent Runtime Model](../../docs/single-agent-runtime-model.md) —
  normative concepts, ownership, state machines, and multi-agent extension
  boundary.
- [Single-Agent Runtime Migration](../../docs/single-agent-runtime-migration.md)
  — phased migration from the current implementation.
- [Single-Agent Actor Runtime Design](../../docs/single-agent-actor-runtime-design.md)
  — Tokio Actor ownership, messaging, persistence, observation, and shutdown.

## Public surface

| Crate / module | Purpose |
|---|---|
| [`orchd-api`](../orchd-api/) | Public contract: `AgentRuntime`, ports, errors, DTO re-exports |
| `orchd::api` | Re-exports `orchd-api` + `AgentRuntimeService` |
| `orchd::Runtime` | Bootstrap: agents, tools, persist sink, approval wiring |
| `orchd::tools` | User-interaction tool provider for host/TUI bridges |

Integrators should depend on **`orchd-api`** for traits and port types. Link **`orchd`** for bootstrap and runtime implementation.

Wire DTOs live in `piko-protocol`.
