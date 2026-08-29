# F-51: Agent work lifecycle and control plane

> Status: proposed (Slice 1 agent interrupt implemented)
> Priority: P0
> Source evidence: piko product/runtime review; consolidates [F-01](F-01-turn-runtime.md), [F-10](F-10-multi-agent.md), [F-22](F-22-client-agent-projection.md), [F-31](F-31-durable-session-journal.md), and [F-48](F-48-authoritative-agent-lifecycle.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Decision: [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md)

## Summary

piko has one mental model for work sent to an AgentInstance. Every start,
steer, follow-up, queue entry, cancellation, Run, Execution, and ModelStep is
part of that model regardless of whether the initiator is a user, another
agent, or the system. A Turn is the optional user-interaction envelope for a
Run, not the parent of all agent work. The append-only host journal is the sole
durable authority; clients render host-authored projections and never combine
independent lifecycle guesses.

## Problem

The existing implementation has the right concepts but not one operational
model:

- the documented `Turn → Run → Execution` hierarchy conflicts with detached
  child Runs that intentionally have no Turn;
- hostd projects active Turns, orchd projects Agent activity, and clients merge
  them to guess whether an AgentInstance is working;
- follow-ups are durably queued through the Agent runtime, but the host queue
  event does not project them and the TUI keeps a process-local shadow queue;
- steer is accepted through a separate path, temporarily tracked in host
  memory, and restricted to host-owned Turns even when a detached Agent Run is
  active;
- cancellation can name a Turn, AgentInstance, queued input, Run, or Execution,
  but those identities do not yet enter through one control boundary;
- accepted input between admission and application is not represented by one
  durable, queryable lifecycle.

Without a shared model, storage cannot answer what is active or queued,
clients cannot display one authoritative state, and product interactions
accidentally depend on how the Run was started.

## Canonical mental model

```text
Session
└── AgentInstance                              long-lived addressable actor
    ├── active Run?                            current logical work
    │   ├── starting AgentInput                creates the Run
    │   ├── applied/pending steer AgentInputs  join the same Run
    │   └── Execution 1..N                     concrete attempts
    │       └── ModelStep 1..N
    │           └── Thought / ToolCall
    └── queued AgentInput 0..N                  future Runs, durable FIFO

Turn 0..1 ── source relation ── Run
```

### Entity meanings

| Entity | Meaning | Identity and lifetime |
|---|---|---|
| Session | Durable collaboration and history boundary | Owns the journal and AgentInstance tree |
| AgentInstance | Long-lived addressable actor with lifecycle, inbox, one active Run, and one input queue | Survives many Runs |
| AgentInput | Immutable, idempotent request to start, steer, or queue work | Exists from durable admission to applied/cancelled terminal state |
| Turn | User-visible interaction envelope | Optional one-to-one source relation to a Run; absent for detached agent/system work |
| Run | Logical processing of one starting AgentInput plus zero or more steers | Always belongs to one AgentInstance; may reference one Turn |
| Execution | Concrete attempt to realize a Run | One Run has one or more attempts over retry/recovery |
| ModelStep | One model request/response boundary inside one Execution | Ordered within its Execution |
| ToolCall | Model-declared action from one ModelStep | Result commits separately but remains attributed to the same step/run |

### Independent state axes

- **Agent lifecycle**: open, closed, terminated, unavailable.
- **Agent foreground**: idle, queued, running, requires action, cancelling.
- **Input state**: queued, pending steer, applied, cancelled.
- **Turn state**: queued, running, waiting for action, cancelling, terminal.
- **Run state**: admitted, running, waiting for action, cancelling, terminal.
- **Execution state**: accepted, running, succeeded, failed, cancelled,
  interrupted.
- **ModelStep state**: running, completed, failed, cancelled.

These axes are correlated by stable IDs; none is inferred by renaming another
axis. Agent foreground is a host-authored projection of the underlying facts,
not another state machine.

## User journeys

1. A user sends to an idle viewed agent. One durable AgentInput is admitted, a
   Turn and Run start, an Execution performs ordered ModelSteps, and all
   clients observe one correlated lifecycle.
2. A user steers any running viewed agent, including a detached child. The
   steer is durably accepted against the current Run, appears as pending, and
   later records the ModelStep boundary at which it was applied. It creates no
   Turn or Run.
3. A user queues a follow-up for a busy agent. A new queued Turn and one durable
   AgentInput appear in the host projection. When the current Run terminals,
   the input leaves the queue and starts its Run exactly once.
4. A user cancels a queued follow-up from another client or after restart. The
   host cancels it by stable input identity, terminals the related queued Turn,
   and every client converges without local queue bookkeeping.
5. A user presses Esc while viewing a running AgentInstance. The current Run
   is interrupted whether or not it has a Turn; any related Turn preserves its
   own cancelling/terminal lifecycle.
6. A crash occurs after a steer or follow-up was accepted. Recovery determines
   from the journal whether the input remains pending, was applied, or was
   cancelled and never guesses from transcript adjacency.

## Input admission

Every input carries a stable request ID, target AgentInstance, content,
origin, delivery intent, and optional source Turn. Delivery behavior is:

| Delivery | Agent idle | Agent running |
|---|---|---|
| Start when idle | Start a Run | Reject |
| Steer active | Reject | Attach durably to the active Run |
| Follow up | Start a Run | Queue durably for a future Run |
| Auto | Start a Run | Steer the active Run |

Admission returns an authoritative disposition and stable identity. The same
request retried with identical content is a duplicate; conflicting reuse is
rejected. Agent lifecycle and queue limits are evaluated before acceptance.

User product commands choose explicit delivery. `Auto` remains useful for
agent/system callers but must not hide whether a user requested steer or
follow-up.

## Steer behavior

- Steer targets the AgentInstance's active Run, not an active Turn.
- Acceptance is linearized against the active Run identity. A terminal race
  rejects without retargeting a later Run.
- Accepted steer becomes durable before success is returned.
- Pending steers retain submission order and are applied at deterministic
  ModelStep boundaries.
- Application records the owning Run and consuming ModelStep. Once applied, a
  steer cannot be dequeued or cancelled as pending input.
- A detached Run and a Turn-backed Run have identical steer semantics.

## Follow-up queue behavior

- The queue belongs to one AgentInstance and is durable FIFO.
- A queue row has a stable input ID, origin, content preview, submission time,
  delivery intent, and optional Turn/recipient relation.
- Transition from queued input to Run start is atomic and exactly once.
- Cancelling a row is idempotent and addresses the input, not its display
  position.
- A user-origin follow-up has a queued Turn immediately, so its visible
  lifecycle survives restart before the Run starts.
- Queue capacity and overload are explicit admission outcomes.
- Host projections expose the complete queue needed by clients; clients keep
  no authoritative shadow stack.

## Cancellation and control

The control plane distinguishes intent:

| Intent | Stable target | Effect |
|---|---|---|
| Interrupt current work | AgentInstance | Cancel the active Run/Execution; keep the agent reusable |
| Cancel queued work | AgentInput | Remove exactly one pending follow-up; terminal related Turn if present |
| Cancel exact Turn | Turn | Compatibility/product operation resolved through the same service |
| Close/reopen agent | AgentInstance | Change future-input admission, not history |

Cancellation acknowledgement and terminal outcome are separate. Idle races
are benign unaccepted results. A Run terminal never implicitly cancels later
queued inputs.

## Storage and recovery contract

The journal records enough facts to reconstruct without private runtime state:

- AgentInput admission and immutable payload identity;
- queued, applied, and cancelled input transitions;
- durable Turn start/status/terminal facts;
- Run admission/start/terminal and its optional Turn relation;
- every Execution attempt and terminal result;
- required ModelStep/message/tool relations;
- pending action and inbox facts owned by their existing features.

Realtime deltas may be lost. Query/read-model paths recover Agent work,
queues, Turns, Runs, Executions, and ModelSteps entirely from durable facts.

## Display and interaction contract

Clients receive one host-authored per-agent work projection containing:

- Agent lifecycle and derived foreground;
- active Run and optional Turn relation;
- current Execution/ModelStep status needed for presentation;
- pending steers and queued follow-ups;
- pending approval/interaction ownership;
- stable IDs for interrupt and queued-input cancellation.

TUI and desktop use the same projection. They may keep ephemeral editor drafts,
selection, animation, and optimistic command correlation, but never own queue
membership or lifecycle truth.

For the viewed AgentInstance:

- idle Enter starts a user Turn/Run;
- running Enter steers the active Run, detached or Turn-backed;
- Alt+Enter submits a follow-up, starting immediately when idle or queueing
  when busy;
- Esc interrupts current work;
- dequeue cancels a stable queued input from the authoritative projection.

## In scope

- Canonical entity/cardinality/state definitions.
- Durable AgentInput lifecycle for start, steer, and follow-up.
- Durable Turn lifecycle and Run/Execution correlation.
- Agent-addressed admission, steer, interrupt, and queued-input cancellation.
- One host-authored work/queue projection shared by clients.
- Compatibility mapping for existing client and multi-agent tool commands.
- Recovery, idempotency, race, and multi-client behavior.

## Out of scope

- Changing model-provider streaming protocols.
- Persisting token deltas, animations, hover state, or editor drafts.
- A global scheduler across Sessions.
- Queue reordering or priority policy in this slice.
- Exposing Execution IDs as required product control handles.
- Replacing trajectory diagnostics; trajectory consumes the same identities but
  remains observational.

## Acceptance criteria

### Slice 1: agent interrupt

- [x] A viewed detached child can be interrupted by AgentInstance identity.
- [x] A Turn-backed interrupt preserves host Turn terminal authority.
- [x] Idle interrupt races return `accepted: false`.

Verification: [V-64](../verification/V-64-agent-control-plane.md)

### Canonical lifecycle and storage

- [ ] Normative docs and protocol use the optional `Turn ↔ Run` relation and
      never require a Turn for detached work.
- [ ] Accepted start, steer, and follow-up inputs have durable identities and
      replayable state transitions.
- [ ] Turn, Run, Execution attempt, and ModelStep identities remain distinct
      and queryable after restart.
- [ ] A crash after input acceptance cannot lose or duplicate a pending steer
      or follow-up.

### Projection and interaction

- [ ] Session reconciliation restores active work, pending steers, and the full
      per-agent follow-up queue from host read models.
- [ ] TUI and desktop derive foreground and controls from the same host
      projection without local queue authority.
- [ ] Steer works for detached and Turn-backed active Runs and cannot retarget
      a newer Run after a race.
- [ ] Queued input cancellation works after restart and from a second client.
- [ ] Concurrent control of one AgentInstance is linearized and other agents
      remain unaffected.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Is Turn the root of all work? | No; it is an optional user-interaction relation to a Run | Detached agent/system work is real work without a user Turn |
| What is the admission unit? | Durable AgentInput | Start, steer, queue, idempotency, and cancellation need one identity |
| What owns a queue? | AgentInstance | Admission is serialized per addressable actor |
| Does steer create a Turn or Run? | No | It joins the active Run at a deterministic step boundary |
| When is an input accepted? | After its durable admission fact commits | Accepted work must survive process loss |
| Who owns visible truth? | hostd journal and read models | Keeps all clients convergent and respects the host/orch split |
| Who owns runtime transitions? | orchd Agent runtime | The actor serializes admission and operates Executions |
| Do clients control Executions directly? | No | Agent/Input/Turn are stable product handles; Execution is an attempt |

## Open questions

1. Queue priority/reordering remains deferred; the initial authoritative queue
   is FIFO.
2. Whether a failed Execution retry remains in the same Run is fixed as
   one-to-many by the model; concrete automatic retry policy remains
   feature-owned.

## Reference evidence

- `packages/orchd-api/src/agent.rs`
- `packages/orchd/src/runtime/agent/actor/`
- `packages/session-store/src/schema.rs` and `aggregate.rs`
- `packages/hostd/src/domain/sessions/queues.rs`
- `packages/tui/src/app/submit.rs`
- [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md)
