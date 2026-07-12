# orchd

piko's agent execution library: transcript mutation, Model Steps, tool
execution, and Execution lifecycle.

hostd drives orchd through [`AgentRuntime`] / `orchd-api::AgentRuntimeApi`.
orchd does not own authentication, Conversation Sessions, Interaction Turns,
durable storage, or TUI rendering.

## Documentation

- [Single-Agent Runtime Model](../../docs/single-agent-runtime-model.md) —
  normative concepts, ownership, state machines, and multi-agent extension
  boundary.
- [Single-Agent Runtime Migration](../../docs/single-agent-runtime-migration.md)
  — completed single-agent product migration and compatibility boundary.
- [Single-Agent Actor Runtime Design](../../docs/single-agent-actor-runtime-design.md)
  — Tokio Actor ownership, messaging, persistence, observation, and shutdown.
- [Multi-Agent Runtime Model](../../docs/multi-agent-execution-model.md) —
  AgentInstance Tree, AgentRuntime routing, AgentActor, tools, and private
  transcripts.
- [Multi-Agent Runtime Migration](../../docs/multi-agent-runtime-migration.md) —
  phased AgentInstance, AgentActor, tools, inbox, recovery, and UI rollout.

## Public surface

| Crate / module | Purpose |
|---|---|
| [`orchd-api`](../orchd-api/) | Public `AgentRuntimeApi`, ports, errors, and DTO re-exports |
| `orchd::AgentRuntime` | AgentInstance registry, policy boundary, and Actor supervisor |
| `orchd::tools` | Multi-agent and user-interaction tool providers |

Integrators should depend on **`orchd-api`** for traits and port types. Link
**`orchd`** for the Agent runtime implementation. `AgentExecutionRuntime` and
`ExecutionActor` are internal implementation details.

Wire DTOs live in `piko-protocol`. Multi-agent support is implemented as a
separate runtime layer built on the completed single-agent invariants; it is
not an Execution tree and does not revive the legacy Task runtime.
