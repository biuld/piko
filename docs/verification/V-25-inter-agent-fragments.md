# V-25: F-20 inter-agent completion fragments

> Date: 2026-08-04
> Fixture: protocol completion format unit tests
> (`agent_completion::tests`), orchd prepare_execution chain
> unit test (`inter_agent_completions_chain_after_world_state_before_input`),
> agent runtime multi-agent tests
> (`parent_next_run_injects_unread_completion_before_input`,
> `consumed_inbox_skips_completion_injection`), full
> `piko-orchd --test agent_runtime` suite
> Environment: macOS (arm64), `cargo test -p piko-protocol --lib`,
> `cargo test -p piko-orchd --lib execution::tests`,
> `cargo test -p piko-orchd --test agent_runtime`,
> `cargo clippy -p piko-protocol -p piko-orchd --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-protocol --lib agent_completion
cargo test -p piko-orchd --lib inter_agent_completions_chain
cargo test -p piko-orchd --test agent_runtime parent_next_run_injects
cargo test -p piko-orchd --test agent_runtime consumed_inbox_skips
cargo test -p piko-orchd --test agent_runtime
cargo clippy -p piko-protocol -p piko-orchd --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-20 acceptance criteria pass:

- **Content contract**: `completion_content_lists_facts_in_fixed_order` pins
  the fixed-key body (`source_agent_instance_id`, `report_id`, `outcome`,
  optional `summary`) and the stable message id
  `agent.completion/<report_id>`;
  `failed_outcome_prefers_error_text_over_summary` and
  `summary_truncates_to_max_chars` cover failure text and the 4_000-char
  bound; `is_agent_completion_message_matches_source_identity` pins the
  source-kind/locator identity used for idempotent selection.
- **Durable chain**: `inter_agent_completions_chain_after_world_state_before_input`
  proves prepare_execution builds head → world-state → completion → input
  and that the ExecutionActor transcript mirrors that order.
- **Parent run injection**: after a detached child report lands in the root
  inbox, the parent's next `run_agent` commits exactly one Context message
  with `source.kind == agent.completion`, names the report, and anchors the
  parent user input on that completion id. The inbox item remains unconsumed.
- **Idempotency**: a second parent run does not commit another
  `agent.completion/*` message (transcript already carries the Context).
- **Collect-first path**: after `collect_agent_reports` marks the report
  consumed, the next parent run injects no completion fragment.
- **Regression**: full `agent_runtime` suite (44 tests) green; clippy
  `-D warnings` clean for protocol + orchd.

## Invariants

- Completions never consume inbox items or start a parent turn.
- Message ids are deterministic per `report_id` so durable recommit is
  idempotent at the host message store.
- Selection skips transcripts that already contain the matching Context
  source and skips consumed inbox items.
- Attached spawn continues to surface reports only as tool results (no
  inbox delivery, therefore no inbox-based fragment on that path).
