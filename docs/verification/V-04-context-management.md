# V-04: Context-management acceptance evidence

> Date: 2026-08-02
> Fixture: orchd unit tests in
> `packages/orchd/src/domain/transcript/{tokens,snapshot,normalize,transcript}.rs`
> and `packages/orchd/src/runtime/execution/tests.rs`; end-to-end through the
> real `AgentRuntime` in
> `packages/orchd/tests/agent_runtime_cases/context.rs` (scripted
> `FauxProvider` gateway + a bloat tool provider registered in the catalog).
> Environment: macOS, `cargo test -p piko-orchd` + `cargo test --workspace`.

## Reproduction

```bash
cargo test -p piko-orchd --lib domain::transcript
cargo test -p piko-orchd --lib runtime::execution::tests
cargo test -p piko-orchd --test agent_runtime oversized_tool_output
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Result

All suites pass: orchd lib 61 tests (18 new F-04 unit tests), orchd agent
runtime 40 integration tests (1 new end-to-end), full workspace green, and
`clippy --workspace --all-targets -- -D warnings` is clean.

Acceptance-criteria evidence:

- **Per-message token accounting** — `total_tokens_tracks_pushes_and_rollback`
  and `estimate_messages_matches_sum_of_parts`: the running total equals the
  sum of per-message estimates and stays consistent across push and rollback.
- **Copy-on-write snapshots** — `clone_shares_storage`,
  `snapshot_is_shared_until_mutation`, `rollback_invalidates_snapshot`:
  repeat `snapshot()` calls return the same allocation (`Arc::ptr_eq`); any
  mutation (push/rollback) invalidates the cache and yields a new snapshot.
- **Normalization/truncation** — `oversized_output_is_truncated_with_marker`,
  `multi_block_budget_is_consumed_in_order`,
  `images_are_preserved_when_text_is_truncated`,
  `small_output_passes_through_unchanged`, `normalization_is_deterministic`:
  oversized tool results project to a head + explicit
  `[Tool output truncated: retained N of M characters ...]` marker; small
  results, image blocks, `details`, error flags, and metadata pass through
  unchanged; the projection is deterministic.
- **Budget preflight accounts the dispatched view** —
  `context_budget_accounts_snapshot_and_reports_context_remaining`: the
  preflight consumes `snapshot.total_tokens()` of the normalized model view
  and over-budget rejection reports `context_remaining` and "compaction
  required".
- **End-to-end** —
  `oversized_tool_output_is_truncated_in_model_view_but_kept_in_committed_transcript`:
  a registered tool returns 200,000 characters; the second model request's
  transcript contains the truncation marker (`retained ... of 200000
  characters`), while the committed transcript (hostd-side
  `ExecutionCommitPort`) retains the full 200,000-character output and the
  run completes normally.

Observed end-to-end sequence (abridged):

```text
request 1: model emits ToolCallChunk { name: "bloat_emit" }
           → tool executed → full ToolResult committed (200k chars)
request 2: transcript contains ToolResult with truncated text +
           "[Tool output truncated: retained … of 200000 characters …]"
commits:   ToolResult message keeps the full payload (details + content)
run:       completes (Succeeded), no ContextBudgetExceeded
```

## Invariants

- Accounting and dispatch share one estimator; the preflight checks the exact
  normalized messages sent to the gateway.
- Truncation exists only in the model view; the committed transcript is
  byte-for-byte unchanged, so hostd stays authoritative for durable content.
- Over-budget after normalization still fails closed with
  `ContextBudgetExceeded` and "compaction required" (F-05 owns trimming).
