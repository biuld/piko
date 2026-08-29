# V-15: End-to-end tracing + OTel metrics slice evidence

> Date: 2026-08-02
> Fixture: `piko-hostd` `tests/otel_end_to_end.rs` (in-memory OTel backend
> for traces, metrics, and logs, real hostd turn path) and `piko-llmd`
> `tests/gateway_retry.rs`
> `llm_request_span_records_retry_ttft_usage_and_done_events` (stub HTTP
> server).
> Environment: macOS, `cargo test --workspace`, `cargo clippy --workspace
> --all-targets -- -D warnings`

## Reproduction

```bash
cargo test -p piko-hostd --test otel_end_to_end
cargo test -p piko-llmd --test gateway_retry
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The hostd test installs a test `tracing` subscriber backed by
`opentelemetry_sdk` in-memory exporters for spans, metrics, and logs (the
logs side via the `opentelemetry-appender-tracing` bridge), then runs one
real turn through `HostServer` + `OrchAgentRunRunner` with a scripted gateway
that first issues a `todo_write` tool call and then replies with text. The
llmd test drives a real `LlmdExecutor` (with the default middleware chain)
against a scripted stub HTTP server that returns 503 once then a
usage-carrying SSE stream, and asserts on the exported `llm.request` span.

## Result

- One turn exports the full span tree with correct parentage:
  `turn.run → agent.run → model.step`, with `model.step.commit` and
  `tool.batch → tool.call` as sibling phases under `agent.run`.
- Correlation attributes: `turn.run` carries `session_id`/`run_id`;
  `model.step` carries `model="test-model"`, `provider="test"`; spans carry
  `agent_instance_id`.
- The `llm.request` span (parent `model.step`) records span events
  `llm.retry`, `llm.ttft`, `llm.usage`, `llm.stream_done` and the
  `model`/`provider`/`run_id` attributes; retry/backoff and usage/cost
  behavior (F-02) is visible inside one step.
- Metrics exported: `piko.turn.duration_ms`, `piko.model.step.duration_ms`
  (plus `piko.tool.*`, `piko.model.ttft_ms`, `piko.model.tokens`,
  `piko.model.cost_usd`, `piko.model.retries`,
  `piko.model.streaming_fallbacks` in the same meter).
- Unified OTel logs: tracing events export as LogRecords; at least one
  LogRecord carries `run_id` and a trace context (correlated with a span).
- The file-logging stack (`tracing-appender`, `--log-file`, `--no-log`,
  `PIKO_LOG_FILE`, `json-logs`) was removed — OTel logs is the managed sink,
  with a stderr console layer as the disabled-mode fallback.
- Cross-task nesting works: `agent.run` spans created in the agent actor task
  nest under `turn.run` (created in the hostd turn runner), proving the
  mailbox span-capture mechanism in D-15.
- `cargo test --workspace` passes (including both observability tests);
  `cargo clippy --workspace --all-targets -- -D warnings` is clean.

## Invariants

- Every turn produces exactly one root `turn.run` span; model steps, model-step
  commits, and tool calls nest under the agent run that issued them.
- `model.step` duration ends when the model response has been assembled, before
  durable model-step commit and local tool execution. `model.step.commit`
  measures the atomic persistence phase separately.
- `llm.request` is a child of its `model.step`; retry/fallback/usage/timing
  events land on the `llm.request` span even though the stream is consumed
  by a different layer.
- With observability disabled (`[observability] enabled = false`) a stderr
  console subscriber is installed and no OTLP traffic is produced
  (`tests/logging.rs` passes with `init(config, None)`).

## Notes

- Child `agent.run` nesting under `tool.call` (`spawn_agent`) uses the same
  `run_agent` span-capture mechanism verified here for root runs; a dedicated
  `spawn_agent` trace assertion is deferred to the multi-agent slice and is
  also visible manually in Jaeger with `[observability] enabled = true`.
- File and JSON logging were removed in this slice. With observability
  disabled, the supported fallback is the stderr console layer asserted by
  `tests/logging.rs`.
