# orchd

piko's agent execution library: transcript mutation, Model Steps, tool
execution, and Execution lifecycle.

hostd drives orchd through [`AgentRuntime`] / `orchd-api::AgentRuntimeApi`.
orchd does not own authentication, Conversation Sessions, Interaction Turns,
durable storage, or TUI rendering.

## Documentation

- [codex-rs Agent Core Digest](../../docs/codex-agent-core-digest.md) — the
  agent-core functional blocks and piko coverage.
- [Agent Runtime Roadmap](../../docs/agent-runtime-roadmap.md) — milestone
  plan and per-block feature decomposition.
- `docs/features/` — Feature PRDs owned by orchd behavior (see
  [docs/features/README.md](../../docs/features/README.md)).

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
