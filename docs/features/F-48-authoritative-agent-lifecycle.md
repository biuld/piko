# F-48: Authoritative agent lifecycle boundaries

> Status: implemented
> Priority: P0
> Source evidence: piko runtime/journal review; extends [F-01](F-01-turn-runtime.md), [F-31](F-31-durable-session-journal.md), and [F-36](F-36-agent-run-trajectory.md)
> Design: [D-65](../design/D-65-authoritative-agent-lifecycle.md)
> Decision: [ADR-025](../decisions/ADR-025-authoritative-agent-lifecycle.md)

## Summary

piko gives every agent operation an explicit lifecycle hierarchy:

```text
Turn → Run → Execution → ModelStep → Thought / ToolCall
```

`hostd` owns the durable, user-visible state. A Turn is the host interaction
boundary, a Run is the logical agent attempt, an Execution is the concrete
runtime instance, and a ModelStep is one model request/response boundary.
Thought segments are presentation-level content inside a ModelStep. A tool
call ends the current ModelStep and is persisted as a separate transcript
fact. Realtime deltas remain transient observations and never become journal
authority.

## Problem

The current runtime persists messages and execution terminals, but does not
persist the relation that says which assistant message and tool declarations
belong to one completed model step. The model-step trajectory is optional and
may be dropped. A realtime `MessageEnded` frame may also be lost while the
client keeps an active thought timer running. Finally, `run_id` and
`execution_id` are carried as separate fields but are normally identical,
which makes recovery and diagnostics ambiguous.

## User journeys

1. A Turn starts a Run and its concrete Execution. The start and input facts
   are durable before model work is exposed.
2. A model emits `thought → tool call`. The thought closes at the tool-call
   boundary; the completed ModelStep atomically records the assistant message
   and ordered tool-call declarations. Tool results are committed later in
   deterministic call order.
3. A client loses realtime frames. Reliable ModelStep/message facts still
   replace the draft and freeze the thought with the duration carried by the
   committed assistant message.
4. The host crashes after a ModelStep commit but before tool results. Replay
   reconstructs the completed step and identifies the declared calls that do
   not yet have results; recovery can mark those calls interrupted without
   rerunning the model step.
5. A session is reopened. Turn attribution, logical Run identity, concrete
   Execution identity, and completed ModelSteps remain queryable from the
   journal projections.

## In scope

- Explicit identity and ownership for Turn, Run, Execution, and ModelStep.
- A required journal `model_step_committed` fact that references existing
  message facts instead of duplicating their content.
- One atomic durable commit for an assistant response plus its ordered tool
  declarations.
- Reliable publication of the ModelStep boundary after the journal append.
- Realtime/client convergence when `MessageEnded` is missing or late.
- Distinct logical Run and concrete Execution identifiers.
- Recovery validation for unfinished executions and declared tool calls.
- Keeping thought text/durations in committed assistant content; no per-delta
  journal writes.

## Out of scope

- Persisting every token, thought-start, or thought-end delta.
- Making trajectory records authoritative; F-36 remains observational.
- Changing provider streaming protocols or tool execution policy.
- Adding a second session database or a client-owned durable cache.
- Reconstructing exact durations for legacy thinking blocks without metadata.

## Behavior and states

### Boundary ownership

| Boundary | Meaning | Durable authority |
|---|---|---|
| Turn | Host interaction and user-visible lifecycle | host turn lifecycle plus journal attribution |
| Run | One logical agent attempt, including child-agent attempts | `run_id` on execution facts and trajectory identity |
| Execution | One concrete runtime instance that can be admitted, interrupted, and recovered | `execution_started` / `execution_finished` |
| ModelStep | One model request/response, ending before local tool execution | required `model_step_committed` plus referenced messages |
| Thought | Ordered reasoning segment inside a ModelStep | `ContentBlock::Thinking` in the committed assistant message |
| ToolCall | Model-declared call and later result | committed `ToolCall` / `ToolResult` messages |

The same value may be used for two IDs by a caller only when their semantic
boundaries genuinely coincide; the protocol and journal must still preserve
both fields. New root Runs use the Turn ID as their logical Run ID, while the
concrete Execution ID is independently derived from the request and agent.

### ModelStep lifecycle

```text
started → completed (answer)
        → completed (tool declarations)
        → failed / cancelled
```

An open thought closes when text, a tool call, another semantic output, step
completion, failure, or cancellation is observed. A tool declaration closes
the step's model output. The step commit is the durable freeze point; tool
execution begins only after it succeeds.

The journal stores one required boundary fact with:

- Turn, Run, Execution, agent, step ID, and ordinal;
- start and finish timestamps and outcome;
- assistant message ID;
- ordered tool-call message IDs.

Message bodies are stored once by `message_committed` facts. The boundary
fact is invalid unless all referenced messages are present, ordered, belong to
the same Execution, and have the expected roles and ancestry.

### Failure and recovery

- If realtime output is dropped, reliable ModelStep publication and journal
  replay replace the draft with committed content.
- If a process stops after the ModelStep commit, replay does not rerun that
  step. Declared calls without results are treated as unresolved work and are
  completed by the existing interruption/recovery policy.
- If a model stream fails after producing an assistant error message, the
  failed ModelStep is still committed before the Execution terminal.
- If persistence fails, the runtime does not advance its private transcript or
  expose a committed boundary.
- A duplicate ModelStep commit with the same identity and content is
  idempotent; a conflicting reuse of the step ID is rejected.

## Acceptance criteria

- [x] A successful model step with zero or more tool calls is represented by
      one journal commit containing the assistant message, ordered tool-call
      messages, and one required ModelStep fact.
- [x] The journal rejects a ModelStep whose IDs, ancestry, roles, order, or
      Execution relation are inconsistent.
- [x] A tool call cannot remain nested under an open thought in the canonical
      client projection; it closes the thought and the step.
- [x] A missing realtime `MessageEnded` frame cannot leave a completed thought
      timer running after the reliable ModelStep/message boundary arrives.
- [x] Replay reconstructs Run and Execution identity separately and exposes
      completed ModelSteps without reading trajectory events.
- [x] A failed/cancelled model response does not advance the runtime transcript
      until its ModelStep commit succeeds.
- [x] Legacy journals without ModelStep facts remain readable; newly written
      model responses use the required boundary.

Verification: [V-60](../verification/V-60-authoritative-agent-lifecycle.md)

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Is thought a journal entity? | No; it is an ordered block in the assistant message | Avoids high-volume timer/delta events while preserving committed duration |
| What closes a ModelStep? | The model response ends; a local tool declaration ends the model side before tool execution | Keeps model request/response and tool execution as separate boundaries |
| Are trajectory records authoritative? | No | They are intentionally best-effort diagnostics under F-36 |
| How are tool declarations persisted? | Atomically with their assistant message | Recovery must never observe a durable assistant response without its declared call set |
| Who owns Turn state? | hostd | Turn lifecycle is user-visible and must not be inferred by orchd |

## Open questions

1. A future schema revision may add explicit durable Turn start/terminal facts
   if host-level queued Turn state needs to survive without a corresponding
   Execution. This slice keeps the existing host Turn lifecycle and its
   `source_turn_id` correlation.

## Reference evidence

- `packages/session-store/src/schema.rs` and `aggregate.rs`
- `packages/hostd/src/infra/storage/session_store/journal/mutations.rs`
- `packages/orchd/src/runtime/execution/actor/run.rs`
- `packages/orchd/src/runtime/events/delta_lane.rs`
- [F-31 durable session journal](F-31-durable-session-journal.md)
- [F-36 agent-run trajectory](F-36-agent-run-trajectory.md)
