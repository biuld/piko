# V-49: Agent run trajectory

> Feature: [F-36](../features/F-36-agent-run-trajectory.md)
> Design: [D-49](../design/D-49-agent-run-trajectory.md)
> Date: 2026-08-16

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Assembly records are durable and per-agent isolated | `captures_and_replaces_successful_assembly_per_agent` — three assemblies written to the journal as `trajectory.assembly` optional events, keyed by run identity |
| Run list/fetch joins journal facts with trajectory records | `query_lists_and_fetches_runs_from_journal_events` — `execution_started`/`execution_finished` facts + assembly/model-step records replay into a summary (terminal, step count, turn id) and a full run record; the query opens a fresh store from the same directory (survives restart) |
| Model steps capture the actual provider-neutral request | `captures_actual_model_step_before_provider_dispatch` — start record carries request/options/identity; finish record carries duration |
| Trajectory DTOs round-trip | `trajectory_records_round_trip` |
| Observational events never alter the acknowledged projection | Journal decode treats `trajectory.*` as ignorable (existing optional-event contract, schema-v4); session aggregate tests unchanged and passing |
| D-30 prompt debugging removed | No `PromptDebug`/`prompt_debug` identifiers remain in the workspace; protocol command/result, hostd port/dispatch, TUI slash/surface and their tests deleted |
| OTel span export and GenAI content removed; metrics/logs retained | `turn_records_metrics_and_logs_without_span_export` — one turn records `piko.turn.duration_ms`/`piko.model.step.duration_ms` and OTel LogRecords with `run_id`; no span exporter is installed |
| End-to-end turn writes durable trajectory records | `turn_writes_durable_trajectory_records` — a real hostd turn appends `trajectory.assembly` + started/completed `trajectory.tool_call` optional events; a fresh query replays the run (terminal, counts, messages) |
| Approval/denial/run-error system notifications | Approval gateway emits `ApprovalRequested`/`ApprovalResolved`/`ToolDenied` resolved to the active run; turn failure emits `RunError` (run loop) |
| Layering: application never imports `infra`/`adapters` | `application_must_not_depend_on_infra_or_adapters` — the trajectory registry is reached through `TrajectoryRegistryPort` |
| Workspace gates | `cargo fmt --all` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; full `cargo test --workspace` passes (agents config installed; local TCP tests run outside the sandbox) |

## Results

| Test | Result |
|---|---|
| Protocol trajectory serde | pass |
| Hostd assembly capture to journal | pass |
| Hostd trajectory query (list + fetch + restart) | pass |
| llmd model-step capture (start + finish) | pass |
| OTel metrics/logs end-to-end without span export | pass |
| End-to-end trajectory journal + query replay | pass |
| Architecture layering | pass |
| Communication catalog + topology regen | pass |
| Workspace clippy (`-D warnings`) | clean |

## Notes

- The web viewer was smoke-tested against a live hostd: `/` serves the static
  two-column page (HTTP 200) with session list, run selector, per-role track
  timeline, and chronological messages; a real persisted session (290
  messages, context/user/assistant/toolCall/toolResult roles) renders through
  the same endpoints. `/api/trajectory/sessions` returns sessions
  most-recently-modified first, and unknown runs return an explicit 404.
  Browser-level interaction beyond the endpoints is manual.
- Child-run records are emitted by the execution actor when a
  `spawn_agent`/`spawn_agent_detached` call completes; steer notifications are
  emitted on `commit_steering`. Approval/denial/run-error notifications are
  emitted by the hostd approval gateway and turn run loop (see table).
