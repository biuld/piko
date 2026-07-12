# orchd

piko's agent execution library: transcript mutation, Model Steps, tool
execution, and Execution lifecycle.

hostd drives orchd through [`AgentExecutionRuntime`] / `orchd-api::AgentExecutor`.
orchd does not own authentication, Conversation Sessions, Interaction Turns,
durable storage, or TUI rendering.

## Documentation

- [Single-Agent Runtime Model](../../docs/single-agent-runtime-model.md) —
  normative concepts, ownership, state machines, and multi-agent extension
  boundary.
- [Single-Agent Runtime Migration](../../docs/single-agent-runtime-migration.md)
  — Phases 0–6 complete for the single-agent product path; Phase 7 deferred.
- [orchd docs index](docs/README.md) — pointers only (Task-as-current package
  docs retired).
- [Single-Agent Actor Runtime Design](../../docs/single-agent-actor-runtime-design.md)
  — Tokio Actor ownership, messaging, persistence, observation, and shutdown.

## Public surface

| Crate / module | Purpose |
|---|---|
| [`orchd-api`](../orchd-api/) | Public contract: `AgentExecutor`, ports, errors, DTO re-exports |
| `orchd::AgentExecutionRuntime` | Execution Actor runtime (product path) |
| `orchd::tools` | User-interaction tool provider for host/TUI bridges |

Integrators should depend on **`orchd-api`** for traits and port types. Link
**`orchd`** for the Execution runtime implementation.

Wire DTOs live in `piko-protocol`. Multi-agent Execution trees are Phase 7.
