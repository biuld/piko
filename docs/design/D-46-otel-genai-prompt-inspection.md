# D-46: OTel GenAI prompt inspection

> Status: superseded
> Superseded by: [F-36/D-49](../features/F-36-agent-run-trajectory.md)
> Implements: [F-15](../features/F-15-observability.md) (OTel prompt-inspection slice)
> Verification: [V-46](../verification/V-46-otel-genai-prompt-inspection.md)

## Goal

Use OpenTelemetry as piko's vendor-neutral telemetry substrate for a
LangSmith-like causal view of prompt assembly, model calls, tool calls, and
multi-agent execution, while preserving hostd authority and making sensitive
content export explicit.

## Assessment

OTel is a good fit for the trace graph and correlation layer, but it is not a
complete LangSmith replacement. OTLP carries spans, events, logs, and metrics;
an external backend must provide storage, search, large-content presentation,
retention, evaluation, and comparison UX.

The current OTel GenAI semantic conventions define model input messages,
system instructions, tool definitions, model output messages, agent spans, and
tool spans. They are still evolving, so piko should isolate semantic-convention
mapping in llmd and version its custom assembly schema.

## Trace model

```text
turn.run
  agent.run / invoke_agent
    piko.prompt.assemble
      ordered block and tool-source metadata attributes
    model.step
      llm.request / gen_ai client operation
      tool.batch
        tool.call / execute_tool
```

`piko.prompt.assemble` is an internal piko span because OTel currently models
the model/agent operation but not piko's host-owned assembly stages. It records
run identity, assembly version, source/cache digests, block count, resource
count, tool count, and ordered block metadata. It does not record bodies by
default.

The actual llmd boundary remains the fidelity point for model input. When
content export is opted in, llmd maps the final provider-neutral request to the
`gen_ai.system_instructions`, `gen_ai.input.messages`, and
`gen_ai.tool.definitions`. Adapter-private HTTP payloads remain out of scope.

Ordered assembly metadata is encoded in the versioned `piko.prompt.blocks` and
`piko.prompt.tool_sources` span attributes. Each block entry includes order,
identity, authority, trust, source, cache scope, digest, and character count,
but never the block body. Each attribute is capped at 64 KiB;
`piko.prompt.metadata_dropped = true` reports an omitted oversized value.

## Data policy

`[observability].capture-content` is separate from
`[observability].enabled` and defaults to `false`. Enabling telemetry never
implicitly exports prompt bodies. This stage exports only the final model
input: system instructions, input messages, and tool definitions. It omits
thinking blocks. Each text part is redacted, bounded to 64 KiB, and each
complete GenAI attribute is bounded to 256 KiB; oversized attributes are
omitted and reported with `piko.gen_ai.content_dropped = true`.

Content attributes are attached directly to the active OTel span instead of
declared as `tracing` fields. This prevents opted-in prompt bodies from also
appearing in the stderr console or unified OTel log records.

The collector should be treated as a second policy boundary: deployments may
apply allow-listing, redaction, encryption, retention, and access control there.
Application-side opt-in is still required because collector policy can be
misconfigured or bypassed.

## Ownership and failure behavior

- hostd remains authoritative for local user-visible diagnostics and durable
  session state;
- orchd and llmd emit instrumentation but do not initialize exporters;
- sampling, exporter failure, or backend outage cannot change prompt assembly
  or model execution;
- piko does not query the telemetry backend to answer protocol commands;
- the local `/prompt-debug` snapshot can coexist as an unsampled, latest-only
  diagnostic until the external UX is sufficient.

## Rollout

1. **Implemented:** add a safe `piko.prompt.assemble` metadata span and adopt
   GenAI names for the model client span where applicable.
2. **Removed with F-36:** the content-export setting and GenAI content
   attributes were deleted; the durable trajectory is the only content
   capture. OTel metrics and unified logs remain.
3. **Follow-up:** provide a reference OTel Collector plus backend configuration and verify
   trace rendering, redaction, and retention.
4. **Follow-up:** add model-output content if needed, with streaming buffering
   and the same explicit policy boundary.
5. **Follow-up:** add evaluation/dataset concepts only as piko product features; do not encode
   them as an accidental dependency on one observability vendor.

## Package impact

- hostd owns the setting, exporter initialization, assembly span, and local
  prompt-debug snapshot;
- orchd preserves the active agent trace context while calling the host-owned
  assembly port;
- llmd owns provider-neutral request-to-GenAI mapping and dispatch-boundary
  content capture;
- protocol carries the exact run identity required by both local diagnostics
  and trace correlation;
- TUI exposes the content switch as a sensitive, restart-class setting.

## Rejected alternatives

- **Use OTel as hostd state storage.** Sampling and exporter availability make
  it unsuitable for product authority.
- **Export full bodies whenever tracing is enabled.** This violates least
  surprise and can leak workspace content or credentials.
- **Record only the pre-assembly resources.** This can drift from the actual
  model input; the llmd dispatch boundary remains necessary.
