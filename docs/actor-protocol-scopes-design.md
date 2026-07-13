# Actor Protocol Scopes Design

> Status: implemented runtime contract
> Runtime base: [Agent Runtime Actor Design](single-agent-actor-runtime-design.md)
> Atomicity contract: [Agent Run Atomicity Design](agent-run-atomicity-design.md)

## 1. Purpose

Define a small set of Rust resource-control abstractions for reliable Actor
protocols. They play the role of Python `with` blocks and context managers:
resources are acquired on entry, transferred only through explicit commit
points, and released or compensated on every exit path.

The objective is to move critical invariants out of manually ordered Actor
handler code and into ownership, typestate, and reusable scope machinery.

This is not a general Actor framework and not an attempt to hide business
transactions behind middleware.

## 2. Problem

AgentActor currently enforces the run protocol through a combination of:

- mailbox serialization;
- `AgentRunState` transitions;
- ordered durable commits;
- `PreparedExecution` ownership;
- explicit retry and acknowledgement code.

Those mechanisms are sound, but some invariants remain implicit in handler
ordering:

```text
commit start before activation
commit terminal before publication
retain terminal payload until receiver acknowledgement
rollback a prepared reservation on every early return
complete every reply exactly once
schedule asynchronous cleanup when a future is dropped
```

A later refactor can accidentally reorder or omit one of these operations while
still compiling. Protocol scopes make the legal path explicit and make common
illegal paths unrepresentable.

## 3. Design Principles

1. Actor ownership remains the consistency boundary.
2. Durable commit points remain explicit at the call site.
3. Resource acquisition and release are represented by owned values.
4. State transitions consume the previous state and return the next state.
5. Normal failure performs complete asynchronous compensation.
6. `Drop` is only a synchronous fail-safe for cancellation, panic, or leaked
   guards; it never performs durable business commits.
7. Cleanup is idempotent because explicit rollback and `Drop` may race.
8. A scope never holds a Tokio lock across persistence, model, tool, or approval
   awaits.
9. Middleware may provide observation and safety rails, but cannot silently
   choose business outcomes.

## 4. Context-Manager Semantics

The conceptual protocol is:

```text
enter
  → acquire resources
  → run body
      → explicit durable commit / ownership transfer
  → exit success

body error
  → classify error
  → asynchronous rollback or retain-for-retry
  → exit failure

future dropped / panic / task abort
  → synchronous abort signal in Drop
  → owning Tokio runtime finishes generation-checked cleanup
```

Python expresses this through `__enter__` and `__exit__`. Rust expresses it
through constructors, consuming methods, typestate, explicit async rollback,
and a synchronous `Drop` fallback.

## 5. Why There Is No Generic Async Context Manager

Rust has no async `Drop`. A generic abstraction such as this is therefore
misleading:

```rust,ignore
trait AsyncContextManager {
    async fn exit(self);
}
```

`exit` cannot be guaranteed when its future is cancelled or the owning Tokio
task is aborted. The runtime instead uses two cleanup levels:

| Exit path | Mechanism | Guarantee |
|---|---|---|
| success | consuming ownership transfer | resource has exactly one new owner |
| ordinary error | explicit `rollback().await` | complete asynchronous compensation |
| cancellation/panic/drop | synchronous `Drop` abort | resource is fenced immediately and cleanup is queued |

`Drop` may cancel a token, invalidate a generation, release a synchronous
reservation, or enqueue a cleanup request. It must not write session storage or
wait for Actor acknowledgement.

## 6. Common Building Blocks

### 6.1 Resource Guard

An owned resource that must be transferred or cleaned up:

```rust,ignore
struct ResourceGuard<T> {
    resource: Option<T>,
    abort: AbortHandle,
    cleanup: CleanupSender,
}

impl<T> ResourceGuard<T> {
    fn resource(&mut self) -> &mut T;
    fn transfer(mut self) -> T;
    async fn rollback(mut self) -> Result<(), CleanupError>;
}
```

`transfer` consumes the guard and disarms cleanup. `rollback` performs the full
normal cleanup. `Drop` fences and queues cleanup only while the resource is
still armed.

