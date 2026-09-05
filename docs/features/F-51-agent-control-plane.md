# F-51: Agent work lifecycle and control plane

> Status: implemented (D-68; [V-64](../verification/V-64-agent-control-plane.md)).
> Host follow-up admission keeps distinct `input_id` / `request_id`.
> `AgentWorkSnapshot` is the published recoverable foreground contract.
> Internal orchd Execution DTOs remain off the client command surface.
> Priority: P0
> Source evidence: piko product/runtime review; consolidates [F-01](F-01-turn-runtime.md), [F-10](F-10-multi-agent.md), [F-22](F-22-client-agent-projection.md), [F-31](F-31-durable-session-journal.md), and [F-48](F-48-authoritative-agent-lifecycle.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Decision: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md)

## Summary

piko presents and controls all work sent to an AgentInstance through one
host-authoritative model. The invariant grains are Session, AgentInstance, and
ModelStep. AgentInput is the stimulus between Agent and ModelStep: an
idempotent request that can start work, steer the current work, wait as a
follow-up, or be cancelled.

The only mid-granularity derived view is the causal closure of a root
AgentInput (from `applied_as_root` until that root is terminal). It has no
second identity. Turn, Run, and Execution are not product scopes and are
removed by the remaining slices.

The client in scope is the TUI. Desktop is out of scope. Old commands and
compatibility paths are deleted, not wrapped.

## Problem

The current system cannot answer one simple question from one authority: what
is this agent doing, and what will it do next?

- hostd owns an active-Turn state machine, while detached child work has no
  Turn;
- orchd exposes activity and execution state, but accepted steers and
  follow-ups do not share one durable lifecycle;
- the runtime queue is durable in part, while clients also keep local queue
  state;
- Turn, Run, and Execution occupy the same lifetime with three identities;
- control behavior changes depending on whether work happened to originate
  from a user Turn.

Storage, display, recovery, and interaction therefore derive different answers
from overlapping state.

## Product mental model

```text
Session                         invariant
└── AgentInstance               invariant
    ├── AgentInput 0..N         stimulus (start / steer / follow-up)
    └── ModelStep 0..N          invariant
        └── Message / Thought / ToolCall / ToolResult

Derived queries (not identities)
active work     = unfinished applied_as_root AgentInput + facts sharing that root
queue           = pending_follow_up inputs in admission order
pending steers  = pending_steer inputs bound to the active root
foreground      = requires_action > cancelling > running > queued > idle
```

There is no abstraction between Session and Agent. Between Agent and
ModelStep the mid grain is the root AgentInput's causal closure. Its identity
is that input's `input_id`. Do not add `turn_id`, `run_id`, or `execution_id`.

### Primitive meanings

| Primitive | Product meaning | Independent reason to exist |
|---|---|---|
| Session | Durable conversation/journal boundary | Owns the append-only fact log |
| AgentInstance | Long-lived addressable collaborator | Receives controls and serializes work across many root inputs |
| AgentInput | Immutable, idempotent request to start, steer, or follow up | Can be accepted, pending, applied, or cancelled before any ModelStep exists |
| ModelStep | One model request/response boundary | Atomically relates assistant output and tool declarations for recovery |

### Derived views

- **Active work** is every steer input, ModelStep, tool, pending action,
  interruption, and outcome that shares one unfinished `root_input_id`. At
  most one root is active per AgentInstance. A later follow-up becomes a new
  root rather than extending terminal work.
- **Queue** is not independently edited. Queueing and dequeueing change the
  disposition of a stable AgentInput.
- **Foreground** is computed by the host and supplied to the TUI.
- Timeline grouping of user messages is a TUI presentation of user-origin
  AgentInputs and their root, not a host Turn aggregate.

Pending follow-ups hang on the Agent. They are not active work until applied
as a root.

## Input admission

Every input has a stable input ID, idempotency request ID, target
AgentInstance, origin, content, submission order, and delivery intent.

| Delivery intent | Agent idle | Agent running |
|---|---|---|
| Start | Apply as root (start work) | Reject |
| Steer | Reject | Bind to the active root; apply at a later ModelStep |
| Follow up | Apply as root (start work) | Remain pending for a future root |
| Auto | Apply as root | Bind to the active root |

