# ADR-025: Authoritative agent lifecycle boundaries

> Status: accepted
> Date: 2026-08-29
> Superseded in part by: [ADR-027](ADR-027-agent-work-lifecycle.md) for the core-domain hierarchy and lifecycle authority

## Context

piko has four useful runtime scopes—Turn, Run, Execution, and ModelStep—but
the durable journal currently records only part of their relation. Assistant
messages and tool declarations are separate commits, and model-step trajectory
records are optional. Realtime thought completion can therefore be lost or
outlive the actual model response. The existing `run_id` and `execution_id`
fields also usually contain the same value even though they represent
different boundaries.

## Historical decision

This ADR originally used the following authority hierarchy:

```text
Turn → Run → Execution → ModelStep → Thought / ToolCall
```

`hostd` remains authoritative for all durable user-visible state. Turn state
is host-owned; Run and Execution identity are persisted by the existing
execution start/finish facts; ModelStep becomes a required journal relation.

One ModelStep commit atomically appends the assistant message and ordered tool
declaration messages, then appends a required `ModelStepCommittedV1` relation
event. The relation references message IDs and timing/outcome metadata; it
does not duplicate message content. Tool results remain later message facts.

Orchd advances its private transcript only after the host acknowledges the
atomic ModelStep commit. Realtime deltas remain lossy. Reliable observation
publishes the ModelStep boundary after persistence, and clients reconcile
committed thought durations from the journal message.

Run and Execution IDs are carried separately end-to-end. Root runs use their
Turn/operation ID as `run_id`; the concrete Execution ID remains independently
derived from the request and agent. Existing journals remain readable.

ADR-027 subsequently makes Session, AgentInstance, and ModelStep the invariant
grains, with AgentInput as stimulus and root-work identity. Turn, Run, and
Execution leftover identities are removed by F-51. The atomic ModelStep
commit requirement in this ADR remains accepted.

## Consequences

- Recovery can distinguish a completed model response from unresolved tool
  execution without replaying the model.
- A client cannot mistake another agent's activity for completion of its own
  thought.
- The journal gains one required event type and one atomic write path; this is
  a schema-reader change but not a storage-generation migration.
- Model-step relations are queryable from the canonical aggregate, while
  trajectory remains useful for optional diagnostics only.
- A legacy message committed without a ModelStep relation can still be read;
  new model responses must use the relation.
- Queued user interaction is recovered from AgentInput facts; it does not
  require an independent durable Turn lifecycle.
