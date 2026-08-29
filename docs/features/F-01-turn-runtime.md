# F-01: Turn & agent runtime

> Status: implemented
> Priority: P0
> Source evidence: codex-rs `core/src/session/{session,turn,turn_context,input_queue,user_message_admission}.rs`,
> `core/src/state/{service,session,turn}.rs`, `core/src/tasks/{regular,lifecycle,compact,review,user_shell}.rs`,
> `core/src/context/turn_aborted.rs`
> Refined by: [F-51](F-51-agent-control-plane.md) for Agent/Input/ModelStep primitive facts and derived Run/Execution/UserTurn/queue/foreground projections

## Summary

A user submission that starts future work becomes a **Turn** related to one
Agent Run; detached agent/system work has a Run without a Turn, and steer joins
an existing Run. Work has a durable lifecycle from admission, through model
steps and tool execution, to a terminal outcome.
Under F-51, this Turn is a host product view derived from the user-origin
AgentInput and its optional AgentRun; it is not an independent lifecycle
authority.
The runtime guarantees that everything a user or agent observes is committed
before it is visible, that transcripts are deterministic and replayable, that
concurrent input is admitted through an explicit contract (start, steer, queue,
duplicate, overload), that cancellation aborts work and reconstructs history
with a turn-abort marker, and that long-lived background work runs as typed,
cancellable tasks alongside turns.

## Problem

An agent runtime that accepts messages and streams model output is easy to
build and hard to make correct. Without a single contract for what a turn is
and when its state becomes visible, clients cannot rely on resumability,
deterministic replay, or safe concurrent input. piko already carries a
substantial turn runtime, but three behaviors are underspecified or missing:

1. **Input admission.** What happens when a message arrives while an agent is
   idle, running, queued, closed, or terminating currently differs per code
   path and is not expressed as one observable contract. Callers cannot predict
   whether they get a start, a steer, a queue, a duplicate, or an overload.
2. **Turn-abort markers.** After a cancellation, a transcript ends mid-stream:
   tool calls carry bounded `aborted` results, but there is no durable,
   model-visible marker explaining that the turn was interrupted and that some
   work may have partially executed. A crash without a terminal commit leaves
   the same ambiguity; recovery needs to reconstruct history without rerunning
   work.
3. **Typed background tasks.** Long-lived side work (auto-compaction, review,
   user-shell commands) has no task taxonomy, typed results, or cancellation
   discipline, so shutdown can orphan work and results cannot be attributed to
   the turn that spawned them.

## User journeys

1. A user submits a message while the agent is idle. The turn is accepted, its
   start and input are durably committed, model steps and tool results stream,
   and the turn ends with a completed outcome and usage accounting.
2. A user submits a second message while the first turn is still running. The
   runtime steers the message into the running turn at the next model-step
   boundary, or queues it as a follow-up when the sender asked for queued
   delivery; in both cases the transcript stays deterministic.
3. A user cancels a run mid-tool-call. In-flight calls are aborted with bounded
   results, no new calls start, the turn terminates with a cancelled outcome,
   and the transcript receives a durable marker stating that the turn was
   interrupted and some work may have partially executed.
4. A host process crashes mid-turn. On restart the session attaches, the
   interrupted run reconstructs as aborted from durable state, no model or tool
   work runs twice, and queued follow-ups still run exactly once.
5. A client retries a submission with the same request id. The runtime returns
   a duplicate receipt with no side effects; the same id with different content
   is rejected as a conflict.
6. A parent agent queues a follow-up for a busy child. The child starts the
   follow-up after its current turn, and the report is delivered to the parent
   (or its inbox) exactly once.
7. Background work such as an automatic review runs as a typed task. Session
   shutdown aborts it, marks it cancelled, and leaves no orphaned process or
   uncommitted result.

## In scope

- Turn lifecycle: admission → started → model steps → terminal, with durable
  commit points at start, input, each message, and terminal.
- Deterministic, replayable transcript commits: commit order follows message
  and tool-call order, never completion order.
- Input admission: delivery intent × agent state, dispositions, idempotency,
  and overload semantics.
- Follow-up queue: durable FIFO queueing, cancel of queued input, retry of
  transient start failures, and exactly-once report delivery.
- Cancellation: cancellation reasons, abort of in-flight tool calls, terminal
  outcome with reason, and a durable turn-abort marker with history
  reconstruction.
- Typed background tasks: task kinds, typed results, cancellation, and
  lifecycle bound to sessions and turns.
- Attachment/detachment: restoring run history, inboxes, queued inputs, and
  pending deliveries on session attach, without duplicate execution or
  delivery.
- Multi-agent binding: child-agent runs without their own interaction turn,
  source-turn attribution for committed messages, and detached report
  delivery.

## Out of scope

- Model gateway behavior: provider selection, streaming, retry/backoff,
  fallback, and usage middleware (`F-02 model-gateway`).
- Prompt assembly: fragment catalog, cache planning, AGENTS.md merging
  (`F-03 prompt-assembly`).
- Transcript history management, token budgeting, and truncation
  (`F-04 context-management`).
