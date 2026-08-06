# V-30: F-15 prompt assembly debugging

> Feature: [F-15](../features/F-15-observability.md) (prompt-debugging slice)
> Design: [D-30](../design/D-30-prompt-assembly-debugging.md)
> Date: 2026-08-06

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Production assembly captures exact semantic prompt and tool catalog | `captures_and_replaces_successful_assembly_per_agent` |
| World-state then mention resource order | `captures_and_replaces_successful_assembly_per_agent` asserts two ordered resource rows built by the production port |
| Missing snapshot is explicit and observational | `prompt_debug_get_is_explicit_when_snapshot_is_unavailable` |
| Query returns the runner-owned snapshot | `prompt_debug_get_returns_latest_runner_snapshot` |
| Replacement and per-agent isolation | `captures_and_replaces_successful_assembly_per_agent` |
| Stable JSON-lines command shape | `prompt_debug_get_round_trips` |
| Bodies absent from logs/OTel | Capture code contains no tracing or instrument recording; code review verifies only keyed in-memory storage |
| Actual llmd input captured before dispatch | `captures_actual_model_input_before_provider_dispatch` asserts mapped message, options, and execution identity against a local stub |

## Commands

```bash
cargo test -p piko-protocol prompt_debug_get
cargo test -p piko-hostd captures_and_replaces_successful_assembly_per_agent
cargo test -p piko-hostd --test server_jsonl prompt_debug
cargo test -p piko-llmd --test gateway_retry captures_actual_model_input_before_provider_dispatch
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Results

| Test | Result |
|---|---|
| Protocol prompt-debug serde | pass |
| Production prompt-port capture/replacement | pass |
| Host command success and unavailable paths | pass |
| Actual llmd model-input capture | pass |
| Workspace clippy (`-D warnings`) | pass |
| Full workspace suite | pass |

## Notes

- Snapshots are latest-only and process-local. Restart intentionally clears
  them until the next successful production assembly.
- The llmd snapshot is the actual provider-neutral request immediately before
  adapter dispatch. Adapter-private HTTP serialization is not claimed.
- The TUI tolerates the command result but has no prompt-debug panel or slash
  command in this slice; protocol clients may issue `prompt_debug_get`
  directly.
