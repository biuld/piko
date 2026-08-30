# D-68: AgentInput work model and control plane

> Status: implemented (slices 1–6.4; recovery and control-plane evidence)
> Implements: [F-51](../features/F-51-agent-control-plane.md)
> Decisions: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md), [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md), [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Keep Session, AgentInstance, and ModelStep as the invariant grains. Put
AgentInput between Agent and ModelStep as the stimulus. When an input is
applied as root, that `input_id` is the identity of the current work. hostd
persists facts and publishes `AgentWorkSnapshot`; orchd serializes admission
and operates live work without making private actor state durable authority.

Turn, Run, and Execution are not product scopes. Their product types, IDs,
maps, commands, and projections have been removed; the host retains only an
internal observation registry keyed by `(session_id, input_id)`. Schema-v4
stays; this is not a journal-format rewrite. The only client in this design
is the TUI.
`piko-desktop` is out of scope.

## Design constraints

- The schema-v4 append-only journal remains the sole durable authority.
- `session.json` remains immutable identity; current query paths read
  write-time projections.
- hostd owns durable user-visible state; orchd owns live Agent execution.
- `piko-protocol` carries DTOs only and remains a shared leaf.
- One AgentInstance admits commands serially and has at most one active root
  AgentInput.
- A successful admission response follows its durable commit.
- Work identity is `root_input_id`. Do not keep `turn_id`, `run_id`, or
  `execution_id` as protocol or product handles. orchd may key an internal
  actor by `root_input_id`.
- Processing start/finish and interruption are facts on the root AgentInput,
  not an Execution aggregate. No multi-attempt product model.
- Old product types and compatibility paths are removed, not retained as
  adapters: `ChatSubmit` / `ChatSubmitMessage`, `QueueSteer` /
  `QueueSteerMessage`, `TurnCancel`, host `TurnRecord` / `steer_queue`, TUI
  local follow-up stacks, orchd `send_agent_input` / `steer_agent` /
  request-id cancel shims, `UserTurnView`, `RunId`, `ExecutionId`,
  `StoredExecution` as a product aggregate, and `active_agent_runs`.
- There is no dual-write period and no unpublished shadow `AgentWorkSnapshot`.
  The snapshot is the published current-state projection the TUI consumes.
- Desktop is out of scope. Protocol and host changes are not constrained by
  `piko-desktop` continuing to compile against the deleted commands.
- Realtime streams remain lossy and reconcilable from host read models.

## Current-state decomposition

| Existing concept | Keep as | Required change |
|---|---|---|
| AgentInstance actor and snapshot | Primitive/runtime owner | Active work is the current root AgentInput |
| `AgentInputQueued` runtime record | Partial AgentInput fact | Start, steer, and follow-up admission/disposition; delete the old queue type |
| host `TurnRecord` / `UserTurnView` / `turn_id` | Delete | User-origin rows are AgentInputs; TUI groups them for display |
| orchd `run_id` / `active_agent_runs` | Delete | Key live work by `root_input_id` |
| Execution actor / `StoredExecution` / `execution_id` | Delete as product | Internal actor keyed by root input; start/finish/interrupt facts on that root |
| `ModelStepCommittedV1` | Core primitive fact | Retain atomic relation and connect applied steer inputs |
| host `steer_queue` | Delete | Durable pending AgentInputs bound to the active root |
| TUI `follow_ups` / local queue overlay | Delete | TUI reads pending AgentInputs from `AgentWorkSnapshot` |
| `ChatSubmit` / `QueueSteer` / `TurnCancel` | Delete | `AgentInputSubmit`, `cancel_agent_input`, `AgentInterrupt` |
| orchd `send_agent_input` / `steer_agent` shims | Delete | `submit_agent_input` is the only admission entry |
| Agent activity / foreground | Partial projection | Materialize once from primitive facts and pending action; publish it |

Slice 1 added `AgentInterrupt` and already proves that control can address an
AgentInstance even when no host Turn exists. Later slices put admission and
projection on that same boundary and delete Turn/Run/Execution leftovers.

## Canonical data model

### AgentInput

The protocol and host domain use one immutable admission DTO:

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
    caller_agent_instance_id: Option<AgentInstanceId>,
}
```

`input_id` is the durable control identity. `request_id` provides caller
idempotency. They are not the same concept. There is no `user_turn_id`.

The stored input has one effective disposition:

```text
admitted
├── pending_follow_up ──→ applied_as_root
├── pending_steer ──────→ applied_to_step
├── applied_as_root
└── cancelled
```

A rejected admission writes no AgentInput. Duplicate `request_id` plus the
same normalized proposal returns its original receipt; conflicting reuse
returns an idempotency conflict.

When disposition is `applied_as_root`, this input is the work identity. Every
later fact in that work carries `root_input_id = input_id`.

### Active work (derived, no extra ID)

Active work is the causal closure of one unfinished root AgentInput: steers,
ModelSteps, tools, pending actions, interruption, and outcome that share that
`root_input_id`. At most one root may be non-terminal per AgentInstance.

Status is derived from input application, processing start/finish on that
root, pending action, interrupt, ModelStep, and tool facts. If presentation
needs a normalized enum, the read-model projector computes it. No generic
status-changed event is appended solely for display.

A steer records the `root_input_id` captured at acceptance and the ModelStep
that later consumed it.

Do not add `RunProjection`, `AgentRunRecord`, `UserTurnView`, or an
`executions: root → Vec<attempt>` aggregate.

### ModelStep

The implemented `ModelStepCommittedV1` remains the durable atomic boundary for
assistant output and ordered tool declarations. It already has independent
identity and recovery semantics. Before issuing a model request, orchd
reserves its `ModelStepId` and commits each steer input's application to that
step. The later ModelStep commit uses the same ID. This prevents a crash after
request dispatch from making an applied steer pending again.

No separate durable `model_step_started` fact is required for product state.
Live step progress comes from realtime runtime observation; after a crash, an
applied root without a terminal fact is closed by interruption recovery.

## Durable event model

Add the smallest versioned fact family missing from the existing journal:

```text
agent_input_admitted_v1
agent_input_disposition_changed_v1
```

Continue using:

```text
message_committed
model_step_committed_v1
pending_action facts                          existing feature authority
agent lifecycle facts                         existing feature authority
root processing start / finish / interrupt    facts on the root AgentInput
```

Do not keep `execution_started` / `execution_finished` as the product
processing boundary, and do not add parallel Run or Turn lifecycle events.
`request_id` on an input remains caller idempotency; `root_input_id` is work
identity. The semantic commit must make the immutable input and its initial
disposition visible together.

Do not add:

- independent `turn_status_changed` / `run_status_changed` facts;
- an independently mutable queue record;
- generic foreground transitions;
- `AgentRunRecord`, `agent_runs`, `StoredExecution` as a product aggregate;
- `model_step_started` solely to persist transient streaming state;
- compatibility aliases for deleted submit/steer/cancel commands;
- `turn_id`, `run_id`, or `execution_id` fields on new or rewritten DTOs.

### Fact relationships and validation

- every input references an existing Session and AgentInstance;
- one request ID maps to one normalized immutable input proposal;
- `applied_as_root` makes the input its own `root_input_id`;
- `pending_steer` captures the exact active `root_input_id` at admission;
- `applied_to_step` references the captured root and a reserved next
  ModelStepId; a later committed step with that ID must agree;
- cancelled inputs have no application fact;
- only one root projects as non-terminal per AgentInstance;
- terminal/interrupt facts for a root are idempotent and cannot imply
  conflicting derived outcomes;
- queue order is journal admission order, not client wall-clock time.

### Semantic commit groups

Immediate start:

```text
agent_input_admitted(initial = applied_as_root)
+ processing-started fact on that root
+ starting user message
```

Follow-up while busy:

```text
agent_input_admitted(initial = pending_follow_up)
```

Queue advancement:

```text
agent_input_disposition_changed(applied_as_root)
+ processing-started fact on that root
```

Steer acceptance:

```text
agent_input_admitted(initial = pending_steer, target_root_input_id)
```

Steer application at a deterministic boundary, before model request dispatch:

```text
steer message commit
+ agent_input_disposition_changed(applied_to_step, target_root_input_id,
                                  reserved_step_id)