- Compaction algorithms and auto-compact triggering (`F-05 compaction`); only
  the task infrastructure for running compaction appears here.
- Tool execution modes, parallel batches, and tool-result shaping
  (`F-06 tool-system`).
- Approval flows, permission requests, and timeouts (`F-07 tool-approvals`).
- The guardian review loop and safety assessment (`F-11 guardian`,
  `F-12 safety`); only the typed review-task infrastructure appears here.
- The multi-agent tool surface itself (`F-10 multi-agent`).

## Behavior and states

### Turn lifecycle

A Turn is created when a user input requests a new Run and covers that one Run.
Child agents spawned by multi-agent tools execute Runs that are *not* bound to
an interaction Turn; their messages carry no source Turn and their reports
deliver to the parent. A steer is another AgentInput applied to the existing
Run and does not create a Turn or Run. F-51 defines the canonical AgentInput,
Turn, Run, and queue relationship.

State transitions observable to clients:

1. **Queued** — the turn is registered but not yet admitted to an execution
   (used when the agent is busy and delivery is queued).
2. **Started / Running** — the run start and the submitted input are durably
   committed before the first model request.
3. **Terminal** — the run ends as **Completed**, **Failed**, or **Cancelled**,
   each with a durable terminal commit; a completed turn carries usage.

The terminal transition is authoritative: a turn whose start was committed but
whose terminal was not is an interrupted turn and reconstructs as aborted on
recovery (see Turn-abort markers).

### Durable commit points

The following are committed to durable history before they become visible:

- Run start: run id, request id, source turn, prompt assembly version and
  digest, start time.
- Submitted input: the input message becomes the first committed message and
  the durable head of the transcript.
- Every assistant message, tool call, tool result, and steered user message in
  deterministic order.
- Run terminal: outcome, summary, usage, finish time.

Commitment is linearizable with visibility: a message observed by a client has
already been durably committed; an uncommitted message is not published.

### Input admission

Delivery intent is one of four modes. The disposition returned to the caller is
one of `accepted`, `queued`, `duplicate`, or `overload`.

| Delivery mode | Agent idle | Agent running |
|---|---|---|
| Auto | start a new run (accepted) | steer into the running run (accepted) |
| Start when idle | start a new run (accepted) | reject (execution already active) |
| Steer active | reject (no active run) | steer into the running run (accepted) |
| Follow up | start a new run (accepted) | queue as follow-up (queued) |

Additional admission rules:

- Agents in a closed or terminated lifecycle reject all input.
- A retried request id with identical content returns a duplicate receipt with
  no side effects; the same id with different content is rejected as a
  conflict.
- Input to a busy execution that cannot be accepted promptly is rejected as
  overload rather than silently dropped or infinitely queued.
- Steered messages commit at model-step boundaries in submission order, so the
  transcript shape is identical regardless of when the model returns.
- Steer-active delivery while the agent is idle is rejected by the state
  machine; it never implicitly starts a run.

### Follow-up queue

- Follow-up inputs queue durably in FIFO order with a stable queued-input id.
- A queued follow-up starts after the current run reaches terminal, in queue
  order, and its start is durably recorded as a queued-input start.
- Cancelling a queued follow-up commits the cancellation and notifies any
  waiter with a cancelled result; it never starts.
- Start failures leave the follow-up queued and retry; a queued follow-up
  cannot be silently lost.
- The queue is bounded by a fixed cap; exceeding the cap returns overload.

### Cancellation and turn-abort markers

Cancellation reasons: user requested, session shutdown, runtime shutdown, and
superseded (a new run takes over).

- A cancel request is acknowledged immediately; the terminal outcome is
  separate and durable.
- Cancellation aborts in-flight tool calls (parallel and sequential), starts no
  new calls, and commits a bounded `aborted` result for every call of the
  interrupted step so the transcript remains complete and replayable.
- The run terminates with a cancelled outcome carrying the reason.
- An interrupted turn appends a durable, model-visible **abort marker** at the
  interruption point stating that the turn was interrupted and that any
  aborted tools may have partially executed. Clients also receive an abort
  event for the turn.
- A run interrupted by a crash (start committed, terminal not) reconstructs as
  aborted on recovery: history is rebuilt from durable commits, the abort
  marker is applied, and no model or tool work reruns. Queued follow-ups still
  run exactly once.

### Typed background tasks

Background work spawned by or alongside a turn runs as a typed task with:

- A feature-owned task kind and a typed result.
- A lifecycle from pending through running to terminal (succeeded, failed, or
  cancelled) that is attributable to the owning turn and agent.
- Cancellation that aborts in-flight work and marks the task cancelled;
  session shutdown cancels all tasks owned by the session.

Task infrastructure lands in F-01; concrete task kinds land with the features
that need them (compaction with F-05, review with F-11, user-shell with F-08).
Each feature persists its results through the durable channel it already owns
(transcript messages or inbox reports); F-01 introduces no new durable
surface.

### Attachment/detachment and recovery

- Attaching a session restores each agent's durable run history, head message,
  inbox, latest report, queued inputs, and pending detached deliveries.
