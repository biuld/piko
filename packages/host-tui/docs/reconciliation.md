# Timeline Reconciliation

The timeline uses two identity domains and never reconciles messages by array
position.

## Live phase

Runtime messages use orchestrator message IDs; tools use `toolEntityId` or
`toolCallId`. `message_start/update/end` and tool events update the projection by
those identifiers. `message_end` is authoritative for live message content.

At `turn_finished`, the Message transcript is used only to refresh tool results
by `toolCallId` when no durable snapshot is available. It must not change a
message's role, kind, content, or identity based on its index.

## Commit phase

For normal Host runs, `turn_finished.entries` contains the persisted current
branch. `entriesToTranscript()` exhaustively maps visible entry variants and the
timeline is rebuilt with session entry IDs. Compaction and branch summaries are
therefore structural items, not synthetic messages.

This full-snapshot replacement is intentional: it atomically switches from
runtime identity to durable identity and produces the same projection as resume.
Metadata entries have an explicit non-visible policy in the converter.

## Extension rule

Adding a `SessionTreeEntry` variant must update `entriesToTranscript()`. The
`assertNever` default makes omission a TypeScript error. Persistable message roles
and canonical protocol message roles are also classified with exhaustive
switches. New renderable entities must define a stable ID policy before they are
added to `TimelineItemKind`.