... model request and streaming ...
+ model_step_committed_v1(reserved_step_id, assistant/tool declarations)
```

If the process fails between application and the ModelStep commit, replay sees
an applied input and unfinished active work. Interruption recovery closes that
root; it never redelivers that input.

Reliable publication follows append success.

## Aggregate and projections

### Session aggregate and indexes

```text
agent_inputs: input_id → StoredAgentInput
model_steps: step_id → StoredModelStep

active_root_by_agent: agent_id → root_input_id     derived index
pending_inputs_by_agent: agent_id → ordered input_id[]  derived index
input_by_request: request_id → input_id             derived index
```

Do not add `agent_runs`, `executions`, or `turns` maps. Trajectory and prompt
assembly rekey to `root_input_id`.

Indexes are rebuilt deterministically from event order. They accelerate
validation and projection; maps and indexes are not separate authorities.

### Current read model

`readmodels/current.json` gains one per-AgentInstance projection:

```rust
AgentWorkSnapshot {
    agent_instance_id: AgentInstanceId,
    lifecycle: AgentLifecycle,
    foreground: AgentForeground,
    active_work: Option<ActiveWorkSnapshot>,
    pending_steers: Vec<AgentInputSummary>,
    queued_inputs: Vec<AgentInputSummary>,
    pending_action: Option<PendingActionSummary>,
}

