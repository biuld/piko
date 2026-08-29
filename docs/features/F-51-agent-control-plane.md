# F-51: Agent work lifecycle and control plane

> Status: proposed (Slice 1 agent interrupt implemented)
> Priority: P0
> Source evidence: piko product/runtime review; consolidates [F-01](F-01-turn-runtime.md), [F-10](F-10-multi-agent.md), [F-22](F-22-client-agent-projection.md), [F-31](F-31-durable-session-journal.md), and [F-48](F-48-authoritative-agent-lifecycle.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Decision: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md)

## Summary

piko presents and controls all work sent to an AgentInstance through one
host-authoritative, two-layer model. AgentInstance, AgentInput, ModelStep, and
their causal lifecycle facts form the primitive layer. Run, Execution, Turn,
Queue, and AgentForeground are derived scopes or views rather than competing
lifecycle authorities.

This feature is a model refactor. It replaces fragmented Turn, queue, steer,
and runtime activity ownership while preserving compatible product commands
during migration.

## Problem

The current system cannot answer one simple question from one authority: what
is this agent doing, and what will it do next?

- hostd owns an active-Turn state machine, while detached child Runs have no
  Turn;
- orchd exposes Agent activity and execution state, but accepted steers and
  follow-ups do not share one durable lifecycle;
- the runtime queue is durable in part, while clients also keep local queue
  state;
- Run and Execution usually identify the same lifetime, yet the documented
  hierarchy encourages two authoritative state machines;
- control behavior changes depending on whether work happened to originate
  from a user Turn.

Storage, display, recovery, and interaction therefore derive different answers
from overlapping state.

## Product mental model

```text
Primitive facts
Session
└── AgentInstance
    ├── AgentInput 0..N
    └── ModelStep 0..N
        └── Message / Thought / ToolCall / ToolResult

Derived scopes
Run                = root input + causal inputs/steps/actions + outcome
Execution          = runtime processing interval for that work
UserTurnView       = user input + correlated conversation/work facts
Queue              = ordered inputs whose work has not started
AgentForeground    = projection of active work/queue/pending action
```

### Primitive meanings

| Primitive | Product meaning | Independent reason to exist |
|---|---|---|
| AgentInstance | Long-lived addressable collaborator | Receives controls and serializes work across many Runs |
| AgentInput | Immutable, idempotent request to start, steer, or follow up | Can be accepted, pending, applied, or cancelled before any derived scope exists |
| ModelStep | One model request/response boundary | Atomically relates assistant output and tool declarations for recovery |
| Causal lifecycle facts | Immutable relations and outcomes | Preserve root, application, interruption, and terminal truth without another mutable aggregate |

### Derived scopes and views

- A Run groups facts sharing one root AgentInput. It is the logical-work view
  used for history, control resolution, and diagnostics, but not an
  independently authoritative state machine.
- Execution groups runtime start/finish and recovery facts for that work. It
  may retain a stable diagnostic ID without owning product state.
- A UserTurnView has stable product identity when useful. Before work starts it
  reflects the starting AgentInput; afterward it reflects correlated work.
- The queue is not independently edited. Queueing and dequeueing change the
  state of a stable AgentInput.
- AgentForeground is computed by the host and supplied to all clients.

Derived does not mean transient or absent from storage. Existing Run,
Execution, and Turn records may remain as materialized projections,
compatibility indexes, and runtime caches. Their state must converge from the
primitive facts rather than become a second authority.

## Input admission

Every input has a stable input ID, idempotency request ID, target
AgentInstance, origin, content, submission order, and delivery intent.

| Delivery intent | Agent idle | Agent running |
|---|---|---|
| Start | Start one AgentRun | Reject |
| Steer | Reject | Bind to and join the active AgentRun |
| Follow up | Start one AgentRun | Remain pending for a future AgentRun |
| Auto | Start one AgentRun | Bind to and join the active AgentRun |

Admission returns an authoritative disposition and stable IDs. Retrying the
same request with identical content is idempotent; conflicting reuse is
rejected. Acceptance is durable before success is reported.

## Lifecycle behavior

### AgentInput

An accepted AgentInput is in exactly one effective disposition:

- pending follow-up;
- pending steer bound to one active AgentRun;
- applied to start one AgentRun;
- applied to one ModelStep as steer;
- cancelled before application.

The transition from pending follow-up to Run start is ordered and exactly
once. A pending steer cannot retarget a later Run after a terminal race.

### Derived Run

A Run is derived from one root AgentInput and every input, ModelStep, tool,
pending-action, interruption, and outcome fact causally attached to that root.
At most one causal root is active per AgentInstance. A later follow-up becomes
a new root rather than extending terminal work.

### ModelStep

ModelSteps remain ordered within a Run. Each committed step atomically relates
the assistant message and tool declarations needed for reliable observation
and recovery. Tool results may commit later while retaining Run and step
correlation.

## User journeys

1. A user submits to an idle agent. The host admits one AgentInput; it starts
   one AgentRun. The UI shows a UserTurnView derived from those facts.
2. A user steers a running detached child. The input is durably bound to the
   current AgentRun and later records the ModelStep that consumed it. No Turn
   or new Run is created.