- Detached report delivery is exactly-once: a delivery committed to the
  recipient's inbox is not redelivered, and recovery does not rerun the source
  run.

## Acceptance criteria

Landed behavior (verifiable against the current runtime):

- [x] Submitting while idle accepts, durably commits start and input before the
      first model request, and commits a terminal outcome with usage.
- [x] Transcript commits are deterministic: messages and tool results commit in
      call order, independent of completion order.
- [x] Cancelling mid-run aborts in-flight tool calls, commits a bounded aborted
      result for every call of the interrupted step, starts no new calls, and
      terminates with a cancelled outcome.
- [x] Follow-up delivery queues durably while a run is active, starts after the
      run in FIFO order, and delivers its report exactly once.
- [x] Session attach reconstructs history, inboxes, queued inputs, and pending
      deliveries without duplicate execution or delivery.
- [x] Retrying a request id with identical content returns a duplicate receipt
      with no side effects; reuse with different content is rejected.
- [x] Closed or terminated agents reject input.

Delivered slices (D-01, verified in V-01):

- [x] The full admission matrix (delivery mode × agent state) is implemented
      and tested, including a fixed-cap follow-up queue that returns overload
      rather than unbounded growth.
- [x] An interrupted turn leaves a durable, model-visible abort marker, clients
      receive an abort event, and a crash-interrupted run reconstructs as
      aborted without rerunning work.
- [x] Typed background tasks provide kinds, typed results, cancellation, and
      session-scoped lifecycle, with no orphaned work on shutdown.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What does Auto delivery do? | Starts when idle, steers when running | Matches piko's existing delivery modes; consistent with codex-rs admission (`Started` vs `Steered`) |
| Follow-up delivery while idle | Starts a run immediately | A queued intent with no active work has nothing to wait for |
| What does an abort marker say? | A model-visible note that the turn was interrupted and aborted tools may have partially executed | Model must not assume atomicity of aborted work (codex-rs `TurnAborted` guidance) |
| How do terminals stay durable? | Terminal commit is authoritative; interrupted runs reconstruct as aborted | Prevents phantom in-flight runs after crash |
| Where do background tasks live? | Session-scoped task infrastructure in F-01 with feature-owned kinds; results persist through each feature's existing durable channel (transcript, inbox) | Avoids a codex-rs-style central taxonomy and a new durable surface (ADR-001) |
| How is overload surfaced? | Bounded follow-up queue with a fixed cap; excess returns overload | Bounded memory and a clear, retryable client signal; no per-agent configuration (exact value is an implementation detail in D-01) |
| Steer-active while idle | Rejected as a caller error; the state machine refuses to start | Avoids ambiguous implicit behavior; callers choose Auto or Start-when-idle explicitly |
| Abort-marker verbosity | Always model-visible with fixed wording | The model must always be able to infer that partial execution may have occurred; no environment-dependent trust levels |

## Fusion decisions (codex-rs, ADR-002)

codex-rs is a modeling reference, not a parity target. F-01 keeps the
behaviors that piko intentionally models, adapts details to piko semantics,
and rejects codex-shaped mechanisms that have no piko consumer.

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Turn lifecycle and durable state transitions (`session/{session,turn,turn_context}.rs`) | Kept | Already modeled natively by piko's agent/execution actors and durable commit scopes; the PRD codifies the contract |
| User-message admission (`user_message_admission.rs`) | Kept (adapted) | piko models admission with four delivery modes and four dispositions; only the behavior contract (start vs steer vs queue) is kept |
| Follow-up input queue (`input_queue.rs`) | Kept (adapted) | piko's durable FIFO queue gains a fixed cap and overload; codex-rs has no bounded-cap semantics |
| Turn-abort marker (`context/turn_aborted.rs`) | Kept (adapted) | Wording covers all cancellation reasons (user, session shutdown, runtime shutdown, superseded), not just user interruption; committed as `Message::Context` through piko's commit pipeline |
| Turn-abort reconstruction (`rollout_reconstruction.rs`) | Kept (adapted) | piko reconstructs via its existing `interrupt_incomplete_agent_executions` sweep plus the stable-id marker; no codex rollout machinery |
| Typed task taxonomy (`tasks/{regular,compact,review,user_shell}.rs`) | Rejected | No piko consumer: compaction is hostd-owned, user-shell results are tool results, review results land in the inbox. F-01 keeps only session-scoped cancellable task infrastructure with feature-owned kinds; see D-01 |

## Reference evidence

- codex-rs turn lifecycle and input queueing:
  `core/src/session/{session,turn,turn_context,input_queue}.rs`
- codex-rs admission result and pending-admission idempotency:
  `core/src/user_message_admission.rs`
- codex-rs typed task taxonomy and cancellation:
  `core/src/tasks/{regular,lifecycle,compact,review,user_shell}.rs`,
  `core/src/state/turn.rs`
- codex-rs turn-abort marker and reconstruction:
  `core/src/context/turn_aborted.rs`,
  `core/src/session/rollout_reconstruction.rs`

The referenced codex-rs files are evidence only (ADR-001); behaviors enter piko
only as specified in this document.
