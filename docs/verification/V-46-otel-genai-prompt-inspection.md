# V-46: OTel GenAI prompt inspection

> Feature: [F-15](../features/F-15-observability.md)
> Design: [D-46](../design/D-46-otel-genai-prompt-inspection.md)
> Date: 2026-08-14

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Assembly is a child of the agent run | `otel_end_to_end::turn_exports_prompt_assembly_as_agent_child` |
| Ordered assembly provenance excludes prompt bodies | `otel_end_to_end::turn_exports_prompt_assembly_as_agent_child` |
| Opted-in final model input uses OTel GenAI attributes | `gateway_retry::llm_request_span_records_retry_ttft_usage_and_done_events` |
| Content is absent when capture is disabled | same gateway test, second request |
| Oversized attributes are omitted and reported | `genai_telemetry::tests::oversized_attribute_is_omitted` |
| Thinking blocks are never exported | `genai_telemetry::tests::thinking_blocks_are_not_exported` |
| Content setting defaults off and merges independently | `observability_content_capture_defaults_off_and_merges_independently` |

## Reproduction

```bash
cargo test -p piko-hostd --test otel_end_to_end
cargo test -p piko-llmd --test gateway_retry llm_request_span_records_retry_ttft_usage_and_done_events
cargo test -p piko-llmd genai_telemetry
cargo test -p piko-hostd observability_content_capture_defaults_off_and_merges_independently
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Result

The focused OTel tests prove the trace parentage and inspect exported span
attributes using an in-memory exporter. The model-boundary test performs one
request with capture enabled and another with capture disabled, then verifies
that only the opted-in span contains `gen_ai.input.messages`. Prompt bodies do
not appear in the assembly span, and direct OTel attachment keeps sensitive
attributes out of `tracing` console and log fields.

2026-08-14, macOS:

| Suite | Result |
|---|---|
| hostd OTel end-to-end | 1 passed |
| llmd GenAI mapping | 2 passed |
| llmd opted-in/default-off exporter check | 1 passed |
| hostd capture-content configuration | 1 passed |
| TUI settings tests | 4 passed |
| `cargo test --workspace` | passed |
| workspace clippy with `-D warnings` | clean |

## Remaining rollout

- A reference Collector/backend stack and deployment policy are follow-up
  operational work.
- Model-output body capture is not part of this slice; usage, finish, retry,
  and timing telemetry remain available without it.