3. A user submits a follow-up to a busy agent. The input appears immediately in
   the host projection. Its UserTurnView is queued because the input is
   pending, not because another Turn state machine was advanced.
4. A user cancels a queued follow-up after restart or from another client. The
   stable AgentInput becomes cancelled and every projection converges.
5. A user presses Esc while viewing active work. The current AgentRun is
   interrupted whether or not it has a UserTurnView.
6. A crash occurs after input acceptance. Replay determines whether the input
   is pending, applied, or cancelled without transcript adjacency or client
   memory.

## Steer and queue behavior

- Steer acceptance is linearized against the active causal root represented by
  the current AgentRun view.
- Pending steers retain admission order and apply at deterministic ModelStep
  boundaries.
- Follow-ups are ordered pending AgentInputs owned by the AgentInstance.
- Queue cancellation addresses input identity, never a display position.
- A Run terminal does not cancel later follow-ups.
- Capacity and lifecycle rejection are explicit admission outcomes.
- Clients retain no authoritative shadow queue or pending-steer counter.

## Control contract

| Intent | Canonical target | Effect |
|---|---|---|
| Submit work | AgentInstance + AgentInput | Start, steer, or follow up according to explicit delivery |
| Interrupt current work | AgentInstance | Cancel the active AgentRun while keeping the agent reusable |
| Cancel pending work | AgentInput | Cancel exactly one unapplied input |
| Cancel displayed Turn | UserTurnView | Resolve to its pending input or active Run, then use the same controls |
| Close/reopen agent | AgentInstance | Change future admission, not historical work |

Acknowledgement and terminal outcome are separate. An idle interrupt race is
a benign unaccepted result and can never affect a later Run.

## Storage and recovery contract

The journal stores enough primitive facts to reconstruct:

- immutable AgentInput admission and disposition changes;
- the causal root established by a starting input and its terminal outcome;
- which causal root accepted a steer and which ModelStep applied it;
- required ModelStep/message/tool relations;
- existing Agent lifecycle and pending-action facts.

Run, Execution, UserTurnView, queue membership/order, and AgentForeground are
materialized from those facts. Existing records remain usable for runtime
recovery and trajectory correlation, but this feature adds no independent
derived-scope lifecycle or multi-attempt requirement.

Realtime deltas may be lost. Query/read-model paths recover the same current
work and queue state entirely from the journal-backed read models.

## Display and interaction contract

Clients receive one host-authored per-agent projection containing Agent
lifecycle, foreground, active Run, pending steers, pending follow-ups, pending
action, and stable control IDs. User Turn views are supplied by the host from
the same facts.

For the viewed AgentInstance:

- idle Enter starts work;
- running Enter steers the active Run, detached or user-origin;
- Alt+Enter follows up, starting immediately when idle or remaining pending
  when busy;
- Esc interrupts active work;
- dequeue cancels a selected AgentInput.

Clients may own editor drafts, selection, animation, and command optimism, but
not lifecycle or queue truth.

## In scope

- AgentInstance, AgentInput, ModelStep, and causal lifecycle facts as the
  primitive layer.
- Run, Execution, UserTurnView, Queue, and AgentForeground as reproducible
  derived scopes.
- Durable input admission for start, steer, and follow-up.
- One derived logical Run view and causal-root-bound steering.
- Derived UserTurnView, queue, and AgentForeground projections.
- Agent-addressed admission/interruption and input-addressed cancellation.
- Compatibility mapping for existing client and multi-agent commands.
- Recovery, idempotency, races, and multi-client convergence.

## Out of scope

- Full-Run retry/resume and multiple ExecutionAttempts per Run.
- Exposing ExecutionAttempt as a product control handle.
- Changing model-provider streaming protocols.
- Persisting token deltas, animations, hover state, or drafts.
- A global scheduler, queue priority, or queue reordering.
- Replacing trajectory diagnostics.

## Acceptance criteria

### Slice 1: agent interrupt

- [x] A viewed detached child can be interrupted by AgentInstance identity.
- [x] A user-origin Run interrupt preserves host-visible terminal behavior.
- [x] Idle interrupt races return `accepted: false`.

Verification: [V-64](../verification/V-64-agent-control-plane.md)

### Primitive lifecycle and storage

- [ ] Normative docs and protocol separate AgentInstance/AgentInput/ModelStep
      primitive facts from derived Run/Execution/Turn scopes.
- [ ] Accepted start, steer, and follow-up inputs have durable identities and
      replayable dispositions.
- [ ] Every derived Run resolves to one root input; every accepted steer is
      durably bound to that causal root before acknowledgement.
- [ ] A crash after input acceptance cannot lose or duplicate pending input.
- [ ] No independently authoritative Run, Turn, queue, foreground, or
      Execution state is required to reconstruct product lifecycle.

### Projection and interaction

- [ ] Session reconciliation restores active work, pending steers, follow-ups,
      and UserTurnViews from host read models.
- [ ] TUI and desktop consume the same foreground and controls without local
      queue authority.
- [ ] Detached and user-origin Runs have identical steer and interrupt
      behavior.
- [ ] Queued input cancellation works after restart and from a second client.
- [ ] Concurrent controls for one AgentInstance are linearized and cannot
      affect a later Run accidentally.
