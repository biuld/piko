# Session Entry Model

`piko-session` is the durable source of truth. A session is an ordered tree of
discriminated `SessionTreeEntry` values; it is not an array of LLM messages.

## Invariants

- `message` stores only `PersistableMessage` values.
- Compaction and branch summaries are stored only as `compaction` and
  `branch_summary` entries. They must never be appended as `message` entries.
- Every entry has a stable `id` and `parentId`. Consumers use those identifiers,
  not array offsets, to preserve identity.
- `buildSessionContext()` is an adapter for model input. It may project structural
  entries into context-only `AgentMessage` values, but those projections are not
  persistence records and must not flow back through `appendMessage()`.

## Adding an entry type

Add the variant to `SessionTreeEntry`, update storage validation and tree
rendering, then update every exhaustive consumer. The TUI converter deliberately
uses a `never` assertion so a new variant fails compilation until its visibility
policy is explicit.

