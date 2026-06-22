# Reconciliation Boundary

The orchestrator guarantees ordered runtime events and a final model transcript.
It does not reconcile the TUI or assign durable session entry IDs.

Agent actors own task-local message identity during a run. Tool calls retain
their `toolCallId` through execution and transcript commit, allowing downstream
consumers to merge results without positional matching. The Host persists the
result and publishes the durable session snapshot; the TUI then switches to
session entry identity.

Keeping this boundary explicit prevents storage-only variants such as compaction
and branch summaries from entering actor state or protocol switches.

