# hostd Runtime Architecture

This document describes the target internal runtime shape for `hostd`.

The main rule is simple: host-owned state must never be held across long-running
agent/model/tool work. `hostd` owns user-visible state and turn lifecycle;
`orchd` owns agent execution.

## Bounded Contexts

```
packages/hostd/src/
  lib.rs                 # crate root with backward-compat re-exports
  main.rs                # entrypoint
  api.rs                 # re-exports piko_protocol types

  protocol/              # Host protocol: command dispatch + event emission
    mod.rs               # HostServer, command routing, shared helpers
    transport/
      jsonl_stdio.rs     # stdio JSON-lines framing, CommandAck
    commands/
      auth.rs            # OAuth + API key commands
      config.rs          # ConfigUpdate command, runner rebuild
      sessions.rs        # session CRUD, open/list/fork/import/navigation
      turns.rs           # turn_submit, prompt assembly, runner invocation, queue drain
      compaction.rs      # threshold checks, summary generation, compaction persistence

  domain/                # Business logic: no IO dependencies (no JSONL, stdio, MCP transport)
    mod.rs
    sessions/
      state.rs           # HostState, SessionState, entries, cumulative usage, queues
    turns/
      runner.rs          # TurnRunner trait, TurnRunInput, ErrorTurnRunner
      orch_adapter.rs    # OrchTurnRunner — production adapter to orchd
      supervisor.rs      # TurnSupervisor — active runner handle, approval/steering routing
    config/
      settings.rs        # HostSettings, SandboxSettings, CompactionSettings, SettingsManager
      models.rs          # ModelRegistry — provider/model resolution
    prompts/
      mod.rs             # System prompt builder, context files, templates
      skills.rs          # Skill loading and formatting
    compaction/
      mod.rs             # Compaction logic: cut points, should_compact
      summarizer.rs      # LLM-based history summarization

  infra/                 # External system adapters (IO, LLM gateway, MCP, storage)
    mod.rs
    storage/
      mod.rs             # Re-exports
      types.rs           # JsonlSessionRepository, PersistedSession, SessionStorageError
      jsonl_io.rs        # Low-level JSONL read/write
      jsonl_repository.rs # Session CRUD on JSONL files (fork, import, etc.)
    mcp/
      mod.rs             # MCP server integration
```

## Key Principles

- `protocol/` can call `domain/`, not vice versa.
- `domain/` does not depend on JSONL, stdio, tokio process, MCP transport.
- `infra/` implements ports/traits needed by `domain/`.
- `OrchTurnRunner` lives in `domain/turns/` as an adapter to orchd.
- `TurnSupervisor` lives in `domain/turns/` — it owns active turn handles,
  cancel settlement, approval routing.

## Runtime Components

```text
JSON-lines server
  -> HostCommandRouter (protocol/)
  -> HostState (domain/sessions/)         short-lived state mutation only
  -> TurnSupervisor (domain/turns/)       active runner, approval, steering, cancel entry point
  -> TurnRunner / orchd (domain/turns/)   long-running async execution
  -> HostEvent emission                   TUI-facing protocol events
```

## Ownership

| Component | Owns | Must not own |
|---|---|---|
| `HostServer` | command routing, session storage access, settings/model/auth resources | long-running turn state |
| `HostState` | sessions, entries, active turn marker, queues, snapshots | model/tool/MCP/OAuth IO waits |
| `TurnSupervisor` | active `TurnRunner`, approval response routing, steering routing | persistent session state |
| `TurnRunner` | executing a prompt through mock or orchd runtime | host state locks, session persistence |
| `orchd` | Execution / model / tool / approval work | host turn lifecycle |

## TurnRunner vs OrchTurnRunner

Both types are needed:

- `TurnRunner` is the hostd abstraction boundary. `HostServer` and
  `TurnSupervisor` depend on this trait, so tests can inject custom runners via
  `HostServer::with_storage_and_runner` (see `tests/support/mock_turn_runner.rs`)
  without constructing orchd or a model gateway.
- `OrchTurnRunner` is the production implementation of `TurnRunner`. It owns an
  `AgentExecutionRuntime`, registers the root `main` agent, bridges SessionOutput
  observation via `ExecutionChanged`, and runs each Turn as one short-lived
  Execution.

Do not delete `TurnRunner`; it is the seam that keeps hostd testable and keeps
the command router independent from the concrete orchestration engine.

## Turn Lifecycle Contract

`hostd` is the only emitter of:

- `turn_started`
- `turn_completed`
- `turn_failed`
- `turn_cancelled`

`TurnRunner` observation (Execution path) publishes:

- `SessionEvent::ExecutionChanged` (Running → terminal)
- message streaming / MessageCommitted
- tool commits
- approval requests/resolution

hostd maps `ExecutionChanged` → `AgentChanged` for the TUI agent panel.
Task/Work observation events are no longer on the product SessionEvent wire.

Turn terminal status is derived from the Execution outcome, not from Task Idle
as command truth.

## Storage shard policy

Per-execution append-only JSONL under `tasks/{id}.jsonl` (filename retained for
schema-v2 compatibility; `id` is the Execution id on the new path):

| Writer | Records |
|---|---|
| Execution path (product) | One root `tasks/{root_id}.jsonl`: Header once + Messages across Turns |
| Legacy shards (read) | May contain Lifecycle / WorkLifecycle lines |

`root_id` is allocated on the first Turn (`exec_*`) and reused for later Turns.
Runtime `execution_id` stays unique per Turn and is not used as a new shard key.

Readers (`load_task`) accept both shapes. Resume / follow-up Turns load
transcript Messages only. Lifecycle lines are not written for new product Turns
and are not used as Turn terminal truth.

## Locking Contract

`HostState` may be locked only for short critical sections:

- validate that a session exists
- create an active turn marker
- append a session entry
- apply a completed event to session state
- clear active turn state
- build a snapshot

`HostState` must not be locked while awaiting:

- model calls
- tool execution
- approval waits
- MCP process IO
- OAuth polling
- compaction summarization
- child task joins

## Turn Submit Flow

```text
turn_submit
  -> read cwd from HostState
  -> build prompt resources outside HostState lock
  -> short-lock HostState.start_turn()
  -> emit turn_started
  -> append user message
  -> await TurnRunner without HostState lock
  -> short-lock apply returned events and persisted transcript entries
  -> hostd emits turn_completed or turn_failed
  -> compact/drain queues after completion
```

## Remaining Architecture Work

- **Turn and Execution lifecycle** — hostd owns Conversation Sessions and
  Interaction Turns; orchd owns active Agent Execution. See the
  [runtime model](../../../docs/single-agent-runtime-model.md) and
  [migration plan](../../../docs/single-agent-runtime-migration.md).
- track active turns and cancellation handles explicitly in `TurnSupervisor`
- extract event_sink: move "apply event → update state → persist JSONL" out of turns.rs
- extract domain-facing repository trait; JSONL implementation stays in infra
