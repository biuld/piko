# D-65: Authoritative agent lifecycle boundaries

> Status: implemented
> Implements: [F-48](../features/F-48-authoritative-agent-lifecycle.md)
> Decisions: [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md), [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Make the Run → Execution → ModelStep spine and optional source Turn relation
explicit across the runtime, host journal, reliable observation stream, and
client projection. F-51/D-68/ADR-027 supersede this design's original
assumption that every Run has a Turn.
The vertical slice must make a completed model step an atomic durable relation
between existing transcript messages and must leave realtime thought rendering
as an ephemeral projection.

## Constraints and non-goals

- The schema-v4 append-only journal remains the sole durable authority.
- `piko-protocol` contains DTOs only; validation and reduction remain in
  `piko-session-store` and host adapters.
- Realtime remains lossy. Reliable observation is published only after a
  successful journal append.
- Do not write token or timer events to the journal.
- Keep legacy single-message commit paths for startup/context/steer/tool
  results and compatibility tests.
- Keep trajectory optional and observational.

## Proposed design

### Identity

`TurnId` remains the host interaction ID and is carried as
`source_turn_id`. `run_id` is the logical agent attempt. For root turns it is
the submitted operation/Turn ID; child-agent requests use their request ID.
`execution_id` is the concrete orchd instance derived from agent + request.
The existing journal already stores both on `ExecutionStartedV1`; orchd now
passes the distinct values through its identity and terminal scopes.

`ModelStepCommit` is the host-facing durable write contract:

```text
session, source_turn, run, execution, agent
step_id, step_index, started_at, finished_at, outcome
assistant_message: MessageCommit
tool_call_messages: Vec<MessageCommit>
```

The nested messages carry their normal private parent IDs. The host computes
missing tree parents and verifies that the assistant extends the admitted
Execution head and each tool declaration extends the previous message in the
same atomic step.

### Journal event and reducer

`piko-session-store` adds required `EventData::ModelStepCommitted` with a
versioned `ModelStepCommittedV1` DTO. It stores only IDs and boundary metadata:

```text
assistant_message_id
tool_call_message_ids[]
```

The aggregate adds a `model_steps` map and validates the event against the
already-applied message events in the same commit. The event must reference an
existing Execution, matching Run/agent/Turn identity, an assistant message,
ordered ToolCall messages, and a monotonic step index. The aggregate does not
derive authority from trajectory records.

One host append contains, in order:

1. assistant `MessageCommitted` and tree entry;
2. usage fact when present;
3. each ToolCall `MessageCommitted` and tree entry in model order;
4. the required `ModelStepCommitted` fact.

The append uses a stable `(session, execution, step)` commit ID. Retry of the
same proposal returns the existing revision; conflicting identity or content
is an idempotency error.

### Runtime commit boundary

`ExecutionCommitPort` gains `commit_model_step`. `ExecutionActor` builds the
assistant and tool declaration messages from the completed `StepDispatch`,
calls the atomic port, and advances its private transcript only after the
acknowledgement. `execute_and_commit_tools` no longer commits declarations;
it starts execution only after the step commit and commits results in the
existing deterministic call-index order.

For a failed model stream that produced an assistant error message, the actor
commits a failed ModelStep with no tool declarations and then returns the
stream error. A cancellation before an assistant response has no completed
ModelStep; Execution terminal/recovery remains authoritative for it.

### Reliable observation and client convergence

The host router publishes one reliable `SessionEvent::ModelStepCommitted`
after the journal append. The observation projector loads the referenced
messages from the durable store and emits ordinary
`TranscriptCommitted` messages in their journal order, followed by a
client-facing `ModelStepCommitted` boundary event. This keeps existing
message consumers compatible while giving clients a reliable step close.

`piko-client-core` records the boundary and closes the draft's active thought
by assistant message ID. The committed assistant message then supplies the
authoritative `ContentBlock::Thinking.duration_ms`. TUI may use a local
monotonic clock only while the draft is open; it never invents a completed
duration. A ModelStep event from another agent or Turn cannot close this
draft because all updates are keyed by agent and assistant message identity.

The `ToolExecutionEvent::Ended` DTO also carries `parent_message_id`, so
legacy stream-item conversion cannot detach a tool result from its model step.

### Turn/Run/Execution projection

The host's existing Turn lifecycle remains the user-visible Turn authority.
Execution start/finish journal facts provide durable Run/Execution state and
the `source_turn_id` relation. Session-store and host projections expose the
model-step records under their Execution; no new client-owned persistence is
introduced. Explicit durable Turn start/terminal facts remain a follow-up if
queued Turns need independent restart semantics.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | ModelStep commit/identity/outcome DTOs; reliable ModelStep event; tool-ended parent identity |
| `piko-session-store` | Required model-step event, aggregate reduction, stored projection, replay validation |
| `piko-orchd-api` | Atomic `ExecutionCommitPort::commit_model_step` |
| `piko-orchd` | Distinct Run/Execution identity; atomic step commit; tool declaration ordering |
| `piko-hostd` | Journal mutation, reliable router/projector, model-step projection/read model |
| `piko-client-core` | Durable model-step boundary projection and draft closure |
| `piko-tui` | Consume the boundary; keep active elapsed display presentation-only |

## Reusable infrastructure

No `island-rs` change required. This slice changes lifecycle/data authority,
not desktop controls or presentation primitives.

## Failure and cancellation

- A journal validation failure leaves the aggregate and runtime transcript
  unchanged.
- Tool execution does not begin until its declaration commit succeeds.
- A crash after the atomic step append can only leave tool results unresolved;
  recovery marks the owning Execution interrupted using the existing policy.
- A missing delta or reliable event causes reconciliation from the journal;
  it cannot keep a completed draft authoritative.
- A failed ModelStep commit fails the Execution rather than publishing an
  uncommitted assistant message.
- Optional trajectory writes may arrive before or after the required fact and
  do not affect replay.

## Verification

- Protocol serde tests for model-step DTOs and tool-ended parent identity.
- Session-store reducer tests for atomic relation, invalid references,
  idempotent retry, replay, and legacy events.
- Host integration test that one model-step commit produces one journal
  revision and ordered reliable observation.
- Orchd tests that declarations are committed once before tool execution and
  actor state advances only after the atomic acknowledgement.
- Client-core/TUI tests that a missing `MessageEnded` is closed by the reliable
  ModelStep boundary and committed duration remains frozen.
- `cargo fmt --all`, workspace tests, and clippy with warnings denied.

## Alternatives considered

- **Persist thought start/end events:** rejected because they turn rendering
  detail into high-volume durable state and still do not define model/tool
  ownership.
- **Use trajectory model-step records as authority:** rejected by F-36; the
  capture path is explicitly best-effort.
- **Keep separate message commits and infer steps by adjacency:** rejected;
  concurrent writers/recovery make adjacency insufficient and can expose an
  incomplete declaration set.
- **Make the client close drafts on any later timeline row:** rejected; rows
  from other agents/Turns are unrelated lifecycle domains.

## Rollout

1. Land the PRD/ADR/design and protocol/reducer DTOs.
2. Add atomic host commit and reliable observation.
3. Switch orchd model-step and Run/Execution identity paths.
4. Add client convergence and recovery tests.
5. Run the full verification suite and update F-01/F-31/F-36 status notes.