ActiveWorkSnapshot {
    root_input_id: AgentInputId,
    state: AgentWorkViewState,
    active_model_step_id: Option<ModelStepId>,
    started_at: i64,
}
```

`AgentInputSummary` exposes input ID, origin, preview, admission order/time,
delivery, and disposition.

The host derives foreground with one shared priority:

```text
requires_action > cancelling > running > queued > idle
```

`AgentWorkSnapshot` is the published per-AgentInstance contract written into
`readmodels/current.json`. It is not a private shadow. `pending_action` and
active ModelStep identity are part of that contract. The TUI does not keep
`active_turns`, `QueueEvent`, or `UserTurnView` as control surfaces.

## Runtime admission boundary

`AgentRuntimeApi` accepts the canonical proposal and returns a receipt:

```rust
submit_agent_input(input) -> AgentInputReceipt {
    input_id,
    disposition,
    queued_position: Option<u32>,
}

interrupt_agent(session_id, agent_instance_id) -> AgentInterruptReceipt
cancel_agent_input(session_id, agent_instance_id, input_id)
    -> AgentInputCancelReceipt
```

`send_agent_input`, `steer_agent`, `run_agent`, and request-id
`cancel_agent_input` are removed. Receipts do not carry `run_id` or
`execution_id`. `input_id` is durable control identity; `request_id` is
caller idempotency.

The AgentActor is the serialization point:

1. capture lifecycle and active root input;
2. evaluate delivery and capacity;
3. construct the complete durable transition proposal;
4. commit through a host-owned admission port;
5. mutate private actor state after acknowledgement;
6. return the receipt and publish a refreshed snapshot.

For steer, the actor captures `target_root_input_id`. Delivery rejects if that
root is no longer active; it never substitutes the next root. Follow-up
advancement commits `applied_as_root` before model work launches.

This ordering intentionally makes host persistence part of admission. If the
commit fails, the runtime has not accepted the input.

## Host application control plane

Create one application use case behind protocol dispatch:

```rust
AgentWorkControl {
    submit(input),
    interrupt_current(session_id, agent_instance_id),
    cancel_input(session_id, agent_instance_id, input_id),
}
```

Wire replacement (delete the old commands; do not keep them as adapters):

| Deleted surface | Canonical operation |
|---|---|
| `ChatSubmit` / `ChatSubmitMessage` | `AgentInputSubmit` with `FollowUp` delivery |
| `QueueSteer` / `QueueSteerMessage` | `AgentInputSubmit` with `Steer` delivery |
| queued `TurnCancel` | `cancel_input` on the pending AgentInput |
| running `TurnCancel` | `interrupt_current` on the AgentInstance |
| `AgentInterrupt` | keep; `interrupt_current` |
| `message_agent when=queue` | agent AgentInput with `FollowUp` |
| `message_agent when=steer` | agent AgentInput with `Steer` |
| `interrupt_agent` tool | runtime-local policy then the same interrupt operation |

Protocol handlers dispatch into `AgentWorkControl`. There is no remaining
submit/steer/cancel command family beside the canonical operations.

## Client cutover (TUI only)

`piko-tui` consumes `AgentWorkSnapshot` from hostd. `piko-desktop` is not
updated by this design.

- composer routing checks active work/foreground instead of `active_turns`;
- Enter steers any active root, including detached work;
- Alt+Enter submits a follow-up and reconciles by input ID;
- Esc interrupts the viewed AgentInstance;
- dequeue cancels an authoritative input ID (`cancel_input`);
- queue counts, previews, and pending steers come only from
  `AgentWorkSnapshot`;
- timeline rows are user-origin AgentInputs grouped by root; no `UserTurnView`;
- delete `SessionUiState::follow_ups` and any local overlay onto host queue
  counts.

The TUI may show a command as submitting before its receipt. Rejection or the
next authoritative snapshot removes that optimism. Restart, agent switching,
and a second TUI never depend on local queue history.

## Races and recovery

- **Steer versus root terminal:** serialization chooses one. A committed steer
  remains bound to that root and is consumed or explicitly cancelled; a later
  root cannot inherit it.
- **Follow-up versus terminal:** the input either starts immediately or is
  admitted pending and then advanced. Both paths create one input; a start
  creates one root.
- **Cancel versus queue advancement:** both address one input ID; exactly one
  transition wins.
- **Interrupt versus terminal:** terminal-first returns `accepted: false`;
  interrupt-first records intent for that root and cannot affect its
  successor.
- **Host crash after acceptance:** journal replay restores pending input,
  active root, queue, and foreground.
- **orchd crash after admission:** attach reconstructs pending input and
  active root from the host projection; unfinished work follows interruption
  recovery on that root.
- **Projection failure:** the journal remains authoritative and write-time read
  models rebuild through existing CQRS recovery.

## Refactor sequence

Each slice compiles. Later slices replace the old surface rather than wrapping
it. Do not land a dual-write or leave a deleted type behind as an adapter.

### Slice 1 — Agent interrupt (implemented)

Keep `AgentInterrupt` end to end. Esc targets the viewed AgentInstance.

### Slice 2 — Primitive facts and published work projection (implemented)

- AgentInput facts, `root_input_id` correlation, reducers, and invariants are
  the admission/storage path.
- Publish `AgentWorkSnapshot` in `readmodels/current.json` with `active_work`
  keyed by `root_input_id`, plus `pending_action` and active ModelStep. It is
  not a shadow.

### Slice 3 — Canonical runtime admission only (implemented)

- Start, steer, and follow-up admit through `submit_agent_input` only.
- orchd `send_agent_input`, `steer_agent`, `run_agent`, `AgentCommand::Run`,
  and `AgentRunAcceptance` are gone. Observation is
  `wait_agent_input_started` / `wait_agent_input_completion` on the snapshot.
- Receipts use `input_id` / `root_input_id`. Prompt staging, tool restriction,
  `root_input_id` and `message_id` stay on `AgentInputRuntime` (not durable
  duplicates of the canonical AgentInput fact).

### Slice 4 — Host control plane (implemented)

- `AgentWorkControl` is the only application use case behind dispatch.
- `ChatSubmit` / `QueueSteer` / `TurnCancel`, host `TurnRecord`, and
  `steer_queue` are gone.
- Queue, foreground, and active work project from AgentInput facts and
  `AgentWorkSnapshot`.

### Slice 5 — TUI cutover (implemented)

- TUI and client-core consume `AgentWorkSnapshot`. Local follow-up stacks and
  client `active_turns` maps are gone.
- Composer busy/steer/queue and desktop chrome compile against `agent_work`.
- `piko-desktop` remains out of product scope.

### Slice 6 — Remaining leftover cleanup (implemented)

Land in this order. Do not introduce a replacement Turn/Run/Execution product
type; keep AgentInput / `agent_work` / `AgentWorkReport`.

1. **Host observation port (implemented).** Fold `AgentRunRunner::run_agent`
   into submit + wait. Delete `AgentRunInput` as an admission DTO. Rekey
   in-process observation registry is keyed by `(session_id, input_id)` and is
   not persisted or exposed as product state.
2. **Turn wire leftovers (implemented).** `TurnEvent`, `TurnSnapshot`,
   `TurnStatus`, `LifecycleEvent`, `ServerMessage::TurnLifecycle`, and
   `SessionSnapshot.active_turns` are gone. Tests wait on
   `AgentWorkSnapshot` / `AgentInputSubmitted` / work terminal facts.
   Remaining presentation wording uses “work diff”; no Turn wire types or
   fields remain in the protocol.
3. **Execution product maps (implemented).** `StoredExecution` and the
   `executions` aggregate map are gone. The journal stores
   `agent_input_processing_started_v1` / `agent_input_processing_finished_v1`
   as facts on the root AgentInput (the finish fact carries the
   `AgentWorkReport`); `execution_started` / `execution_finished` are no
   longer decodable (`READER_VERSION` 3). Per-root processing state lives on
   `StoredAgentInput.processing`; transcript head and model-step continuity
   are derived from committed messages and steps. orchd's internal execution
   actor is keyed directly by `root_input_id`.
4. **Rekey remaining grains (implemented).** The
   `execution_id` / `run_id` / `internal_execution_id` grains are gone from
   protocol, journal, orchd, and read models; every commit, usage fact,
   trajectory record, abort marker, and realtime delta carries
   `root_input_id`. `ModelStepCommit` / `ModelStepBoundary` /
   `MessageCommit` / `UsageAttribution` / commit receipts carry
   `root_input_id` only; the orchd execution actor and
   `SessionExecutionScope` are keyed by `root_input_id` and
   `ExecutionIdentity` has no second id; the trajectory read model keys runs
   by root input and drops `execution_to_run`; the dead `AgentRunEvent` /
   `ServerMessage::AgentRunLifecycle` wire surface is deleted.
5. **Recovery and evidence (implemented).** Crash-point, race, restart, and
   multi-client reconciliation evidence is recorded in V-64. F-51 is now
   implemented.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | AgentInput/receipt/disposition DTOs, `AgentWorkSnapshot`, `AgentInputSubmit`; delete ChatSubmit/QueueSteer/TurnCancel and `turn_id` / `run_id` / `execution_id` product fields |
| `piko-session-store` | AgentInput facts, root indexes, published work projection; delete Turn/Run/Execution aggregates |
| `piko-orchd-api` | Canonical admission/control; delete send/steer/run shims and Run/Execution receipts |
| `piko-orchd` | AgentActor serialization, root-bound steer, follow-up advancement; internal actor keyed by root input |
| `piko-hostd` | AgentWorkControl, journal commits, work projections; delete Turn lifecycle, Run maps, `steer_queue` |
| `piko-client-core` | Consume `AgentWorkSnapshot`; delete Turn/queue reduction |
| `piko-tui` | Projection-driven routing, queue, steer, interrupt; timeline grouped by root AgentInput |
| `piko-desktop` | Out of scope |

No `island-rs` lifecycle changes are required. Reusable presentation controls
may live there later, while piko retains domain IDs and intent mapping.

## Verification

- Event/reducer transition tables for AgentInput and causal-root relations,
  including invalid references, idempotency conflict, and replay.
- Compile-time absence of deleted commands, Turn/Run/Execution product types
  and IDs, host `steer_queue`, orchd send/steer shims, and TUI local
  follow-up authority.
- Property tests that queue, active work, and foreground are pure
  deterministic projections of one aggregate.
- Crash-point tests before and after each admission/application/start/terminal
  commit.
- AgentActor races for steer-terminal, follow-up-terminal, cancel-advance, and
  interrupt-successor isolation.
- Host tests proving TUI commands call only `AgentWorkControl`.
- Fresh-TUI and two-TUI reconciliation without local queue history.
- Cross-process E2E for detached steer/interrupt and queued cancellation after
  restart.
- `cargo fmt --all`, workspace tests, and workspace clippy with warnings
  denied.

## Alternatives considered

- **Add durable Turn and Execution state machines first:** rejected because
  product state is derivable from AgentInput and ModelStep.
- **Keep Run as a derived identity with its own `run_id`:** rejected. The
  work identity is the root `input_id`; a second ID recreates the old
  hierarchy.
- **Keep Execution for actor/recovery correlation:** rejected as a product
  scope. An internal actor keyed by `root_input_id` is enough; start/finish
  facts attach to that root.
- **Keep UserTurnView for user-origin rows:** rejected. Those rows are
  AgentInputs; the TUI can group them without a host Turn type.
- **Use Agent activity alone:** rejected because it lacks stable input,
  queue, idempotency, and race identity.
- **Keep steer ephemeral:** rejected because acknowledged input can disappear
  before the next ModelStep.
- **Keep queues client-local:** rejected because restart and multiple clients
  cannot converge.
- **Dual-write with compatibility adapters:** rejected.
- **Update desktop in the same cutover:** rejected as out of scope.
