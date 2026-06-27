# hostd / orchd Runtime Boundary Correction

## Background

After introducing `hostd`, the clean product boundary is no longer:

```text
Host <-> orchd protocol
```

That boundary made sense when the TypeScript Host treated `orchd` as an external
server. It is no longer the right main path once `hostd`, `orchd`, and `sandbox`
live in the same Rust workspace.

The corrected architecture is:

```text
TUI <-> host-protocol <-> hostd -> orchd runtime API -> sandbox
```

`protocol` should describe an external boundary. `hostd` and `orchd` are now
internal Rust crates, so their main interaction should be typed Rust APIs, not a
wire protocol or JSON/RPC-shaped DTO layer.

## Target Boundaries

### TUI / hostd

This is the only client/server boundary in the main product.

The TUI may only depend on:

- `host-protocol` command/event/snapshot types, or a generated TypeScript mirror
- a `HostClient` abstraction that sends `HostCommand` and receives `HostEvent`
- local presentation-only state

The TUI must not depend on:

- `orchd` event types
- `orchd::protocol`
- tool execution internals
- model runtime internals
- session storage internals
- the old TypeScript `PikoHost` facade as an authoritative runtime dependency

### hostd / orchd

This is an internal Rust crate boundary.

`hostd` should call a typed runtime API exported by `orchd`, for example:

```rust
pub trait OrchestratorRuntime: Send + Sync {
    async fn run_turn(
        &self,
        input: RunTurnInput,
        sink: &mut dyn RuntimeEventSink,
    ) -> Result<RunTurnOutput, RunTurnError>;

    async fn cancel(&self, task_id: TaskId) -> Result<(), RunTurnError>;

    async fn respond_approval(
        &self,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    ) -> Result<(), RunTurnError>;
}
```

The exact trait shape can change, but the important rule is: `hostd` should not
use `orchd::protocol` as its primary integration layer.

### orchd / sandbox

`orchd` owns tool routing and execution policy decisions. `sandbox` remains a
lower-level command/file execution crate. `sandbox` must not depend on `hostd` or
product-level Host concepts.

## Protocol Ownership

### `host-protocol`

`host-protocol` is the stable product protocol between TUI and hostd.

It owns:

- `HostCommand`
- `HostEvent`
- `HostSessionSnapshot`
- session summaries
- turn snapshots
- tool call snapshots
- approval snapshots
- reconnect/resume semantics

All user-visible state transitions must flow through this protocol.

### `orchd::protocol`

`orchd::protocol` should be treated as a compatibility/wire layer only.

It may remain for:

- standalone `orchd` daemon mode
- JSON-RPC compatibility
- old tests while migration is in progress
- external integrations that explicitly spawn `orchd`

It must not be the normal hostd/orchd integration path.

## Event Model Correction

There should be exactly one TUI-facing event type:

```text
host_protocol::HostEvent
```

`orchd` may have internal runtime events, but they should not be named
`HostEvent`. Current names such as:

- `orchd::protocol::events::HostEvent`
- `OrchEvent`
- `OrchestratorEvent`

need to be reviewed and normalized.

Recommended target naming:

- `orchd::runtime::RuntimeEvent`
- `orchd::runtime::RuntimeMessageEvent`
- `orchd::runtime::ToolRuntimeEvent`
- `orchd::runtime::ApprovalRuntimeEvent`

Avoid using `HostEvent` anywhere inside `orchd`. `HostEvent` should mean
`host_protocol::HostEvent` only.

## Correct Event Flow

The corrected event flow is:

```text
orchd RuntimeEvent
  -> hostd applies event to HostState
  -> hostd emits host_protocol::HostEvent
  -> TUI reducer updates presentation state
```

`hostd` must not pass orchd runtime events through to the TUI.

For example:

```text
RuntimeEvent::AssistantDelta("hi")
  -> HostState.append_assistant_delta(...)
  -> HostEvent::AssistantDelta { seq, session_id, turn_id, text: "hi" }
```

For tools:

```text
RuntimeEvent::ToolStarted { runtime_tool_id, name, args }
  -> HostState.start_tool(...)
  -> HostEvent::ToolStarted { seq, session_id, turn_id, tool_call_id, name }
```

For approvals:

```text
RuntimeEvent::ApprovalRequested { runtime_approval_id, request }
  -> HostState.request_approval(...)
  -> HostEvent::ApprovalRequested { seq, session_id, approval_id, request }
```

Approval response then flows back:

```text
TUI approval action
  -> HostCommand::ApprovalRespond
  -> hostd resolves HostState approval
  -> hostd calls orchd runtime approval responder
  -> HostEvent::ApprovalResolved
```

