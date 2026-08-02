# V-05: Compaction acceptance evidence

> Date: 2026-08-02
> Fixture: hostd unit tests in
> `packages/hostd/src/domain/compaction/{trigger,tree,tokens,file_ops}.rs`;
> hostd integration through the real `HostServer` in
> `packages/hostd/tests/compaction_reconcile.rs` +
> `packages/hostd/tests/compaction_reconcile_cases/` (scripted `LlmGateway`
> variants + `JsonlSessionRepository`); orchd unit tests in
> `packages/orchd/src/adapters/tools/context_tools_provider.rs` and
> `packages/orchd/src/domain/transcript/transcript.rs`; orchd end-to-end
> through the real `AgentRuntime` in
> `packages/orchd/tests/agent_runtime_cases/context/{mod,budget_tools,truncation_cap}.rs`
> (scripted `FauxProvider` gateway + bloat tool provider registered in the
> catalog).
> Environment: macOS, `cargo test --workspace` (llmd socket-binding
> integration tests are sandbox-blocked, unrelated to F-05) +
> `cargo clippy --workspace --all-targets -- -D warnings`.

## Reproduction

```bash
cargo test -p piko-hostd --lib domain::compaction
cargo test -p piko-hostd --test compaction_reconcile
cargo test -p piko-orchd --lib domain::transcript adapters::tools::context_tools_provider
cargo test -p piko-orchd --test agent_runtime context_budget transcript_max
cargo test -p piko-protocol --lib command
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Result

All suites pass: hostd compaction unit tests green (4 new), hostd
compaction-reconcile integration green (5 tests, 3 new F-05), orchd context
tools green (4 new unit + 2 new end-to-end), transcript reset green (2 new
unit), protocol wire-compat green (1 new), full workspace green aside from
the sandbox-blocked llmd socket tests, `cargo fmt --all` clean, and
`clippy --workspace --all-targets -- -D warnings` clean.

Acceptance-criteria evidence:

- **Budget-window trigger with hysteresis** —
  `trigger_respects_enabled_and_high_waterline`,
  `first_window_triggers_once_past_waterline`,
  `rearm_baseline_requires_minimum_growth`: disabled always holds; below the
  high waterline (`window − reserve`) holds; the first over-waterline branch
  triggers; after a recorded rearm baseline, only growth ≥ `min_growth_tokens`
  re-triggers, otherwise `Hold(InsufficientGrowth)`.
- **Pending guard (single rewrite under racing compacts)** —
  `concurrent_compacts_produce_a_single_rewrite`: two `session.compact`
  commands racing through a blocking summarizer produce exactly one
  `SessionReconciled`; the second command is skipped without error.
- **New-context-window inline compact** —
  `new_context_window_mode_rewrites_without_calling_the_model`: after two
  turns, `SessionCompact { mode: NewContextWindow }` appends a checkpoint
  (`trigger = new_context_window`, `windowNumber = 1`, tokens before/after
  recorded), anchors `first_kept_entry_id` at the latest user message, emits
  `SessionReconciled`, and never calls the model (a `PanicGateway` proves the
  no-summarization invariant).
- **Summarizer model override + fallback** —
  `summarizer_failure_falls_back_to_default_model`: with
  `[compaction] summarizer-model` configured, the summary call runs against
  the override; on failure hostd retries once with the default model and the
  compaction lands (recorded call order `["summarizer-model", "default"]`).
- **`get_context_remaining`** —
  `get_context_remaining_reports_budget_basis` and
  `context_budget_tools_report_remaining_and_request_fresh_window`: the tool
  returns the threaded F-04 `context_remaining` (`tokens_left`), `null` when
  unknown, and a running agent receives a real estimate in its tool result.
- **`new_context_window`** —
  `new_context_window_fails_closed_without_callback`,
  `new_context_window_invokes_host_callback_once`,
  `new_context_window_surfaces_callback_failure`, and the end-to-end run: the
  tool fails closed without a wired host, invokes the host callback once with
  the root identity, surfaces host failures as `compact_failed`, and the
  running execution trims to the latest user message so the final request
  still begins from the user instruction.
- **Truncation-cap settings wiring** —
  `transcript_max_tool_output_tokens_reaches_the_model_view`: with
  `OrchdConfig.transcript_max_tool_output_tokens = 100`, an oversized tool
  result is truncated at ~300 characters in the model view instead of the
  24k default, while the committed transcript keeps the full output.
- **Wire compatibility** — `session_compact_without_mode_defaults_to_summarize`:
  old clients that omit `mode` parse as `Summarize`; the explicit
  `new-context-window` mode round-trips.

## Invariants

- The trigger, the rearm baseline, and the budget tools all use the F-04
  estimator; hostd and orchd cannot diverge on what "tokens" means.
- Compaction rewrites only the root shard, always through
  `compact_session_if_needed`, and only ever appends a checkpoint entry —
  the append-only JSONL transcript and `SessionReconciled` rebuild are
  unchanged in shape.
- `new_context_window` is a request, not an authority: the durable rewrite
  stays host-owned (callback), and orchd only trims its transient in-run
  transcript to keep the current execution aligned.
