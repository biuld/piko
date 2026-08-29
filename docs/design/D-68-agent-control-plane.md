# D-68: Two-layer Agent work model and control plane

> Status: proposed (Slice 1 agent interrupt implemented)
> Implements: [F-51](../features/F-51-agent-control-plane.md)
> Decisions: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md), [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md), [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Separate Agent work into primitive facts and derived scopes. AgentInstance,
AgentInput, ModelStep, and causal lifecycle facts form the lower layer. Run,
Execution, Turn, Queue, and Foreground remain stable upper-layer views. hostd
persists facts and materializes views; orchd serializes admission and operates
live work without making private actor state durable authority.

The migration evolves existing commands, journal-backed execution projections,
process-local host Turn state, maps, and read models in place. Derived does not
mean physically deleted. Existing structures become caches, indexes,
compatibility state, or materialized views whose values converge from
lower-layer facts. No full schema or runtime rewrite is required.

## Design constraints

- The schema-v4 append-only journal remains the sole durable authority.
- `session.json` remains immutable identity; current query paths read
  write-time projections.
- hostd owns durable user-visible state; orchd owns live Agent execution.
- `piko-protocol` carries DTOs only and remains a shared leaf.
- One AgentInstance admits commands serially and has at most one active Run.
- A successful admission response follows its durable commit.
- Existing Execution records and IDs remain readable and usable for runtime
  correlation; no multi-attempt product model is introduced.
- Existing host `TurnRecord` state, session-store execution maps, and
  `active_agent_runs` runtime maps remain during and after the migration where
  useful; the change is their authority and update path, not mandatory
  removal.
- Realtime streams remain lossy and reconcilable from host read models.

## Current-state decomposition

| Existing concept | Keep as | Required change |
|---|---|---|
| AgentInstance actor and snapshot | Primitive/runtime owner | Add stable root-input identity to the host projection |
| `AgentInputQueued` runtime record | Partial AgentInput fact | Generalize to start, steer, and follow-up admission/disposition |
| host `TurnRecord` / active Turns | Existing process-local product projection | Update from input/execution facts instead of an independent lifecycle path |
| orchd Run identity | Stable derived correlation | Map it immutably to the root input; keep the existing ID and call sites |
| Execution actor / session-store `StoredExecution` | Existing operational and durable projection | Keep correlation/recovery facts; do not create a parallel product lifecycle |
| `ModelStepCommittedV1` | Core primitive fact | Retain atomic relation and connect applied steer inputs |
| host `steer_queue` | Process-local compatibility state | Replace with durable pending AgentInputs bound to a Run |
| runtime/TUI follow-up queues | Existing caches/projections | Reconcile from host-stored pending AgentInputs; remove only redundant ownership |
| Agent activity / foreground | Partial projection | Materialize once from primitive facts and pending action |

Slice 1 added `AgentInterrupt` and already proves that control can address an
AgentInstance even when no host Turn exists. Later slices move admission and
projection onto the same boundary.

## Canonical data model

### AgentInput

The protocol and host domain gain one immutable admission DTO:

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
    user_turn_id: Option<TurnId>,
    caller_agent_instance_id: Option<AgentInstanceId>,
}
```

`input_id` is the durable control identity. `request_id` provides caller
idempotency. They may initially contain the same generated value but are not
the same concept. `user_turn_id` is correlation metadata for a derived product
view, not a parent aggregate.

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

### Derived Run and existing Run storage

```rust
RunProjection {
    run_id: RunId,
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    root_input_id: AgentInputId,
    started_at: i64,
    terminal: Option<AgentRunTerminal>,
}
```

This is a projection shape, not a new canonical `AgentRunRecord`. D-68 does
not require adding an `agent_runs` aggregate or replacing the existing
`active_agent_runs` runtime map. Existing Run IDs and maps remain. For current
schema-v4 events, `ExecutionStartedV1.request_id` resolves the starting
AgentInput, so the immutable `run_id → root input` relation needs no new
execution-event field.

Run status is derived from input application, existing execution start/finish,
pending action, interrupt, ModelStep, and tool facts. If presentation needs a
normalized enum, the read-model projector computes it. No generic
`run_status_changed` event is appended solely for display.

A Run view has exactly one root AgentInput. A steer records the causal root it
was bound to at acceptance and the ModelStep that later consumed it. At most
one root may project as active for an AgentInstance.

### ModelStep

The implemented `ModelStepCommittedV1` remains the durable atomic boundary for
assistant output and ordered tool declarations. It already has independent
identity and recovery semantics. Before issuing a model request, orchd
reserves its `ModelStepId` and commits each steer input's application to that
step. The later ModelStep commit uses the same ID. This prevents a crash after
request dispatch from making an applied steer pending again.

No separate durable `model_step_started` fact is required for product state.
Live step progress comes from realtime runtime observation; after a crash, the
absence of a committed step is handled by the Run/Execution interruption
policy.

### UserTurnView

`turn_id` remains a stable product correlation for existing navigation,
usage, compatibility commands, and message grouping. It does not own status.

```text
if starting input is pending_follow_up  => queued
if starting input is cancelled         => cancelled
if linked Run is active                 => running/requires_action/cancelling
if linked Run is terminal               => same terminal class
```

User-origin immediate starts allocate a `turn_id` with their input proposal.
User-origin follow-ups allocate it at admission so the queued interaction is
visible before a Run exists. Agent/system inputs normally omit it. Existing
host Turn mutation paths remain adapters during migration, then reduce to
projection maintenance or disappear.

### Derived Execution

Existing `execution_started`, `execution_finished`, `execution_id`,
session-store `StoredExecution`, host `ExecutionProjection`, and actors remain
because they support orchestration, recovery, trajectory, and correlation.
The Execution view is already materialized from these facts and currently has
a one-to-one operational relation with Run.

D-68 does not add an `executions: run_id → Vec<attempt>` aggregate, attempt
ordinal, retry policy, or client state. Provider-request retry remains inside
a ModelStep. Process recovery follows the existing interrupted outcome. A
future full-Run retry/resume PRD may introduce multiple attempts if it defines
observable semantics that cannot be represented by the existing primitive
facts and projections.

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
execution_started / execution_finished       operational compatibility facts
pending_action facts                          existing feature authority
agent lifecycle facts                         existing feature authority
```

