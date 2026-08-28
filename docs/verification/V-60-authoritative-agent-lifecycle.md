# V-60: Authoritative agent lifecycle boundaries

> Feature: [F-48](../features/F-48-authoritative-agent-lifecycle.md)
> Design: [D-65](../design/D-65-authoritative-agent-lifecycle.md)
> Date: 2026-08-29

## Scope under test

The vertical slice verifies the durable hierarchy:

```text
Turn → Run → Execution → ModelStep → Thought / ToolCall
```

## Reproduction

```text
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Focused cases:

- `piko-session-store::model_step_commit_replays_as_an_atomic_authoritative_relation`
- `piko-session-store::model_step_boundary_rejects_messages_from_an_earlier_revision`
- `piko-hostd::recovery_completes_declared_tool_calls_without_rerunning_the_model_step`
- `piko-hostd::turn_writes_durable_trajectory_records`
- `piko-client-core::timeline::reliable_model_step_boundary_closes_thought_without_message_end`
- `piko-orchd::runtime::execution::tool_batch::all_sequential_step_commits_all_calls_before_results`
- `piko-orchd::runtime::execution::tool_batch::stream_error_commits_failed_model_step_before_failed_terminal`
- `piko-orchd::runtime::execution::tool_batch::cancelled_finish_commits_cancelled_model_step_before_cancelled_terminal`

## Results

- `cargo test --workspace`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- The real hostd → orchd → journal path persists two model-step relations,
  publishes two reliable boundaries, and keeps the logical Run ID distinct
  from the concrete Execution ID.
- Replay exposes ordered ModelSteps from required journal facts; trajectory
  remains optional and observational.
- Recovery appends deterministic aborted ToolResult messages for declared calls
  without results, then appends the abort marker and terminal fact in one
  logical journal commit.
- Existing schema-v4 journals remain readable. `SCHEMA_VERSION` stays `4`;
  the event reader capability advances to `READER_VERSION = 2` for the new
  required event.

## Invariants

- A ModelStep is acknowledged only after its assistant message and ordered
  ToolCall declarations are atomically durable.
- The reducer rejects a boundary that attempts to adopt messages from an
  earlier journal revision, and an idempotent retry must match the stored
  message bodies as well as the boundary metadata.
- Tool execution starts only after that acknowledgement; failed persistence
  cannot advance the private runtime transcript.
- Stream errors, cancelled finishes, and EOF without a terminal frame remain
  failed/cancelled step outcomes rather than being promoted to success.
- A reliable ModelStep boundary closes a matching realtime thought draft by
  assistant message ID and cannot close another agent's draft.
- A crash after the step boundary cannot cause the model request to run again;
  unresolved calls are represented as interrupted results during recovery.
