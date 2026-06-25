# Transcript Protocol Boundary

The orchestrator protocol describes model execution, not session persistence.

- `Message[]` is task/model transcript data.
- Runtime message events carry stable IDs for streaming projection.
- Tool lifecycle events carry `toolEntityId`, `toolCallId`, and parent ordering
  metadata.
- Session entries, compaction records, branch summaries, and UI reconciliation
  snapshots are Host concerns and do not belong in this protocol.

Consumers must not infer durable identity from a `Message[]` index. If a future
cross-process commit protocol is needed, introduce an explicit ID-bearing commit
entity rather than overloading `Message`.

