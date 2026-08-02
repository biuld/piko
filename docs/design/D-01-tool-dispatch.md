# D-01: Parallel tool batch dispatch

> Status: implemented
> Implements: [F-06](../features/F-06-tool-system.md)

## Goal

Deliver the F-06 vertical slice: model-step tool calls execute as a batch
where parallel-capable calls overlap, sequential calls are exclusive,
cancellation aborts in-flight calls, and the transcript stays deterministic.

## Constraints and non-goals

- Transcripts are append-only and must be deterministic per run; result commit
  order is by `tool_call_index`, never completion order.
- Existing all-sequential batches must remain byte-identical to today (no
  regression in durable message shapes).
- No cross-batch scheduling, no retry policy, no provider-level truncation.
- Approval and routing behavior is unchanged; this design only changes
  concurrency and commit grouping.

## Proposed design

### 1. Effective execution mode resolution (registry)

`CatalogRoute` gains `max_concurrent_calls: Option<u32>`. Catalog building
threads the owning `ToolSetPolicy` into projection:

- `project_tool_def` first applies the existing per-tool policy
  `executionMode` override.
- If the result is still `None` and the owning set has
  `allowParallel: Some(true)`, the effective mode becomes `Parallel`.
- Otherwise the effective mode is `Sequential` (fail-closed default).
- `max_concurrent_calls` is copied from the owning set policy into the route
  so the runtime can enforce a batch cap without re-reading tool sets.

Resolution order matches the PRD: explicit tool/per-tool-policy mode wins;
`allowParallel` only upgrades an unset mode; unset defaults to sequential.

### 2. Batch dispatch (ExecutionActor)

`execute_and_commit_tools` becomes group-based:

```text
for each group in calls grouped by (consecutive) effective mode:
  if group is sequential (single call):
    commit ToolCall message
    execute exclusively (unchanged path)
    commit ToolResult message
  else:  # parallel group
    commit every ToolCall message in index order
    run all calls concurrently under a Semaphore(capacity = min caps, ≥ 1)
      where each future selects on run cancellation
    commit every ToolResult message in index order
```

Grouping consecutive parallel calls gives the shared-reader semantics of
codex-rs `parallel.rs` with simpler scheduling: parallel calls in a group
overlap, a sequential call never overlaps anything, and execution order across
groups follows call order.

### 3. Cancellation and complete transcripts

Each parallel call's future is:

```text
tokio::select! {
  biased;
  _ = cancel.cancelled() => aborted result,
  record = registry.execute_tool(...) => record.result,
}
```

On cancellation the future returns a bounded error result with
`code = "aborted"`. After the batch, the runtime commits results for **every**
call of the step — executed, aborted, or never started — so an assistant
message never leaves dangling tool calls in the durable transcript.

### 4. Sequential regression path

When every call in a step is sequential, the code path is the current per-call
loop unchanged (`ToolCall` → execute → `ToolResult`, repeated). Only steps
containing parallel calls use the batched shape
(assistant → all `ToolCall` → all `ToolResult`).

## Package impact

| Package | Change |
|---|---|
| `piko-orchd` | `CatalogRoute` + `max_concurrent_calls`; mode projection from set policy; group-based dispatch in `ExecutionActor`; cancellation selection; complete-transcript commit |
| `piko-protocol` | None (types already present: `ToolExecutionMode`, `ToolSetPolicy`) |
| `piko-hostd` | None |
| `piko-llmd` | None |
| `piko-sandbox` | None |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Cancellation mid-batch aborts in-flight tasks and commits bounded error
  results in index order; pending calls are not started.
- A provider panic or task join failure yields a bounded error result for that
  call; other calls are unaffected.
- `max_concurrent_calls: 0` is treated as 1 to avoid a deadlock; values above
  the group size are capped to the group size.

## Verification

- Unit/integration tests with a timing-aware fake provider:
  - two parallel calls overlap (observed concurrency), results in call order;
  - sequential call in a mixed batch overlaps nothing;
  - cap of 1 serializes parallel calls;
  - completion order differs from commit order;
  - cancellation mid-batch commits aborted results for all calls;
  - all-sequential step transcript stays byte-identical.
- Differential reference: codex-rs `core/src/tools/parallel.rs` gate semantics.
- `cargo test -p piko-orchd`, `cargo clippy --workspace --all-targets -- -D warnings`.

## Alternatives considered

- Shared/unique `RwLock` gate per batch exactly like codex-rs: more faithful,
  but grouping is simpler to reason about and yields the same observable
  guarantees with deterministic group order.
- Commit results as they complete: rejected, breaks append-only determinism.
- No default mode (treat `None` as parallel): rejected, would race mutating
  tools by default; codex-rs defaults to non-parallel.

## Rollout

1. Registry: route cap + mode projection (pure, unit-tested).
2. Actor: group dispatch + cancellation + complete transcript (tests).
3. Provider: mark `read` as `Parallel` opt-in (single-line, differential-safe).