Existing execution start/finish facts continue to supply processing boundaries
for the derived Run and Execution views. `ExecutionStartedV1.request_id`
resolves the root AgentInput; do not version that event merely to add a second
root field, and do not add parallel Run lifecycle events. The exact split
between admission and initial disposition may be one atomic event payload or
adjacent facts in one append. The semantic commit must make the immutable
input and its initial disposition visible together.

Do not add:

- independent `turn_status_changed` facts;
- an independently mutable queue record;
- generic foreground transitions;
- a second Run-shaped Execution state machine;
- a new authoritative `AgentRunRecord`/`agent_runs` aggregate;
- `model_step_started` solely to persist transient streaming state.

### Fact relationships and validation

- every input references an existing Session and AgentInstance;
- one request ID maps to one normalized immutable input proposal;
- root application records the existing/derived Run correlation and makes the
  input its causal root;
- `pending_steer` captures the exact active `root_input_id` at admission;
- `applied_to_step` references the captured root and a reserved next
  ModelStepId; a later committed step with that ID must agree;
- cancelled inputs have no application fact;
- an execution start's `request_id` resolves an admitted root input for the
  same AgentInstance;
- only one root projects as non-terminal per AgentInstance;
- execution terminal facts for a root are idempotent and cannot imply
  conflicting derived Run outcomes;
- queue order is journal admission order, not client wall-clock time.

### Semantic commit groups

Immediate start:

