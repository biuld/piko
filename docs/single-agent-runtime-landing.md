# Single-Agent Runtime Landing Plan

> Status: single-agent product path complete; Phase 7 deferred
> Normative model: [Single-Agent Runtime Model](single-agent-runtime-model.md)
> Actor design: [Single-Agent Actor Runtime Design](single-agent-actor-runtime-design.md)
> Migration phases: [Single-Agent Runtime Migration](single-agent-runtime-migration.md)

## 1. Strategy

Do **not** create a new crate. Land inside `orchd` / `orchd-api` / `protocol` /
`hostd` with a parallel Execution path, then delete Task/Work.

```text
retain:  runtime/step, runtime/tools, transcript, Event/Delta lanes, llmd glue
replace: Task mailbox, Work SM, Supervisor TaskRegistry as business state
add:     AgentRuntime + ExecutionActor + session-scoped ports
cut over: hostd TurnSubmit → start_execution
delete:  CreateTask / SubmitTask / TaskControl public surface
```

One Session selects exactly one runtime path. No dual writers for the same Turn.

## 2. Module Map

### Target orchd layout (additive first)

```text
packages/orchd/src/
  runtime/
    step/          KEEP — Model Step stream + assembly
    tools/         KEEP — sequential/parallel tool batches
    events/        KEEP lanes; stop using hub as command ack
    execution/     NEW  — ExecutionActor, state, mailbox, finalizer
    task/          DELETE after Phase 5
  application/
    agent_runtime.rs   NEW — public facade + supervision
    supervision/       SHRINK — JoinSet + generation cleanup only
    commands/          REPLACE Task commands with Execution commands
  ports/
    execution_commit.rs  NEW — Message + ExecutionOutcome commit
    approval / interaction — session-scoped, immutable after attach
```

### Target orchd-api surface

```text
AgentRuntimeApi
  attach_session / detach_session
  start_execution / steer_execution / request_cancel / execution_snapshot

ExecutionCommitPort
ApprovalPort
InteractionPort
RealtimeDeltaSink

SessionSubscription { committed, realtime }   # keep QoS split
```

Legacy Task request DTOs (`CreateTaskRequest`, `SubmitTaskInput`,
`TaskControlRequest`) and `TaskChanged` / `WorkChanged` observation have been
removed from the product wire surface. Storage may still parse legacy lifecycle
records for resume.

### protocol DTOs to add (Phase 1)

| Type | Notes |
|---|---|
| `ExecutionId` | distinct from `TurnId` / legacy `TaskId` |
| `ExecutionStatus` / `ExecutionOutcome` | Accepted→Running→terminal |
| `StartExecutionRequest` | includes committed context + ids |
| `ExecutionReceipt` / `InputReceipt` / `CancelReceipt` | oneshot ack, not event-hub |
| `SteerExecutionRequest` / `CancelExecutionRequest` | addressed by session+execution |
| `ExecutionSnapshot` | watch-lane projection |
| `MessageCommit` / `ExecutionOutcomeCommit` | durable identity fields |

### hostd cutover points

| Current | Target |
|---|---|
| `OrchTurnRunner` create_task + submit_input | `start_execution` after user Message commit |
| Task Idle + Work terminal inference | matching `execution_id` terminal only |
| global PersistSink / approval rebind | `SessionExecutionPorts` at attach |
| `SessionActor` (future) | sole Session/Turn writer; no await of full Execution in mailbox |

## 3. Phase Checklist

### Phase 0 — Contract freeze (now)

- [x] Design docs accepted as source of truth under `docs/`
- [x] Landing checklist written (`single-agent-runtime-landing.md`)
- [x] Mark Task/Work public types as migration-only
- [x] Freeze new features: spawn child task, detached mode, cross-task steer, poll-based completion (`AGENTS.md`)
- [x] No new product code depends on Task Idle / WorkChanged as truth

Exit: terminology frozen; old API labeled; no new Task-lifecycle features.

### Phase 1 — Execution DTOs + API

- [x] Add Execution DTOs to `piko-protocol` (`execution.rs`)
- [x] Add `AgentExecutor` + receipts to `orchd-api` alongside Task API
- [x] Introduce `SessionExecutionPorts` (immutable after attach)
- [x] Contract tests: start/steer/cancel receipts do not require SessionOutput

Exit: new API compiles; no global port rebinding on new path.

### Phase 2 — ExecutionActor vertical slice

- [x] `runtime/execution/` Actor loop from retained step/tools
- [x] Active Execution registry (≤1 root per Session)
- [x] Single `ExecutionFinalizer` for all terminal paths
- [x] In-memory prompt: text-only Model Step end-to-end (`tests/execution_api.rs`)
- [x] `wait_terminal` + cancel acceptance
- [x] Tool batches on Execution path (commit ToolCall/ToolResult, continue Model Steps)
- [x] Multi-step continuation with tools

Exit: every terminal path removes handle exactly once; panic finalized.

### Phase 3 — hostd root Turn

