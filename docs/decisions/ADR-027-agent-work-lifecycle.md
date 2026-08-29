# ADR-027: Agent work lifecycle is centered on AgentInput and Run

> Status: proposed
> Date: 2026-08-29
> Supersedes in part: [ADR-025](ADR-025-authoritative-agent-lifecycle.md) strict `Turn → Run` hierarchy

## Context

piko has durable AgentInstance identities, host Turns, Agent Runs, concrete
Executions, ModelSteps, a durable follow-up queue, steer delivery, and
agent-addressed cancellation. These pieces do not currently form one model.

ADR-025 expresses the runtime spine as `Turn → Run → Execution → ModelStep`.
That is correct for user-originated work but false for detached child and
system Runs, whose `source_turn_id` is intentionally absent. Treating Turn as
the universal root has led product controls and projections to gate Agent work
on host Turn presence.

Input also lacks one durable lifecycle. Follow-ups are journaled, starting
inputs are represented indirectly by execution/message facts, and accepted
steers are tracked transiently until they become transcript messages. Host and
clients therefore combine Turn state, Agent activity, local queue records, and
realtime events to infer one foreground state.

## Decision

Adopt this canonical relationship model:

```text
Session → AgentInstance → Run → Execution → ModelStep → Thought / ToolCall
             │             ▲
             ├─ input FIFO │
             └─ AgentInput ┘

Turn 0..1 ── source relation ── Run
```

### Identity and cardinality

- **AgentInstance** is the long-lived addressable runtime actor. It owns at
  most one active Run and one ordered pending-input queue.
- **AgentInput** is the immutable, idempotent admission unit for start, steer,
  and follow-up delivery. It has a durable lifecycle from acceptance to
  applied or cancelled.
- **Turn** is an optional host-owned user-interaction envelope related to one
  Run. It is not created for detached agent/system work. Steer does not create
  a Turn.
- **Run** is logical Agent work created by one starting AgentInput and may
  consume later steer inputs. A Run belongs to exactly one AgentInstance and
  references zero or one Turn.
- **Execution** is one concrete attempt of a Run. A Run permits one or more
  Execution attempts so recovery/retry does not collapse logical work into a
  process attempt.
- **ModelStep** belongs to exactly one Execution. Thought and ToolCall remain
  content/action boundaries within the step.

Stable product controls address AgentInstance, AgentInput, or Turn. Execution
identity remains available for storage, correlation, recovery, and diagnostics
but is not required for ordinary product control.

### Authority

- orchd owns AgentInstance admission serialization, Run state transitions,
  Execution operation, and ModelStep operation.
- hostd owns the append-only journal and every durable user-visible
  projection, including Turn state, input queues, pending steers, and derived
  Agent foreground.
- orchd must durably commit an accepted transition through host-owned ports
  before reporting acceptance or publishing reliable visibility.
- clients consume host projections. They may own ephemeral drafts and
  optimistic correlation, but not queue membership or lifecycle truth.

### Admission and control

Start, steer, and follow-up use one AgentInput admission contract with explicit
delivery intent. Steer targets the active Run and is durably bound to that Run
at acceptance so it cannot race into a later Run. Follow-up queues on the
AgentInstance and starts exactly once. Interrupt targets current Agent work by
AgentInstance; queued cancellation targets AgentInput; Turn cancellation is a
host product operation resolved through the same control service.

### Projection

The host materializes one per-AgentInstance work projection from journal facts:
lifecycle, derived foreground, active Run/optional Turn, current Execution and
ModelStep presentation state, pending steers, queued follow-ups, and pending
user action. TUI, desktop, and future clients use this same projection.

## Consequences

- Detached and Turn-backed Runs share admission, steer, queue, interrupt, and
  display semantics.
- The journal needs explicit AgentInput and durable Turn lifecycle facts beyond
  the currently partial queue/execution representation.
- Run and Execution storage must permit multiple Execution attempts for one
  Run instead of keying the execution projection by Run ID.
- Existing `ChatSubmit`, `QueueSteer`, `TurnCancel`, and multi-agent tools can
  remain compatibility surfaces, but must delegate to one application control
  service.
- Client-local follow-up queues and process-local steer counters cease to be
  authoritative and are removed after the host projection lands.
- ADR-025 remains authoritative for distinct Run/Execution/ModelStep identity,
  atomic ModelStep commits, and realtime convergence. Only its universal
  `Turn → Run` parent relation is superseded by the optional Turn relation in
  this ADR.
- Schema v4 remains the storage generation; new event versions follow the
  journal's compatibility/upcasting rules and require no older-layout
  migration.