### 6.2 Reply Guard

Every request/reply Actor command owns a reply obligation:

```rust,ignore
struct ReplyGuard<T> {
    sender: Option<oneshot::Sender<T>>,
    fallback: Option<T>,
}

impl<T> ReplyGuard<T> {
    fn complete(mut self, value: T);
}
```

Dropping an incomplete guard sends a predefined internal-error response when
possible. It does not invent a successful business acknowledgement.

This prevents early returns and newly added match branches from silently
dropping callers.

### 6.3 Handoff Lease

A cross-Actor payload is transferred to the receiving Actor as a lease. The
sender retains a completion waiter, not a second copy of the payload:

```rust,ignore
struct HandoffLease<T> {
    payload: Option<T>,
    completion: Option<oneshot::Sender<HandoffOutcome>>,
}

struct HandoffWaiter {
    completion: oneshot::Receiver<HandoffOutcome>,
}
```

Creating the pair atomically moves the only payload into `HandoffLease`. The
receiving Actor owns that lease until it explicitly accepts or rejects the
handoff. The sender waits for `HandoffOutcome` before releasing its supervision
obligation. Dropping an unresolved lease sends an aborted outcome when possible;
mailbox closure therefore becomes a supervision failure rather than silent
payload loss.

### 6.4 Retry State

Retry policy is data, separate from the business payload:

```rust,ignore
struct RetryState {
    attempts: u32,
    policy: RetryPolicy,
}

enum CommitFailure {
    Retryable(CommitError),
    Permanent(CommitError),
}
```

All durable protocols use the same error classification and capped backoff.
The retry timer sends a self-message; it never sleeps inside the Actor handler.

### 6.5 Drop Cleanup

`PreparedExecution::Drop` schedules generation-checked reservation rollback on
the owning Tokio runtime. Cleanup is idempotent: an explicit rollback and a
later stale Drop cannot remove a newer generation.

No cleanup worker is required while every armed resource is created and dropped
inside the owning runtime. Process loss needs no process-local cleanup; hostd
recovers durably started nonterminal runs as interrupted.

## 7. Run Startup Scope

`RunStartupScope` owns the protocol from prepared Execution through live Actor
activation.

### 7.1 Typestate

```rust,ignore
struct RunStartupScope<P> {
    prepared: ResourceGuard<PreparedExecution>,
    context: RunStartupContext,
    phase: PhantomData<P>,
}

struct Prepared;
struct DurablyStarted {
    ack: AgentCommitAck,
}
struct InputCommitted;

struct ActiveRun {
    handle: ExecutionHandle,
    run_id: AgentRunId,
}
```

Legal transitions consume `self`:

```rust,ignore
RunStartupScope<Prepared>
    .commit_start(command)
    .await
    -> Result<RunStartupScope<DurablyStarted>, StartupFailure>

RunStartupScope<DurablyStarted>
    .commit_input()
    .await
    -> Result<RunStartupScope<InputCommitted>, StartedRunFailure>

RunStartupScope<InputCommitted>
    .activate()
    .await
    -> ActiveRun
```

There is no `activate` method on `RunStartupScope<Prepared>`. Durable ordering is
therefore enforced by the type system rather than a boolean or comment.

### 7.2 Failure Semantics

| Phase | Failure handling |
|---|---|
| prepare | no scope returned; no durable run |
| prepared, start commit fails | async rollback reservation; Agent remains Idle |
| durable start, input commit fails | fence prepared resource; synthesize failed terminal candidate |
| input committed, activate | infallible ownership transfer to supervisor |
| active task panic/abnormal exit | supervisor selects failed terminal candidate |

Once `RunStarted` is durable, rollback cannot erase acceptance. Failures must
converge through `RunTerminal`.

### 7.3 Usage

```rust,ignore
let startup = RunStartupScope::prepare(execution_runtime, request).await?;
let startup = startup.commit_start(agent_commit, command).await?;
let startup = startup.commit_input().await?;
let active = startup.activate().await?;

self.run_state = AgentRunState::Running(active.into_state());
```

The commit points remain visible during review.

## 8. Terminal Commit Scope

