# orchd

piko's agent runtime library: task lifecycle, transcript mutation, LLM steps, tool execution, and multi-agent supervision.

hostd drives orchd through the **Agent Runtime API**. orchd does not handle auth, session storage, or TUI rendering.

## Documentation

Full index: [docs/README.md](docs/README.md)

| Doc | Description |
|---|---|
| [overview.md](docs/overview.md) | Architecture, goals, design decisions |
| [core-model.md](docs/core-model.md) | Task / Work / Message identities |
| [public-api.md](docs/public-api.md) | `AgentRuntime` contract and crate exports |
| [host-integration.md](docs/host-integration.md) | How hostd bootstraps and calls orchd |
| [task-runtime.md](docs/task-runtime.md) | Mailbox, input commit, state machines |
| [events-and-observation.md](docs/events-and-observation.md) | SessionOutput and observation hub |
| [persistence.md](docs/persistence.md) | PersistSink barrier and storage contract |
| [invariants.md](docs/invariants.md) | Runtime rules and constraints |
| [errors.md](docs/errors.md) | Public error types |
| [testing.md](docs/testing.md) | Integration test layout |

## Public surface

| Module | Purpose |
|---|---|
| `orchd::api` | `AgentRuntime`, `AgentRuntimeService`, observation types |
| `orchd::host` | Bootstrap: supervisor, tool registry, approval ports |
| `orchd::integration` | `PersistSink` contract (implemented by hostd) |

Wire DTOs live in `piko-protocol`.

## Related docs (repo root)

- [`docs/agent-identity.md`](../../docs/agent-identity.md) — identity conventions
- [`docs/multi-agent-mental-model.md`](../../docs/multi-agent-mental-model.md) — spawn / steer / poll semantics
