# D-68: Agent work lifecycle and control plane

> Status: proposed (Slice 1 agent interrupt implemented)
> Implements: [F-51](../features/F-51-agent-control-plane.md)
> Decisions: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md), [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md), [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Build one vertical lifecycle from input admission through queueing, Run,
Execution, ModelStep, storage, projection, and product control. The design must
make detached and Turn-backed Agent work behave identically where the product
intent is identical, while preserving hostd as durable user-visible authority
and orchd as runtime authority.

## Current-state gaps

| Concern | Current authority/path | Gap |
|---|---|---|
| User Turn | hostd `HostState.active_turns` | Process-local lifecycle is not fully journaled |
| Agent activity | orchd `AgentSnapshot.activity` mirrored by host | Does not identify active Run/input/queue |
| Follow-up queue | durable `AgentInputQueued` in session journal and AgentActor | Missing from `SessionSnapshot`; host `follow_up_count` is zero; TUI shadows it |
| Steer | separate host command, runtime execution mailbox, host `steer_queue` | Host gate requires a Turn; accepted pending state is not durable/queryable |
| Run/Execution | journal execution facts | Projection is keyed by Run ID, preventing multiple attempts per Run |
| Cancellation | Turn, Agent, and queued-input paths | No single application control service resolves intent and correlated state |
| Display | active Turns + Agent activity + pending actions + local queue | Multiple clients can disagree after restart or races |

Slice 1 added `AgentInterrupt` and proved that host can preserve Turn terminal
authority while forwarding detached cancellation. The remaining slices replace
the fragmented admission and projection paths.

## Canonical domain model

### AgentInput

Introduce one protocol/domain DTO for durable admission:

```rust
AgentInput {
    input_id: AgentInputId,
    request_id: RequestId,
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    origin: AgentInputOrigin,
    delivery: AgentInputDelivery,
    content: MessageContent,
    submitted_at: i64,
    source_turn_id: Option<TurnId>,
    caller_agent_instance_id: Option<AgentInstanceId>,
    detached_recipient_agent_instance_id: Option<AgentInstanceId>,
}
```

`input_id` is the durable control identity. `request_id` is caller idempotency
identity. They may initially share a generated value, but remain different
concepts. Origin is typed as user, agent, or system; it is not inferred from
which command happened to arrive.

State is represented by journal transitions:

```text
admitted
├── queued ──→ applied_to_run
├── pending_steer ──→ applied_to_step
├── applied_to_run
└── cancelled
```

Rejected admission is a command result and writes no input fact. Duplicate
admission returns the existing projection. A conflicting request ID fails.

### Turn, Run, Execution, and ModelStep

```rust
TurnRecord {
    turn_id,
    starting_input_id,
    agent_instance_id,
    status,
    timestamps,
    terminal,
}

RunRecord {
    run_id,
    starting_input_id,
    agent_instance_id,
    source_turn_id: Option<TurnId>,
    state,
    execution_ids: Vec<ExecutionId>,
    terminal,
}

ExecutionRecord {
    execution_id,
    run_id,
    attempt,
    state,
    model_step_ids,
    terminal,
}
```

A user start/follow-up allocates a Turn ID before orchd admission so it can be
included in the AgentInput proposal; allocation alone is not durable state.
The AgentActor's admission commit asks the host-owned port to append the Turn
and input facts atomically with the queue disposition or Run/Execution start.
A queue disposition leaves the Turn queued. An accepted start atomically
relates input, Turn, Run, and first Execution start facts.

Detached agent/system input omits `source_turn_id`; its Run is otherwise
identical. Steer references `target_run_id` in its durable pending transition
and never creates a Turn or Run.

## Journal facts

Add versioned facts to the schema-v4 event family:

```text
agent_input_admitted_v1
agent_input_queued_v1
agent_input_applied_v1
agent_input_cancelled_v1

turn_admitted_v1
turn_status_changed_v1
turn_finished_v1

run_admitted_v1
run_status_changed_v1
run_finished_v1

model_step_started_v1
```

Existing `execution_started`, `execution_finished`, and
`model_step_committed` remain, but validation changes:

- Execution references an existing Run, not merely a repeated `run_id` string.
- ModelStep references the same Run and Execution.
- source Turn, when present, must address the same AgentInstance and starting
  input.
- queued/applied/cancelled transitions must follow the AgentInput state machine.
- a steer application records `run_id`, `execution_id`, and the consuming
  `model_step_id`; it is committed with `model_step_started` and cannot apply
  to a Run other than the one captured when accepted.
- one AgentInstance may have only one non-terminal Run in a valid aggregate.
- queue order is the admission revision/order, not client timestamp.

The current `AgentInputQueued`/`AgentInputDequeued` events are upcast into the
new queue transition model when read. New writes use the new facts. Existing
execution facts without explicit Run facts are reconstructed as legacy Runs
during upcast/read-model generation; the journal generation stays v4.

### Commit ordering

For a new Run:

```text
turn_admitted? → agent_input_admitted → run_admitted
→ execution_started → input message committed
```

For a follow-up queued behind active work:

```text
turn_admitted? → agent_input_admitted → agent_input_queued
```

When it starts:

```text
agent_input_applied(run) → run_admitted → execution_started
→ turn_status_changed(running)?
```

For steer:

```text
agent_input_admitted(pending_steer, target_run)
... deterministic boundary ...
message_committed → agent_input_applied(run, execution, model_step)
→ model_step_started
```

The acceptance response is not sent until the relevant admission commit
succeeds. Reliable publication occurs after the commit acknowledgement.

## Aggregate and invariants

`SessionAggregate` gains explicit maps instead of reconstructing relationships
from scattered fields:

```text
inputs: input_id → StoredAgentInput
turns: turn_id → StoredTurn
runs: run_id → StoredRun
executions: execution_id → StoredExecution
agent_queues: agent_instance_id → ordered input_id[]
agent_active_runs: agent_instance_id → run_id
```

`executions` remains keyed by Execution ID. The current projection keyed by Run
ID is removed. Secondary relationships are derived and validated during apply,
not maintained as competing mutable truth.

Every transition validates Session, AgentInstance, source identity, lifecycle,
current Run generation, and idempotency. Live append preflight remains
transactional; replay fails closed on contradictory required facts.

## Materialized read model

`readmodels/current.json` becomes sufficient for current product state. Add a
per-AgentInstance projection:

```rust
AgentWorkSnapshot {
    agent_instance_id,
    lifecycle,
    foreground,
    active_run: Option<ActiveRunSnapshot>,
    pending_steers: Vec<AgentInputSummary>,
    queued_inputs: Vec<AgentInputSummary>,
    pending_action: Option<PendingActionSummary>,
}

ActiveRunSnapshot {
    run_id,
    source_turn_id: Option<TurnId>,
    execution_id,
    execution_attempt,
    state,
    active_model_step_id: Option<ModelStepId>,
    started_at,
}
```

`AgentInputSummary` exposes the stable input ID, origin, content preview,
submission order/time, delivery, optional Turn, and state. Full content stays
in the durable journal/projection path needed for dequeue restoration and need
not be repeated in every lightweight event.

`SessionSnapshot` carries `agent_work`. Existing `active_turns` remains during
client migration but is derived from the same stored Turn records. The
standalone `QueueEvent` becomes a compatibility projection and is later
removed once TUI/desktop consume `AgentWorkSnapshot`.

Foreground is derived once in protocol/host projection with priority:

```text
requires_action > cancelling > running > queued > idle
```

Agent lifecycle and foreground remain separate. No client calls
`AgentForeground::project` with partially different inputs after migration.

## Runtime admission service

`AgentRuntimeApi` remains the orchd control plane but uses the canonical input
DTO and receipts:

```rust
submit_agent_input(input) -> AgentInputReceipt {
    input_id,
    disposition,
    run_id: Option<RunId>,
    queued_position: Option<u32>,
}

interrupt_agent(session, agent) -> AgentInterruptReceipt
cancel_agent_input(session, agent, input_id) -> AgentInputCancelReceipt
```

The AgentActor is the serialization point for one AgentInstance:

1. capture current Run generation;
2. evaluate lifecycle and delivery;
3. propose the durable transition through `AgentCommitPort`;
4. mutate private runtime state only after commit acknowledgement;
5. return the receipt and publish the new snapshot.

For steer, the captured Run ID is sent to the Execution actor. The Execution
rejects a mismatched generation/Run rather than applying the input to whatever
happens to be current later.

Follow-up advancement atomically commits queued-input application and Run start
before launching model work. A failed launch either leaves the row queued or
commits a failed Run according to whether Execution start was durably admitted;
it never silently removes the input.

## Host application control service

Create a focused application port/use case instead of adding more semantics to
protocol dispatch or `AgentRunRunner`:

```rust
AgentWorkControl {
    submit(input, optional_turn_intent),
    interrupt_current(session, agent),
    cancel_input(session, agent, input_id),
    cancel_turn(session, turn_id),
}
```

The service coordinates host Turn commits with the runtime admission port and
publishes updated work projections. Protocol handlers are adapters only.

Compatibility mappings:

| Existing surface | Canonical use case |
|---|---|
| `ChatSubmit[Message]` | user `FollowUp` input with Turn intent |
| `QueueSteer[Message]` | user `SteerActive` input without new Turn |
| `TurnCancel` queued | resolve Turn → starting input → `cancel_input` |
| `TurnCancel` running | resolve Turn → AgentInstance → `interrupt_current` |
| `AgentInterrupt` | `interrupt_current` |
| `message_agent when=queue` | agent `FollowUp` input, no Turn |
| `message_agent when=steer` | agent `SteerActive` input, no Turn |
| `interrupt_agent` tool | `interrupt_current` through runtime-local caller policy |

After client migration, a single product `AgentInputSubmit` command may replace
the Chat/QueueSteer duplication. Compatibility commands remain decoders, not
separate behavior.

## TUI and desktop migration

Both clients consume `AgentWorkSnapshot` through shared client-core where
possible.

- Composer routing checks `foreground`/active Run, not `active_turns`.
- Enter steers any active Run, including detached work.
- Alt+Enter submits follow-up and waits for the authoritative input receipt.
- Esc interrupts the viewed AgentInstance.
- Dequeue selects an authoritative queued `input_id`; restoring content uses
  the host-projected input, not a TUI-local record.
- Queue counts/previews and pending steer state come from the same projection.
- Session restart, agent switch, and a second client display identical state.

Optimistic UI may show a command as submitting, but it must reconcile by input
ID and remove optimism on rejection or the next authoritative snapshot.

## Failure, races, and recovery

- **Steer versus terminal:** admission captures active Run ID; terminal-first
  rejects, steer-first persists against that Run and is consumed or explicitly
  cancelled during its terminal sequence.
- **Follow-up versus terminal:** actor serialization yields either queued then
  advanced or immediately started; both produce one input and one Run.
- **Interrupt versus terminal:** returns benign `accepted: false` when terminal
  wins; never interrupts a later Run.
- **Cancel versus queue advance:** both address input ID and serialize in the
  AgentActor; exactly one transition wins.
- **Host crash after acceptance:** journal replay restores pending steer/queue
  and Turn relation before attach.
- **Orchd crash after durable admission:** attach rebuilds actor queue and active
  work from the host projection; unfinished Execution recovery uses existing
  abort policy.
- **Client disconnect:** no lifecycle state is lost; reconnect reads the
  materialized projection.
- **Projection failure:** journal remains authoritative and write-time read
  models rebuild by the existing CQRS recovery mechanism.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Canonical AgentInput/receipt/state, Run/Turn facts, AgentWorkSnapshot, compatibility DTO mapping |
| `piko-session-store` | Input/Turn/Run facts, validation, upcasting, aggregate maps, multi-attempt Execution relation |
| `piko-orchd-api` | Canonical admission/control port contracts |
| `piko-orchd` | AgentActor durable admission and Run-bound steer; queue/control serialization |
| `piko-hostd` | AgentWorkControl service, journal commits, materialized projection, compatibility command adapters |
| `piko-client-core` | Sole client reduction of AgentWorkSnapshot and incremental updates |
| `piko-tui` | Remove local follow-up/steer authority; projection-driven interaction |
| `piko-desktop` | Consume the same projection and controls |

## Reusable infrastructure

No `island-rs` lifecycle change is required. Island may render generic status,
queue, or control components later, but piko owns AgentInput, Turn, Run, and
host projection semantics.

## Verification

- Schema/aggregate transition-table tests for every AgentInput, Turn, Run, and
  Execution relation, invalid transition, idempotent retry, and legacy upcast.
- Crash-point tests after every admission/queue/apply/start/terminal commit.
- AgentActor race tests for steer-terminal, follow-up-terminal,
  cancel-advance, and interrupt-new-run isolation.
- Host tests that compatibility commands call one control service and produce
  the same journal/read-model facts.
- Reconciliation tests that a fresh TUI/client-core instance reconstructs
  active work, pending steers, and queues with no local history.
- Cross-process E2E for detached steer, detached interrupt, queued cancellation
  after restart, and two-client convergence.
- `cargo fmt --all`, workspace tests, and workspace clippy with warnings denied.

## Alternatives considered

- **Keep Turn as the universal root:** rejected because detached Run is a
  first-class runtime path and synthetic Turns falsify user history.
- **Use Agent activity as the complete lifecycle:** rejected because it has no
  stable input, Run, queue, or attempt identity.
- **Treat steer as an ephemeral message:** rejected because acknowledged input
  can be lost before the next ModelStep boundary.
- **Keep queues client-local:** rejected because restart and multiple clients
  cannot converge.
- **Expose Execution control to clients:** rejected because retries/recovery
  make Execution an unstable product handle.
- **Create a second control state store:** rejected; the session journal and
  materialized read models already own durable truth.

## Rollout

1. **Slice 1 — interrupt:** agent-addressed interrupt for Turn-backed and
   detached work (implemented; V-64).
2. **Slice 2 — schema/model:** AgentInput, durable Turn/Run facts, aggregate
   invariants, compatibility upcasts, and multi-attempt Execution indexing.
3. **Slice 3 — admission:** AgentActor commits start/steer/follow-up transitions
   before acceptance and binds steer to Run identity.
4. **Slice 4 — host control/projection:** AgentWorkControl and materialized
   AgentWorkSnapshot; compatibility commands delegate to it.
5. **Slice 5 — clients:** client-core/TUI/desktop consume the authoritative
   projection; remove local queue and process-local steer authority.
6. **Slice 6 — recovery/E2E:** crash matrix, multi-client convergence, remove
   superseded compatibility projection internals, and mark F-51 implemented.