Admission returns an authoritative disposition and stable input ID. Retrying
the same request with identical content is idempotent; conflicting reuse is
rejected. Acceptance is durable before success is reported.

## Lifecycle behavior

### AgentInput

An accepted AgentInput is in exactly one effective disposition:

- pending follow-up;
- pending steer bound to one active root input;
- applied as root (this input is the work identity);
- applied to one ModelStep as steer;
- cancelled before application.

The transition from pending follow-up to `applied_as_root` is ordered and
exactly once. A pending steer cannot retarget a later root after a terminal
race.

### Active work (derived)

Active work starts when an input is applied as root and ends when that root
is terminal. Steer, ModelStep, tool, pending-action, interrupt, and outcome
facts carry `root_input_id`. Processing start/finish and interruption are
facts on that root, not an Execution aggregate.

### ModelStep

ModelSteps remain ordered under one root input. Each committed step atomically
relates the assistant message and tool declarations needed for reliable
observation and recovery. Tool results may commit later while retaining the
same root and step.

## User journeys

1. A user submits to an idle agent. The host admits one AgentInput as
   `applied_as_root`. The TUI shows that input and the work that follows it.
2. A user steers a running detached child. The input is durably bound to the
   current root and later records the ModelStep that consumed it. No new root
   is created.
3. A user submits a follow-up to a busy agent. The input appears immediately
   in the host projection as `pending_follow_up`.
4. A user cancels a queued follow-up after restart or from another TUI. The
   stable AgentInput becomes cancelled and every projection converges.
5. A user presses Esc while viewing active work. The current root is
   interrupted; the AgentInstance stays reusable.
6. A crash occurs after input acceptance. Replay determines whether the input
   is pending, applied, or cancelled without transcript adjacency or client
   memory.

## Steer and queue behavior

- Steer acceptance is linearized against the active `root_input_id`.
- Pending steers retain admission order and apply at deterministic ModelStep
  boundaries.
- Follow-ups are ordered pending AgentInputs owned by the AgentInstance.
- Queue cancellation addresses input identity, never a display position.
- A root terminal does not cancel later follow-ups.
- Capacity and lifecycle rejection are explicit admission outcomes.
- The TUI retains no authoritative shadow queue or pending-steer counter.

## Control contract

| Intent | Canonical target | Effect |
|---|---|---|
| Submit work | AgentInstance + AgentInput | Start, steer, or follow up according to explicit delivery |
| Interrupt current work | AgentInstance | Terminal-interrupt the active root; keep the agent reusable |
| Cancel pending work | AgentInput | Cancel exactly one unapplied input |
| Close/reopen agent | AgentInstance | Change future admission, not historical work |

Acknowledgement and terminal outcome are separate. An idle interrupt race is
a benign unaccepted result and can never affect a later root.

Displayed timeline rows are not live commands. The TUI resolves a user-origin
input to `cancel_input` or `interrupt_current`.

## Storage and recovery contract

The journal stores enough primitive facts to reconstruct:

- immutable AgentInput admission and disposition changes;
- which input is the active or historical root and its terminal outcome;
- which root accepted a steer and which ModelStep applied it;
- required ModelStep/message/tool relations;
- Agent lifecycle and pending-action facts.

Queue membership/order, foreground, and active-work state are materialized
from those facts. Processing start/finish and interruption attach to the root
AgentInput. This feature does not keep Turn, Run, or Execution as identities
or write aggregates.

Realtime deltas may be lost. Query/read-model paths recover the same current
work and queue state entirely from the journal-backed read models.

## Display and interaction contract

The TUI receives one host-authored per-agent projection containing Agent
lifecycle, foreground, active root work, pending steers, pending follow-ups,
pending action, and stable input IDs.

For the viewed AgentInstance:

- idle Enter starts work;
- running Enter steers the active root, detached or user-origin;
- Alt+Enter follows up, starting immediately when idle or remaining pending
  when busy;
- Esc interrupts active work;
- dequeue cancels the last-admitted pending follow-up of the viewed agent by
  `input_id` (`queued_inputs.last()` after host admission-order sort). There
  is no selected-row queue UI.

