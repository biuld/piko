# F-06: Tool execution contract

> Status: reviewed
> Priority: P0
> Source evidence: codex-rs `core/src/tools/parallel.rs`, `core/src/tools/registry.rs`,
> `core/src/tools/router.rs`, `core/src/tools/handlers/{view_image,tool_search,shell_command}.rs`

## Summary

When a model step yields one or more tool calls, the agent runtime executes
them as a **batch**, honoring each tool's execution mode: parallel-capable
tools run concurrently, sequential tools run exclusively, and every result is
committed to the transcript in deterministic call order. Cancelling a run
aborts in-flight calls and still commits a bounded error result for each one.

## Problem

Model providers commonly emit several independent tool calls in one step.
Executing them one at a time makes a run wait for slow tools that could have
run in parallel. Executing them carelessly is worse: mutating tools can race
with each other, and concurrent completion order would make the append-only
transcript non-deterministic, breaking resumability and differential
replay. The runtime needs a small, explicit contract for what may overlap and
what the transcript must look like afterwards.

## User journeys

1. An agent reads three files in one step. The runtime runs the three
   read calls concurrently and returns results in the order the model issued
   them, so the next model step sees the same transcript shape it would see
   with sequential execution.
2. An agent edits a file while reading another in the same step. The edit
   runs exclusively (never concurrent with the read), and the read may run
   before or after it; the transcript still lists results in call order.
3. A user cancels a run while a long tool call is in flight. The call is
   aborted, no further calls start, and the transcript receives a bounded
   error result for every in-flight call, in call order.

## In scope

- Batch dispatch for one model step's tool calls.
- Execution modes (`parallel`, `sequential`) and their mutual-exclusion
  guarantees within a batch.
- Deterministic transcript commit order independent of completion order.
- Cancellation of in-flight batch calls with bounded error results.
- A configurable concurrency cap for parallel calls.
- Unknown-tool routing to a bounded error result (unchanged from current
  behavior).

## Out of scope

- Provider-level output truncation details; providers are responsible for
  returning bounded results (the built-in workspace tools already truncate).
- Tool approval flows (already specified and implemented separately).
- Cross-batch scheduling; batches from different model steps are already
  sequential by the run loop.
- Retry/backoff policy for failed tool calls.

## Behavior and states

### Mode resolution

The effective execution mode of a tool call is:

1. The `ToolDef.executionMode` after per-tool policy projection, or
2. `parallel` when the owning tool set declares `allowParallel: true` and the
   tool does not opt out, otherwise
3. `sequential` (fail-closed default).

`sequential` is the default for any tool that does not declare otherwise,
mirroring codex-rs where `supports_parallel_tool_calls()` defaults to `false`.
Parallel is an opt-in property of tools known to be safe to overlap (read-only
tools), never inferred from arguments.

### Batch execution

- A batch is the set of tool calls from one assistant message.
- Parallel calls may overlap each other and are bounded by the batch
  concurrency cap (`maxConcurrentCalls` of the owning tool set; unbounded when
  unset).
- A sequential call runs exclusively: no other call in the batch overlaps it.
- Calls that fail routing still occupy their transcript slot and produce a
  bounded error result.

### Transcript commit order

The runtime commits in this fixed order:

1. The assistant message.
2. Each `ToolCall` message, in `toolCallIndex` order.
3. Each `ToolResult` message, in `toolCallIndex` order.

Completion order never changes transcript order. This keeps the append-only
transcript deterministic per run, which is required for durable resume and
for differential replay against codex-rs.

### Cancellation

- Once cancellation is observed, no new tool call starts.
- Every in-flight call is aborted and produces a bounded error result.
- Aborted results are committed in `toolCallIndex` order like normal results.
- Cancelling during a sequential call does not start pending parallel calls.

### Failure

- A tool returning an error produces a bounded error `ToolResult`; the run
  continues unless the tool's failure mode declares `failTask`.
- Provider panic or task join failure produces a bounded error result for that
  call; other calls in the batch are unaffected.

## Acceptance criteria

- [ ] A batch with two parallel-capable tools shows observable overlap in
      execution and commits both results in call order.
- [ ] A sequential tool in a batch never overlaps any other call.
- [ ] A mixed batch keeps the sequential call exclusive while parallel calls
      may overlap each other.
- [ ] With a concurrency cap of 1, parallel calls do not overlap.
- [ ] Results commit in `toolCallIndex` order even when completion order
      differs.
- [ ] Cancelling mid-batch commits a bounded error result for every in-flight
      call in call order and starts no new calls.
- [ ] Unknown tools produce a bounded error result (unchanged).
- [ ] Transcript ordering for a sequential batch is byte-identical to current
      behavior (differential regression).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Default mode when a tool declares nothing | Sequential | Fail-closed; matches codex-rs default |
| How is parallel allowed? | Opt-in per tool (`executionMode: parallel`) or per tool set (`allowParallel: true`) | Avoids racing mutating tools by accident |
| Result commit order | By `toolCallIndex`, not completion order | Preserves append-only transcript determinism |
| What does cancellation commit? | Bounded error result per in-flight call | Keeps the transcript complete and replayable |
| Concurrency cap | `maxConcurrentCalls` from the owning tool set | Bounded resource use; unbounded when unset |

## Open questions

1. Should `bash`/shell tools opt into parallel execution later? codex-rs
   allows it; piko defers until shell sessions are first-class.

## Reference evidence

- codex-rs `core/src/tools/parallel.rs` — shared/unique RwLock gate and
  per-call task dispatch.
- codex-rs `core/src/tools/registry.rs` — `supports_parallel_tool_calls()`
  resolution.
- codex-rs `core/src/tools/handlers/view_image.rs`,
  `core/src/tools/handlers/tool_search.rs`, `core/src/tools/handlers/shell_command.rs`
  — opt-in overrides to `true`.
- piko `packages/protocol/src/tools.rs` — `ToolExecutionMode`,
  `ToolSetPolicy { allowParallel, maxConcurrentCalls }` (types already present,
  currently unused by the runtime).