- [x] `HostExecutionCommitPort` projects Messages into HostState
- [x] Legacy PersistSink/task-shard bridge (`LegacyPersistExecutionCommitPort`)
- [x] `OrchTurnRunner` always uses Execution (Task turn path removed from runner)
- [x] Observation bridge publishes MessageCommitted + Running/Idle TaskChanged
- [x] Execution path registers user_interaction tools on `AgentExecutionRuntime`
- [x] `QueueSteer` / `TurnCancel` route to Execution API when active
- [x] MCP tool providers mirrored onto Execution runtime
- [x] Default path cutover (Execution is the only Turn path in OrchTurnRunner)
- [x] Turn terminal only from matching ExecutionOutcome in hostd state machine
- [x] Feature-flag or session path select: Task path off for new sessions (runner is Execution-only)

Exit: TUI first + subsequent Turns without root Task reuse.

### Phase 4 — Control + recovery

- [x] Steering at Model Step boundary (orchd + hostd wiring)
- [x] hostd follow-up queue → new Turn (shared `apply_turn_submit` drain after Completed)
- [x] `TurnCancel` → `request_cancel` → durable Cancelled
- [x] Session open: non-terminal historical Turns → interrupted/failed
- [x] Subscriber disconnect does not cancel Execution (hub resubscribe via `active_hubs`)

Exit: cancel / retry / reconnect / interrupt tests green.

### Phase 5 — Delete Task/Work

- [x] Remove Task turn path from `OrchTurnRunner` (Execution only)
- [x] Drop classic `Runtime` bootstrap from hostd turn runner
- [x] `AgentExecutionRuntime::bootstrap` installs workspace/todo tools (no task_control)
- [x] MCP registers on Execution only
- [x] Mark classic `orchd::Runtime` migration-only; update hostd/AGENTS docs
- [x] Execution path observation uses `SessionEvent::ExecutionChanged` (not TaskChanged/WorkChanged)
- [x] Execution path updates TUI via `AgentChanged` (no TaskLifecycle bridge)
- [x] hostd observation drops `TaskChanged` handler (tests use `ExecutionChanged`)
- [x] TaskControlProvider / builtin spawn tools opt-in (`runtime.enableTaskControl`, default false)
- [x] Remove root Task reuse from `start_root_turn` (one call → one task; matches Execution)
- [x] Remove Task commands, permanent mailbox, TaskRegistry, classic `Runtime`,
  `AgentRuntimeService`, `runtime/task`, `TaskControlProvider`, and Task-based
  multi_agent / runtime_integration test suites
- [x] orchd product surface is `AgentExecutionRuntime` only
- [x] Drop `AgentRuntime` trait from orchd-api (Execution API remains)
- [x] TUI ignores `TaskLifecycle` (agent panel driven only by `AgentChanged`)
- [x] Drop remaining Task wire DTOs / `Event::TaskLifecycle` where unused
  (storage may still parse legacy lifecycle records)

Exit: workspace tests pass without old runtime path.

### Phase 6 — Storage + docs

- [x] Execution path: one root shard via `ensure_task_shard` (header once; Messages across Turns)
- [x] Execution path: stop writing Work lifecycle on `commit_execution_outcome`
- [x] Legacy task shard read policy: keep reading Lifecycle/WorkLifecycle; Messages authoritative for resume
- [x] Stop writing Task/Work lifecycle from product path (legacy shard read only)
- [x] Update hostd runtime/logging docs for Execution observation + shard policy
- [x] Broader orchd docs cleanup; retire Task-as-current docs

Exit: new sessions use Execution only; one root `tasks/{id}.jsonl` for transcript;
legacy shard read remains; docs point at Execution as current.

### Phase 7 — Multi-agent (later)

Only after single-agent invariants are stable. Out of current landing scope.

## 4. First Vertical Slice (landed)

The initial Execution vertical slice is complete (`execution_api` + hostd Turns).
Historical checklist below is retained for context:

```text
1. protocol: ExecutionId + StartExecutionRequest + ExecutionOutcome
2. orchd-api: AgentExecutor + ExecutionCommitPort
3. orchd: ExecutionActor via AgentExecutionRuntime
4. test: in-memory PersistSink + faux LLM (`tests/execution_api.rs`)
5. hostd TurnSubmit → start_execution (default / only path)
```

Do not rename `runtime/step` or tools until multi-agent (Phase 7) needs it.

## 5. Risk Guards

| Risk | Guard |
|---|---|
| Dual path double-commit | Session binds one path; assert one terminal writer |
| EventHub used as ack | New path: mpsc + oneshot only |
| Global port rebind | Ports frozen at `attach_session` |
| Await cycle Session↔Execution | Session commits user msg before spawn; never awaits full Execution in mailbox |
| TUI Task panel | Derive activity from Execution/Session projection |

## 6. Verification Commands

```bash
# Execution path
cargo test -p piko-protocol
cargo test -p orchd-api
cargo test -p orchd --test execution_api
cargo test -p hostd
cargo test -p tui

# Landing gate
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