The TUI may own editor drafts, selection, animation, and command optimism, but
not lifecycle or queue truth.

## In scope

- Session, AgentInstance, and ModelStep as the invariant grains.
- AgentInput as the stimulus and, when applied as root, the work identity.
- Active work as the derived causal closure of that root.
- Durable input admission for start, steer, and follow-up.
- Queue and foreground as snapshot fields.
- Agent-addressed admission/interruption and input-addressed cancellation.
- TUI consumption of the host work projection.
- Removal of leftover Turn, Run, and Execution types, IDs, commands, maps,
  and projections, plus the old submit/steer/cancel paths listed under Out of
  scope.
- Recovery, idempotency, races, and multi-client (TUI) convergence.

## Out of scope

- `piko-desktop` and any desktop-specific projection or control mapping.
- Dual-write, shadow projections, and retained compatibility adapters.
- Keeping Turn, Run, or Execution as product scopes, protocol IDs, or live
  maps (`turn_id`, `run_id`, `execution_id`, `TurnRecord`, `UserTurnView`,
  `StoredExecution` as a product aggregate, `active_agent_runs`).
- Keeping `ChatSubmit` / `ChatSubmitMessage`, `QueueSteer` /
  `QueueSteerMessage`, `TurnCancel`, host `steer_queue`, TUI local follow-up
  stacks, or orchd `send_agent_input` / `steer_agent` / request-id cancel
  shims as live surfaces.
- Full-work retry/resume and multiple processing attempts per root.
- Changing model-provider streaming protocols.
- Persisting token deltas, animations, hover state, or drafts.
- A global scheduler, queue priority, or queue reordering.
- Replacing Session History; remaining slices only rekey leftover trajectory
  capture off `root_input_id`. The F-36 web viewer is retired by ADR-029.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Which clients does this feature update? | TUI only | Desktop is a separate product surface and is not a constraint on the cutover. |
| Keep old commands as adapters while authority moves? | No. Delete them. | Dual-write keeps two answers for the same agent. |
| Session ↔ Agent mid-layer? | None | Session already owns the journal; another scope would compete with it. |
| Agent ↔ ModelStep mid-layer? | Root AgentInput causal closure | Steer, interrupt, and follow-up need a generation; ModelStep is too fine and may not exist yet. Identity is `input_id`, not a new type. |
| Keep Turn, Run, and Execution as derived identities? | No | They name the same lifetime three times. Remaining slices delete the leftovers. |

## Acceptance criteria

### Slice 1: agent interrupt

- [x] A viewed detached child can be interrupted by AgentInstance identity.
- [x] A user-origin work interrupt preserves host-visible terminal behavior.
- [x] Idle interrupt races return `accepted: false`.

Verification: [V-64](../verification/V-64-agent-control-plane.md)

### Primitive lifecycle and storage

- [x] Normative docs and protocol use Session, AgentInstance, AgentInput, and
      ModelStep. Turn, Run, and Execution are not product identities.
- [x] Accepted start, steer, and follow-up inputs have durable identities and
      replayable dispositions.
- [x] Active work is the unfinished root AgentInput; every accepted steer is
      durably bound to that `root_input_id` before acknowledgement.
- [x] A crash after input acceptance cannot lose or duplicate pending input
      (V-64 recovery matrix).
- [x] Product lifecycle reconstructs from AgentInput, ModelStep, and causal
      facts without Turn, Run, or Execution aggregates.

### Projection and interaction

- [x] Session reconciliation restores active work, pending steers, and
      follow-ups from host read models keyed by AgentInput.
- [x] The TUI consumes host foreground and controls without local queue
      authority. `ChatSubmit`, `QueueSteer`, `TurnCancel`, `TurnEvent`, and
      the local follow-up stack are gone.
- [x] Detached and user-origin work have identical steer and interrupt
      behavior.
- [x] Queued input cancellation works after restart and from a second TUI
      client (V-64 multi-client reconciliation evidence).
- [x] Concurrent controls for one AgentInstance are linearized and cannot
      affect a later root accidentally (V-64 race evidence).