## Required Refactor Steps

### 1. Introduce `orchd::runtime`

Create an internal runtime facade that is independent from wire protocol types.

Suggested files:

```text
packages/orchd/src/runtime/mod.rs
packages/orchd/src/runtime/types.rs
packages/orchd/src/runtime/facade.rs
packages/orchd/src/runtime/events.rs
```

The facade should expose:

- runtime construction from model executor/config
- turn execution
- streaming event sink
- cancellation
- approval response
- optional snapshot/debug hooks

Do not expose serde/camelCase protocol DTOs in this API unless there is a
specific reason.

### 2. Rename or Isolate orchd `HostEvent`

Current `orchd::protocol::events::HostEvent` is misleading.

Options:

1. Rename it to `RuntimeWireEvent` or `LegacyHostWireEvent`.
2. Move it under an RPC/compat module.
3. Keep a deprecated type alias temporarily, but prevent hostd from importing it.

Acceptance check:

```bash
rg "orchd::protocol::events::HostEvent" packages/hostd packages/host-tui
```

This should return no matches.

### 3. Migrate `hostd::turn_runner`

`hostd::turn_runner` should depend on:

- `host_protocol`
- `hostd::state`
- `orchd::runtime`

It should not depend on:

- `orchd::protocol::events`
- `orchd::protocol::runtime_stream`
- `orchd::protocol::config` as the main runtime config surface
- `orchd` RPC handlers

Temporary use of `orchd::protocol::messages` can be tolerated only as an
intermediate step if model transcript types have not yet moved to runtime/core
types. The final state should move shared model/runtime types out of protocol.

### 4. Make hostd the Sole Adapter

All mappings from orch runtime concepts to product Host concepts must live in
hostd.

Create a clear adapter module:

```text
packages/hostd/src/orchestrator_runtime_adapter.rs
```

or keep it inside `turn_runner.rs` if it remains small.

This adapter must:

- apply every runtime event to `HostState`
- emit only `host_protocol::HostEvent`
- preserve tool call identity
- preserve approval identity
- preserve streaming sequence ordering
- never emit host events without updating HostState first

### 5. Remove TUI Runtime Knowledge

The TUI should not import any host-runtime runtime authority after hostd becomes
the default Host.

Current imports from `piko-host-runtime` may remain temporarily only for:

- UI-only shared display types
- path helpers
- legacy transition code

But TUI must not use `PikoHost` as the authoritative Host runtime once hostd is
enabled.

Target TUI dependency:

```text
ActionService -> HostClient interface -> HostdClient -> host-protocol
```

Not:

```text
ActionService -> PikoHost + optional HostdClient
```

### 6. Keep RPC Compatibility Separate

Standalone `orchd` daemon mode can still exist, but it should be explicitly
separate:

```text
orchd runtime API
  -> rpc adapter
  -> orchd::protocol wire DTOs
```

The RPC adapter should depend on runtime, not the other way around.

## Acceptance Criteria

The refactor is complete when all of the following are true:

1. `hostd` does not import `orchd::protocol::events::HostEvent`.
2. `hostd` does not use `orchd` JSON-RPC/stdio handlers for normal turn runs.
3. `hostd` turn execution goes through `orchd::runtime` typed API.
4. TUI sends only `host-protocol` commands and receives only `host-protocol` events.
5. `orchd::protocol` is used only by standalone daemon/RPC compatibility paths.
6. There is no type named `HostEvent` in `orchd` public runtime APIs.
7. Streaming assistant deltas update `HostState` before being emitted to TUI.
8. Tool lifecycle events update `HostState` before being emitted to TUI.
9. Approval requests/responses round-trip through `HostCommand` and hostd back into orchd runtime.
10. `cargo test --workspace`, `bun run check`, and `bun run test` pass.

## Suggested Verification Commands

```bash
rg "orchd::protocol::events::HostEvent" packages/hostd packages/host-tui
rg "orchd::protocol" packages/hostd
rg "PikoHost" packages/host-tui/src
cargo test --workspace
bun run check
bun run test
```

The second and third searches may still return transitional matches during the
migration, but each remaining match must be justified as compatibility or
UI-only transition code, not as the main runtime path.

## Non-Goals

This correction does not require deleting standalone `orchd` daemon support.
It only requires that standalone daemon protocol no longer be the default
hostd/orchd integration mechanism.

This correction also does not require TUI to understand orchd internals. In
fact, the opposite is required: TUI must become less aware of orchd.