```text
agent_input_admitted(initial = applied_as_root, request_id, run_id)
+ existing execution_started(request_id, run_id) fact
+ starting user message
```

Follow-up while busy:

```text
agent_input_admitted(initial = pending_follow_up, bound_after_run_id)
```

Queue advancement:

```text
agent_input_disposition_changed(applied_as_root, request_id, run_id)
+ existing execution_started(request_id, run_id) fact
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
an applied input and unfinished derived Run/Execution views. Existing
interruption recovery closes those views; it never redelivers that input.

Interrupt and terminal facts continue through the existing host-owned
execution/Turn compatibility ports while their projections are unified.
Reliable publication follows append success.

## Aggregate and projections

### Session aggregate and existing indexes

The canonical aggregate adds AgentInput facts and the minimum indexes needed
to join them to existing storage:

```text
agent_inputs: input_id → StoredAgentInput
model_steps: step_id → StoredModelStep             existing
executions: existing storage with request_id → root-input correlation
turns: existing process-local TurnRecord projected/reconciled from the facts

active_root_by_agent: agent_id → root_input_id     derived index
pending_inputs_by_agent: agent_id → ordered input_id[]  derived index
input_by_request: request_id → input_id             derived index
```

Do not add a parallel `agent_runs` map. Existing `StoredExecution` storage,
Run-keyed runtime maps, and process-local host `TurnRecord` state remain in
place. Reducers/projectors enrich them with root-input correlation and ensure
their derived status agrees with lower-layer facts. This reuses current
recovery and query paths instead of forcing a storage rewrite.

Indexes are rebuilt deterministically from event order. They accelerate
validation and projection; maps and indexes are not separate authorities.

### Current read model

`readmodels/current.json` gains one per-AgentInstance projection:

```rust
AgentWorkSnapshot {
    agent_instance_id: AgentInstanceId,
    lifecycle: AgentLifecycle,
    foreground: AgentForeground,
    active_run: Option<ActiveRunSnapshot>,
    pending_steers: Vec<AgentInputSummary>,
    queued_inputs: Vec<AgentInputSummary>,
    pending_action: Option<PendingActionSummary>,
}

ActiveRunSnapshot {
    run_id: RunId,
    root_input_id: AgentInputId,
    user_turn_id: Option<TurnId>,
    state: AgentRunViewState,
    active_model_step_id: Option<ModelStepId>,
    started_at: i64,
}
```

Run and Execution IDs remain stable projection/correlation IDs. Clients do not
need an Execution ID to choose a product control. `AgentInputSummary` exposes input ID,
origin, preview, admission order/time, delivery, optional Turn correlation,
and disposition.

The host derives foreground with one shared priority:

```text
requires_action > cancelling > running > queued > idle
```

`UserTurnView` rows are projected from user-origin inputs and their linked
Runs. Existing `active_turns` and queue events remain compatibility views until
all clients consume `AgentWorkSnapshot`.

During the compatibility slices, `AgentWorkSnapshot` is an internal shadow
projection for replay and equivalence tests. It must not be treated as a
complete client contract while `pending_action` and active ModelStep facts are
not yet projected; the client cutover belongs to Slice 5.

## Runtime admission boundary

`AgentRuntimeApi` accepts the canonical proposal and returns a receipt:

```rust
submit_agent_input(input) -> AgentInputReceipt {
    input_id,
    disposition,
    run_id: Option<RunId>,
    queued_position: Option<u32>,
}

interrupt_agent(session_id, agent_instance_id) -> AgentInterruptReceipt
cancel_agent_input(session_id, agent_instance_id, input_id)
    -> AgentInputCancelReceipt
