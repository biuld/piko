# hostd Runtime Architecture

This document describes the target internal runtime shape for `hostd`.

The main rule is simple: host-owned state must never be held across long-running
agent/model/tool work. `hostd` owns user-visible state and turn lifecycle;
`orchd` owns task execution.

## Runtime Components

```text
JSON-lines server
  -> HostCommandRouter
  -> HostState                    short-lived state mutation only
  -> TurnSupervisor               active runner, approval, steering, cancel entry point
  -> TurnRunner / orchd           long-running async execution
  -> HostEvent emission           TUI-facing protocol events
```

## Ownership

| Component | Owns | Must not own |
|---|---|---|
| `HostServer` | command routing, session storage access, settings/model/auth resources | long-running turn state |
| `HostState` | sessions, entries, active turn marker, queues, snapshots | model/tool/MCP/OAuth IO waits |
| `TurnSupervisor` | active `TurnRunner`, approval response routing, steering routing | persistent session state |
| `TurnRunner` | executing a prompt through mock or orchd runtime | host state locks, session persistence |
| `orchd` | task/message/tool/approval execution events | host turn lifecycle |

## TurnRunner vs OrchTurnRunner

Both types are needed:

- `TurnRunner` is the hostd abstraction boundary. `HostServer` and
  `TurnSupervisor` depend on this trait, so tests can use `MockTurnRunner` or
  custom runners without constructing orchd or a model gateway.
- `OrchTurnRunner` is the production implementation of `TurnRunner`. It owns an
  `OrchCore`, registers the hostd-managed agent, subscribes to orchd host-facing
  events, and runs the prompt through the real orchestrator.

Do not delete `TurnRunner`; it is the seam that keeps hostd testable and keeps
the command router independent from the concrete orchestration engine.

Do not delete `OrchTurnRunner` until there is another production
`TurnRunner` implementation backed by a better internal orchd runtime API.

`TurnRunner` lives under `turn/runner.rs`, not under `server/`, because it is
the hostd turn execution boundary to orchd. The server layer depends on that
boundary; it should not own the implementation.

## Server Module Layout

`server/` is the host protocol layer. `server/mod.rs` owns `HostServer`, command
dispatch, and shared protocol helpers. Domain work lives in sibling modules:

| Module | Responsibility |
|---|---|
| `server/mod.rs` | `HostServer`, command routing, shared event/storage helpers |
| `server/auth.rs` | API key commands and OAuth device login event streaming |
| `server/config.rs` | settings mutation, runner rebuild, config metadata persistence |
| `server/sessions.rs` | session CRUD, open/list/fork/import/navigation/snapshot |
| `server/supervisor.rs` | internal turn runner handle, approval routing, steering routing |
| `server/turns.rs` | `turn_submit`, prompt resource assembly, runner invocation, queue drain |
| `server/compaction.rs` | threshold checks, summary generation, compaction entry persistence |
| `server/transport.rs` | stdio JSON-lines framing, command ack writing, event serialization |

Keep new command families out of `server/mod.rs` unless the code is only
protocol dispatch. Add a sibling module when a command needs storage, state
mutation, or long-running IO.

## Turn Lifecycle Contract

`hostd` is the only emitter of:

- `turn_started`
- `turn_completed`
- `turn_failed`
- `turn_cancelled`

`TurnRunner` emits or returns task-scoped events:

- task lifecycle
- message streaming
- assistant/tool result commits
- tool lifecycle
- approval requests/resolution

This prevents duplicate turn lifecycle events and keeps root turn state
independent from orchd implementation details.

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

## Current Phase

The current implementation has completed the first split:

- `TurnRunner::run_turn` no longer receives `&mut HostState`.
- `HostServer::apply_turn_submit` releases `HostState` before awaiting the
  runner.
- `OrchTurnRunner` no longer emits `turn_started`.
- `hostd` emits the terminal turn event after the runner returns.
- `server.rs` has been replaced by `server/mod.rs` plus
  auth/config/session/supervisor/turn/compaction/transport modules.

Remaining architecture work:

- track active turns and cancellation handles explicitly in `TurnSupervisor`
- route cancellation to orchd and settle `turn_cancelled` after acknowledgement
- move event application/persistence into a dedicated event sink
- define command ack failure semantics
- define snapshot-only versus event replay recovery
