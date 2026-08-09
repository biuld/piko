# V-06: F-06 tool batch dispatch acceptance evidence

> Date: 2026-08-09
> Fixture: `piko-orchd` unit/integration tests in
> `packages/orchd/src/runtime/execution/tool_batch/tests.rs` and
> `packages/orchd/src/adapters/tools/registry.rs`, plus `piko-llmd` projection
> tests in `packages/llmd/src/executor/prompt_mapping_tests.rs`
> Environment: macOS, `cargo test -p piko-orchd`

## Reproduction

```bash
cargo test -p piko-orchd --lib runtime::execution::tool_batch::tests
cargo test -p piko-orchd --lib adapters::tools::registry::tests
cargo test -p piko-llmd executor::prompt_mapping
```

The execution tests drive a real `ExecutionActor` end-to-end with a
tool-calling gateway and a timing-aware fake provider that records overlap,
ordering, and cancellation behavior.

## Result

All F-06 acceptance criteria pass in the tool-batch, registry-resolution, and
provider prompt-projection suites:

- Two parallel calls overlap (observed max concurrency 2) and commit results
  in call order.
- A sequential call in a mixed batch never overlaps parallel calls; the
  parallel pair still overlaps.
- `maxConcurrentCalls: 1` serializes parallel calls.
- Results commit in `tool_call_index` order even when completion order
  differs.
- Cancelling mid-batch commits a bounded `aborted` error result for every
  in-flight call and starts no new calls (model call count stays at 1).
- Cancelling during a sequential call does not start pending parallel calls;
  never-started calls still receive committed aborted results.
- Unknown tools produce the unchanged bounded error shape.
- All-sequential steps use the provider-valid `Assistant` → all `ToolCall`
  → all `ToolResult` transcript shape.
- Legacy sequential transcripts with interleaved calls and results normalize
  to the same provider-valid shape while retaining assistant reasoning.
- Registry resolution: explicit per-tool mode wins; set-level `allowParallel`
  upgrades an unset mode; unset defaults fail-closed to sequential.

## Invariants

- Batch results commit in `tool_call_index` order, never completion order.
- Every call declared by one assistant step is committed before any result for
  that step, independent of execution mode.
- Sequential calls never overlap any other call; parallel calls overlap only
  within their own group and under the set-level cap.
- Cancellation aborts in-flight calls, starts nothing new, and leaves a
  complete, replayable transcript (every call of the step has a committed
  result).
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