`TerminalCommitScope` owns a frozen terminal candidate, its supervisor
acknowledgement, durable retry state, and the publication capabilities that are
unlocked by commit.

### 8.1 States

```rust,ignore
struct PendingTerminal {
    handoff: HandoffLease<FrozenTerminal>,
    retry: RetryState,
}

struct CommittedTerminal {
    report: AgentRunReport,
    transcript: Vec<Message>,
    head_message_id: Option<MessageId>,
    publications: TerminalPublications,
}
```

Only `CommittedTerminal` exposes publication operations:

```rust,ignore
impl CommittedTerminal {
    fn apply_transcript(self, actor: &mut AgentActor) -> PublishableTerminal;
}

impl PublishableTerminal {
    fn resolve_waiters(&mut self);
    async fn schedule_detached_delivery(&mut self);
    async fn advance_follow_up(&mut self);
    fn acknowledge_handoff(self);
}
```

`PendingTerminal` cannot update the reusable transcript or access waiter
senders. An uncommitted success is therefore not expressible through its API.

### 8.2 Commit Outcomes

```rust,ignore
enum TerminalCommitResult {
    Committed(CommittedTerminal),
    Retry(PendingTerminal, RetryAt),
    PermanentFailure(TerminalPersistenceFailure),
}
```

- `Committed` unlocks publication and handoff acknowledgement.
- `Retry` retains all resources in `AgentRunState::Finalizing` and schedules a
  self-message.
- `PermanentFailure` marks the Agent unavailable, fails waiters with a
  persistence error, and acknowledges handoff only after AgentActor has
  accepted ownership of that failure state.

## 9. Detached Delivery Scope

Detached delivery is a durable outbox protocol:

```text
terminal run with pending recipient
→ commit report to recipient inbox
→ mark source delivery complete
→ publish live inbox notification
```

The initial implementation may continue storing pending-delivery metadata on
the terminal run record. `DetachedDeliveryScope` treats that record as an
outbox item regardless of physical schema.

```rust,ignore
struct PendingDelivery {
    source_run_id: AgentRunId,
    recipient_agent_instance_id: AgentInstanceId,
    report: AgentRunReport,
    retry: RetryState,
}

struct CommittedDelivery {
    inbox_item: AgentInboxItem,
}
```

Only `CommittedDelivery` may emit a live `InboxReport`. Delivery failure never
changes the source run outcome and never restarts its Execution.

## 10. Actor Command Scope

`ActorCommandScope` provides narrow cross-cutting behavior around a mailbox
handler:

- tracing span and correlation fields;
- latency and retry metrics;
- panic-to-internal-error conversion at the supervision boundary;
- reply obligation tracking;
- cancellation observation;
- leaked resource-guard diagnostics.

It must not:

- perform an implicit durable commit;
- infer success from handler return;
- mutate Agent business state;
- retry a non-idempotent operation;
- hide mailbox sends or cross-Actor awaits.

Conceptual usage:

```rust,ignore
async fn handle(&mut self, command: AgentCommand) {
    ActorCommandScope::new(command.metadata(), command.reply())
        .run(self, |actor, reply| async move {
            let result = actor.handle_command(command).await;
            reply.complete(result);
        })
        .await;
}
```

Business protocol scopes remain explicit inside `handle_command`.

## 11. Cancellation and Panic

Rust futures may disappear without returning `Result` when:

- their owning task is aborted;
- a parent future wins a `select!` branch;
- Actor shutdown drops queued work;
- code panics and supervision catches unwind.

Every armed scope therefore has an abort-safe synchronous action:

| Scope | Synchronous `Drop` action |
|---|---|
| startup before durable start | fence and enqueue reservation rollback |
| startup after durable start | abort Execution resource and notify supervisor of abnormal started run |
| terminal | retain failure in Agent supervision state; never publish candidate |
| detached delivery | leave durable outbox pending |
| reply | send internal failure if channel remains open |

Process termination needs no async cleanup. On restart, hostd identifies
durably started nonterminal runs as interrupted and reconstructs pending
deliveries from durable state.

## 12. Structured Concurrency