```

The AgentActor is the serialization point:

1. capture lifecycle, active root input, and existing Run correlation;
2. evaluate delivery and capacity;
3. construct the complete durable transition proposal;
4. commit through a host-owned admission port;
5. mutate private actor state after acknowledgement;
6. return the receipt and publish a refreshed snapshot.

For steer, the actor captures `target_root_input_id` plus the existing Run
correlation. Delivery rejects if that root is no longer active; it never
substitutes the next root. Follow-up advancement commits input application
with the existing execution-start path before model work launches.

This ordering intentionally makes host persistence part of admission. If the
commit fails, the runtime has not accepted the input.

## Host application control plane

Create one application use case behind protocol dispatch:

```rust
AgentWorkControl {
    submit(input),
    interrupt_current(session_id, agent_instance_id),
    cancel_input(session_id, agent_instance_id, input_id),
    cancel_user_turn(session_id, turn_id),
}
```

`cancel_user_turn` is a resolver, not a separate cancellation engine:

- a pending view resolves to its AgentInput and calls `cancel_input`;
- an active view resolves to its AgentInstance/current Run and calls
  `interrupt_current`;
- a terminal view returns an idempotent no-op result.

Compatibility mappings:

| Existing surface | Canonical operation |
|---|---|
| `ChatSubmit[Message]` | user AgentInput with `FollowUp` delivery and Turn correlation |
| `QueueSteer[Message]` | user AgentInput with `Steer` delivery |
| queued `TurnCancel` | resolve UserTurnView → `cancel_input` |
| running `TurnCancel` | resolve UserTurnView → `interrupt_current` |
| `AgentInterrupt` | `interrupt_current` |
| `message_agent when=queue` | agent AgentInput with `FollowUp`, no Turn |
| `message_agent when=steer` | agent AgentInput with `Steer`, no Turn |
| `interrupt_agent` tool | runtime-local policy then same interrupt operation |

Protocol handlers remain adapters. After migration, a single
`AgentInputSubmit` command may replace duplicated submit/steer commands, but
wire cleanup is not required to establish one authority.

## Client migration

TUI, desktop, and client-core consume the host projection:

- composer routing checks active Run/foreground instead of active Turn;
- Enter steers any active Run;
- Alt+Enter submits a follow-up and reconciles by input ID;
- Esc interrupts the viewed AgentInstance;
- dequeue selects an authoritative input ID;
- queue counts, previews, and pending steers come from `AgentWorkSnapshot`;
- Turn rows render `UserTurnView` derived by hostd.

Clients may show a command as submitting before its receipt. Rejection or the
next authoritative snapshot removes that optimism. Restart, agent switching,
and multiple clients never depend on local queue history.

## Races and recovery

- **Steer versus Run terminal:** serialization chooses one. A committed steer
  remains bound to that Run and is consumed or explicitly cancelled; a later
  Run cannot inherit it.
- **Follow-up versus terminal:** the input either starts immediately or is
  admitted pending and then advanced. Both paths create one input and one Run.
- **Cancel versus queue advancement:** both address one input ID; exactly one
  transition wins.
- **Interrupt versus terminal:** terminal-first returns `accepted: false`;
  interrupt-first records intent for that Run and cannot affect its successor.
- **Host crash after acceptance:** journal replay restores pending input,
  active Run, Turn views, queue, and foreground.
- **orchd crash after admission:** attach reconstructs pending input and Run
  identity from the host projection; unfinished operational execution follows
  the existing interrupted policy.
- **Projection failure:** the journal remains authoritative and write-time read
  models rebuild through existing CQRS recovery.

## Refactor sequence

### Slice 1 — Agent interrupt (implemented)

Keep `AgentInterrupt` end to end. Esc targets the viewed AgentInstance, and
hostd preserves compatibility Turn terminal behavior for user-origin work.

### Slice 2 — Enrich existing commits with primitive facts

- Add AgentInput facts, root-input correlation, reducers, and invariants.
- Append input facts in the same commits that already write queue, Turn, and
  execution records; do not introduce an AgentRun aggregate.
- Build `AgentWorkSnapshot` and compare it against existing projections in
  tests and optional diagnostics.
- Interpret existing queue and execution facts as legacy projection inputs
  during replay; do not rewrite journal segments or synthesize a second full
  aggregate.

### Slice 3 — Canonical runtime admission

- Route start, steer, and follow-up through one AgentActor admission path.
- Commit acceptance before actor mutation and response.
- Bind pending steers to root input and persist their application.
- Advance follow-ups from host-recoverable pending inputs.

### Slice 4 — Host control plane and derived Turn views

- Introduce `AgentWorkControl` and move command semantics out of dispatch.
- Make Run/Execution/UserTurnView/foreground/queue read models converge from
  primitive and existing compatibility facts.
- Route existing Turn lifecycle updates through the common projector rather
  than deleting Turn records.
- Retain compatibility DTOs and verify equivalent results.

### Slice 5 — Client cutover and authority removal

- Move TUI and desktop to `AgentWorkSnapshot`.
- Remove local queue and process-local pending-steer authority.
- Remove only obsolete host Turn mutation paths after no caller depends on
  them; retaining materialized Turn records is valid.
- Keep legacy read support for existing schema-v4 journals.

### Slice 6 — Recovery and convergence closure

- Run crash-point, race, restart, and two-client matrices.
- Remove shadow comparisons and superseded compatibility projections where
  safe.
- Update F-51 and verification evidence to implemented.

Each slice must compile and preserve current behavior. A compatibility adapter
may be deleted only after the canonical path has durable replay and client
reconciliation coverage.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | AgentInput/receipt/disposition DTOs, causal-root fields, AgentWorkSnapshot, compatibility mappings |
| `piko-session-store` | AgentInput facts, root correlations, reducer invariants, in-place enrichment of existing execution/Turn projections |
| `piko-orchd-api` | Canonical admission/control and host commit ports |
| `piko-orchd` | AgentActor serialization, root-bound steer, existing Run/Execution cache enrichment, host-recoverable follow-up advancement |
| `piko-hostd` | AgentWorkControl, journal commits, UserTurnView/work projections, compatibility adapters |
| `piko-client-core` | Shared reduction and consumption of AgentWorkSnapshot |
| `piko-tui` | Projection-driven routing, queue, steer, interrupt, and Turn presentation |
| `piko-desktop` | Same projection and controls; no independent lifecycle model |

No `island-rs` lifecycle changes are required. Reusable presentation controls
may live there later, while piko retains domain IDs and intent mapping.

## Verification

- Event/reducer transition tables for AgentInput and causal-root relations,
  including invalid references, idempotency conflict, replay, and legacy
  interpretation.
- Property tests that UserTurnView, queue, active Run, and foreground are pure
  deterministic projections of one aggregate.
- Crash-point tests before and after each admission/application/start/terminal
  commit.
- AgentActor races for steer-terminal, follow-up-terminal, cancel-advance, and
  interrupt-successor isolation.
- Shadow-projection tests during dual-write migration.
- Host tests proving compatibility commands call one control service.
- Fresh-client and two-client reconciliation without local queue history.
- Cross-process E2E for detached steer/interrupt and queued cancellation after
  restart.
- `cargo fmt --all`, workspace tests, and workspace clippy with warnings
  denied.

## Alternatives considered

- **Add durable Turn and Execution state machines first:** rejected because it
  hardens concepts whose product state is derivable from input and Run.
- **Use Agent activity alone:** rejected because it lacks stable input, Run,
  queue, idempotency, and race identity.
- **Keep steer ephemeral:** rejected because acknowledged input can disappear
  before the next ModelStep.
- **Keep queues client-local:** rejected because restart and multiple clients
  cannot converge.
- **Replace existing Run/Execution/Turn storage:** rejected because derived
  scopes can remain materialized in current records and maps; changing their
  authority does not justify a full rewrite.
- **Big-bang wire/schema replacement:** rejected because behavior can remain
  available through compatibility adapters while authority moves safely.
