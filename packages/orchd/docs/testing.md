# Testing

> Status: current  
> Audience: orchd contributors

How orchd integration tests are organized and what each suite verifies.

## Running tests

```bash
# Full orchd crate (unit + integration)
cargo test -p orchd

# Workspace (when touching protocol or hostd wiring)
cargo test --workspace
```

Before commit:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Layout

```text
packages/orchd/tests/
├── common/                    # Shared helpers
│   ├── faux_provider.rs     # Deterministic LLM stub
│   ├── runtime.rs             # Bootstrap AgentRuntimeService for tests
│   └── session_output.rs      # Collect session output from hub
├── agent_api/                 # Public API contract
│   ├── create_task.rs
│   ├── submit_input.rs
│   ├── control_task.rs
│   ├── input_idempotency.rs
│   ├── observation.rs
│   ├── persistence.rs
│   ├── recovery.rs
│   └── support.rs
├── multi_agent/               # Spawn, steer, detached tasks
│   ├── spawn.rs
│   ├── steer.rs
│   ├── detached.rs
│   ├── poll.rs
│   └── shared_agent_spec.rs
├── runtime_integration/       # End-to-end runtime behavior
│   ├── cancel.rs
│   ├── errors.rs
│   ├── observation.rs
│   ├── snapshot.rs
│   └── tools.rs
├── agent_api.rs               # mod aggregator
├── multi_agent.rs
├── runtime_integration.rs
├── orchestrator_integration.rs
└── kernel_integration.rs
```

Unit tests also live alongside source under `packages/orchd/src/**`.

## Shared helpers (`tests/common/`)

| Module | Purpose |
|---|---|
| `faux_provider` | Scripted model responses — no network |
| `runtime` | Build `AgentRuntimeService` with collecting persist sink and test tools |
| `session_output` | Drain `SessionSubscription` into vectors for assertions |

Production code exposes `orchd::testing` (e.g. `CollectingPersistSink`) for the same persist semantics tests rely on.

## Suite guide

### `agent_api/`

Verifies the four command planes and their contracts:

- **create_task** — root/child creation, resume, session binding
- **submit_input** — commit path, receipts, duplicate handling
- **control_task** — close, reopen, cancel_work, terminate
- **input_idempotency** — `request_id` / `message_id` rules
- **observation** — reliable events after commit, subscription basics
- **persistence** — PersistSink called before LLM, ack ordering
- **recovery** — `TaskResumeState` reattach without double-append

### `multi_agent/`

Spawn and steer through tools / task control:

- Child task creation and input routing
- Detached tasks (no parent work coupling)
- Steer and poll semantics
- Multiple tasks sharing one `agent_id` (separate shards)

### `runtime_integration/`

Full loop behavior:

- **cancel** — CancelWork aborts work, task survives
- **errors** — API and stream error mapping
- **observation** — Event vs delta lanes, cursor behavior
- **snapshot** — `session_snapshot` and `SnapshotRequired` recovery
- **tools** — parallel/sequential execution, transcript commits

### Root integration tests

| File | Focus |
|---|---|
| `orchestrator_integration.rs` | Orchestrator step cycle, model streaming |
| `kernel_integration.rs` | Lower-level agent loop and dispatch |

## What to test when changing code

| Change area | Extend |
|---|---|
| `commit_input` / PersistSink | `agent_api/persistence`, `agent_api/submit_input` |
| Idempotency | `agent_api/input_idempotency` |
| Session hub / cursors | `runtime_integration/observation`, `runtime_integration/snapshot` |
| Spawn/steer tools | `multi_agent/*` |
| Error variants in `api/error.rs` | `runtime_integration/errors`, relevant agent_api test |
| hostd storage contract | `hostd` session tests (cross-crate) |

## Principles

1. **No network in integration tests** — use `faux_provider`.
2. **Assert durable order** — inspect collecting persist sink commits, not just session events.
3. **Events after commit** — tests must not expect `MessageCommitted` before PersistAck path completes.
4. **Session-scoped observation** — multi-agent tests subscribe at session level, filter by task when needed.

## Related reading

- [invariants.md](invariants.md) — rules tests enforce
- [public-api.md](public-api.md) — API under test
- [persistence.md](persistence.md) — storage contract (hostd tests)