Protocol scopes control resource ownership, while supervisors control Tokio task
lifetime:

```text
SessionAgentScope
  └─ AgentActor supervisor
      ├─ AgentActor task
      ├─ active Execution supervisor
      │   └─ ExecutionActor task
      └─ generation-checked Drop cleanup
```

No detached Tokio task may own the sole copy of a terminal candidate, reply
obligation, or cleanup responsibility. A spawned task must transfer those
resources back through a reliable handoff or leave a durable recovery record.

## 13. Observability

Every scope has a stable protocol ID and records phase transitions:

```text
scope_entered
scope_committed
scope_transferred
scope_rolled_back
scope_aborted
scope_cleanup_queued
scope_cleanup_completed
scope_retry_scheduled
scope_leaked
```

These are diagnostic events, not user-visible Agent events. Metrics should make
stuck Finalizing states, leaked reservations, repeated commit retries, and
unacknowledged handoffs visible.

## 14. Non-Goals

- replacing Tokio channels with an Actor framework;
- providing arbitrary distributed transactions;
- implementing async `Drop` through blocking executors;
- hiding persistence behind annotations or procedural macros;
- making every future or tool call an Actor;
- treating rollback as reversal of external model or tool side effects;
- moving hostd's durable authority into orchd.

## 15. Implementation Sequence

Implemented:

1. `runtime/reliability` owns retry classification and terminal handoff leases.
2. `RunStartupScope` provides the prepared, durably-started, input-committed,
   and activated startup typestates.
3. `TerminalCommitScope` restricts report, transcript, waiter, and detached
   publication until durable terminal acknowledgement.
4. `MessageCommitScope` gives ExecutionActor a committed-message capability
   before transcript/head mutation.
5. `DetachedDeliveryScope` retains its frozen report, recipient, idempotency
   identity, and retry state.
6. `ActorCommandScope` owns request/reply obligations, supplies an explicit
   failure fallback on early drop, and supports consuming ownership transfer to
   longer-lived waiters and durable follow-ups.

Deferred until justified by another concrete resource:

1. Extract the existing `PreparedExecution` drop rollback into a reusable
   generation-checked reservation guard when another runtime resource needs the
   same cleanup semantics.

## 16. Verification

Tests must demonstrate:

- activation cannot be called on an uncommitted startup typestate;
- dropping a prepared startup releases or fences its reservation;
- explicit rollback and `Drop` cleanup are idempotent;
- failure after durable start produces a terminal candidate rather than erasing
  the run;
- only a committed terminal can advance reusable transcript state;
- terminal retry retains handoff and waiter ownership;
- reply guards complete exactly once on success, error, and early return;
- task abort cannot silently lose a prepared reservation or terminal handoff;
- cleanup requests reject stale generations;
- detached delivery remains pending across Actor and Session restart;
- duplicate detached delivery produces one inbox item;
- middleware never emits a successful business acknowledgement by itself.

The suite includes direct scope tests for explicit completion, fallback on
Drop, reply ownership transfer, prepared-resource Drop, task abort, stale
generation cleanup, first-wins terminal selection, retry classification, and
acknowledged/unacknowledged handoff leases.

The runtime test suite additionally enforces the detached crash boundary:

- durable recipient registration precedes a fast terminal result;
- terminal persistence precedes inbox delivery;
- a recovered pending delivery performs no model call and does not rerun the
  source Agent;
- duplicate `CommitReport` commands produce one durable inbox item;
- successful inbox commit removes the run from pending-delivery recovery.

## 17. Invariants

1. Every acquired runtime resource has exactly one owner.
2. Every ownership transfer consumes the previous owner.
3. Every armed guard has both an explicit async rollback path and a synchronous
   abort fallback.
4. `Drop` never performs durable business I/O.
5. No live publication capability exists before its durable commit.
6. A durable start is never compensated by deletion; it converges through a
   terminal record.
7. Retry retains the original idempotency key and frozen payload.
8. Cleanup is idempotent and generation-checked.
9. Cross-Actor handoff payloads remain owned until acknowledgement.
10. Business commit points remain explicit and reviewable.
