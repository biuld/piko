# Timeline Ordering Contract Design

## Status

Implemented contract. The live projection is the rendering authority; the
persisted transcript remains the durable authority used to rebuild it on resume.

This document extends `docs/runtime-streaming-redesign.md`. That redesign defines
the structured streaming payload; this document defines identity, ordering, and
projection rules so every consumer produces the same timeline during streaming
and after completion.

## Problem

The current timeline has two authorities:

1. During a run, TUI reducers append and update items in Host event arrival order.
2. At `turn_finished`, `reconcileTranscript` rebuilds the timeline from the
   canonical transcript and may produce a different order.

The difference is visible while assistant content grows, tools execute in
parallel, and multi-step turns produce additional assistant messages. The TUI
also matches messages heuristically and renders its item array by index. A final
reconciliation can therefore move content between component positions, change
layout heights, and make the timeline appear out of order.

The missing abstraction is a protocol-level ordering contract. TUI must project
an ordered event stream; it must not infer causal relationships from roles,
text, or event arrival timing.

## Goals

- One ordering authority from Orchestrator through Host, TUI, RPC, and print.
- Stable identity for every message and tool invocation.
- Identical logical order during streaming and after completion.
- Deterministic order for parallel tool calls independent of completion time.
- Streaming updates are idempotent upserts, not new timeline insertions.
- Final transcript commit validates and completes live state without reordering it.
- Preserve pi-compatible persisted session messages.

## Non-goals

- Do not encode visual styling, collapse state, or scroll state in the protocol.
- Do not expose provider-specific events directly to TUI.
- Do not serialize TUI timeline items into session JSONL.
- Do not require sequential tool execution.
- Do not redesign the Orchestrator actor/event-store architecture.

## Ownership

| Layer | Responsibility |
|---|---|
| Model caller | Preserve provider block order and update one stable runtime message. |
| Orchestrator | Assign stable IDs and task-local ordering metadata; emit ordered lifecycle events. |
| `orch-protocol` | Define identity, ordering, and parent/position fields plus invariants. |
| Host runtime | Filter by requested agent/run and project events without changing order. |
| TUI state | Apply deterministic upserts into a normalized timeline projection. |
| Renderer | Render projected order and preserve component identity by item ID. |
| Session persistence | Persist canonical pi-compatible messages; not the live UI projection. |

## Required invariants

These are normative requirements.

1. `eventSeq` is strictly increasing within one `runId`.
2. A lifecycle entity keeps one ID from start through end.
3. Producers emit `message_start` before update/end, but consumers must recover
   when observation begins at update/end by creating the same entity as an upsert.
4. A message identity is inserted exactly once. Every lifecycle event is an
   idempotent full-state upsert for that identity.
5. `contentIndex` is the provider-declared position inside the assistant message
   and never changes during that message lifecycle.
6. A tool item is positioned by its parent assistant message and
   `toolCallIndex`, not by tool start/end arrival time.
7. Parallel tool completion cannot alter tool display order.
8. The final transcript commit cannot move a live item whose stable identity is
   known.
9. Duplicate events with the same identity and sequence are harmless.
10. Consumers reject or diagnose sequence regressions; they do not silently
    reorder events by timestamps.

## Protocol design

### Common ordering metadata

Add these types to `packages/orch-protocol/src/runtime-stream.ts`:

```ts
export interface RuntimeOrder {
  /** Strictly increasing within runId. */
  eventSeq: number;

  /** Zero-based model step within the run. */
  turnIndex: number;

  /** Stable logical position of this message within the run. */
  messageIndex?: number;
}

export interface RuntimeToolOrder {
  /** Assistant message containing the corresponding toolCall block. */
  parentMessageId: string;

  /** Position of the toolCall block in parent assistant content. */
  contentIndex: number;

  /** Dense position among tool calls in that assistant message. */
  toolCallIndex: number;
}
```

`eventSeq` orders event application. `messageIndex`, `parentMessageId`, and the
block/tool indices order the resulting projection. They solve different
problems and must not be conflated.

### Host runtime events

Extend the existing `RuntimeEventBase`:

```ts
export interface RuntimeEventBase extends RuntimeOrder {
  runId: string;
  agentId: string;
}
```

Extend tool lifecycle variants with `RuntimeToolOrder`:

```ts
type RuntimeToolEventBase = RuntimeEventBase & RuntimeToolOrder & {
  toolCallId: string;
  toolName: string;
};
```

Message lifecycle events continue carrying the full partial `RuntimeMessage`.
The message ID is the entity identity. Do not introduce a second message ID on
the event envelope.

### Stable IDs

Use deterministic, run-scoped IDs assigned before emitting `message_start`:

```text
message:             assistant-<taskId>-step_<stepIndex>
provider toolCallId: retained unchanged as opaque provider correlation data
tool entity:         <messageId>:tool:<toolCallIndex>
```

Provider `toolCallId` is retained unchanged for model-protocol correlation.
`toolEntityId` is always generated internally and is the identity used by event
state, approvals, and the live timeline. The two IDs must never be conflated.

Runtime-to-persisted conversion must retain the runtime message ID in memory
until the turn is committed. The pi-compatible session format does not need to
gain this field unless pi already supports it.

## Timeline projection model

Replace append-oriented timeline state with a normalized projection:

```ts
interface TimelineProjection {
  orderedIds: string[];
  itemsById: Map<string, TimelineItem>;
  lastAppliedSeqByRun: Map<string, number>;
}
```

`timeline.items` is a temporary compatibility view and must always be
materialized from `orderedIds` plus `itemsById`; reducers must not maintain a
second independent ordering authority.

If serializable state is preferred, use `Record<string, TimelineItem>` instead
of `Map`. The behavioral contract is the same.

### Projection keys

```text
user/assistant/custom message: msg:<messageId>
tool execution/result:         tool:<toolEntityId>
approval:                      approval:<approvalId>
```

### Projection order

The display order is lexicographic over a structured logical key, not over event
arrival time:

```ts
type TimelineOrderKey =
  | [messageIndex: number, kind: 0]
  | [parentMessageIndex: number, kind: 1, toolCallIndex: number];
```

- `kind: 0` is the message itself.
- `kind: 1` is a tool belonging to that assistant message.
- The next assistant message has a larger `messageIndex`, so it follows all
  tools belonging to the previous assistant step.

Approval UI attaches to the tool key and does not become an independently
ordered transcript entry unless product requirements explicitly demand it.

### Reducer behavior

`message_start`:

- Assert sequence monotonicity.
- Insert `msg:<message.id>` once using `messageIndex`.
- Store the complete partial message.

`message_update`:

- Diagnose a missing start and upsert the item if it does not exist.
- Replace the partial message payload in `itemsById`.
- Do not modify `orderedIds`.

`message_end`:

- Update or create the same item.
- Mark it non-streaming.
- Do not modify `orderedIds`.

`tool_execution_start`:

- Insert `tool:<toolEntityId>` once using parent message position and
  `toolCallIndex`.
- If it arrives before the parent, render it provisionally at the end, retain
  the pending parent relation, and move the same keyed entity when the parent
  arrives. Approval must never make a tool known but invisible.

`tool_execution_update` / `tool_execution_end`:

- Update or create the same tool item.
- Never change its order key.

Final commit:

- Validate that all canonical messages/tool results are represented.
- Fill final content/status/usage fields.
- Insert only genuinely missing historical entities.
- Never reorder known live entities.
- Emit diagnostics for missing IDs, duplicate IDs, or order disagreement.

## Event production

### Orchestrator

Assign `messageIndex` and `eventSeq` in the task-scoped agent worker. Do not use
wall-clock time. Actor mailbox/event-store serialization is the authority for
incrementing `eventSeq`.

When extracting tool calls from the assistant message, enumerate them in
content order and attach:

```ts
{
  parentMessageId: assistantRuntimeMessage.id,
  contentIndex,
  toolCallIndex
}
```

The metadata must be carried through ToolRegistry start/update/end events.
Parallel execution remains unchanged. `Promise.all` completion order must not
affect projection order.

### Host

Host filters events to the requested `agentId` and `runId`, then maps names and
payloads. It must preserve all ordering metadata verbatim.

Host must not synthesize ordering from callback arrival order. It may detect and
fail fast on an `eventSeq` regression in development/test builds.

## Renderer contract

The renderer receives `orderedIds` plus item lookup accessors.

Requirements:

- Component identity is keyed by timeline item ID.
- Updating one streaming item does not recreate completed items.
- Reordering, when explicitly requested, moves an existing component identity
  rather than changing the semantic item at an array index.
- Markdown block identity is `(messageId, contentIndex)`.
- OpenTUI `streaming=true` remains enabled until `message_end`, then changes to
  false on the same MarkdownRenderable.

Solid's `<Index>` is suitable only for a list whose semantic identity is the
position. It must not be the identity mechanism for the top-level timeline.
Use an ID-keyed representation or a keyed wrapper whose object identity remains
stable for the lifetime of the timeline item.

## Reconciliation changes

Remove these matching fallbacks from the live path:

```text
first streaming assistant
assistant with equal text
first unused assistant
```

They may remain temporarily for loading legacy sessions, isolated behind a
`reconcileLegacyTranscript` function. They must not run after a live turn that
has protocol IDs.

Split reconciliation into:

```ts
validateCommittedTranscript(projection, messages): Diagnostic[]
finalizeProjection(projection, messagesByRuntimeId): TimelineProjection
reconcileLegacyTranscript(messages): TimelineProjection
```

This prevents compatibility heuristics from changing live ordering.

## Migration plan

### Phase 1: Protocol metadata

1. Add `RuntimeOrder` and `RuntimeToolOrder`.
2. Add ordering fields to `HostRuntimeEvent` and relevant `HostEvent` variants.
3. Update protocol exports.
4. Add compile-time exhaustive tests for all event variants.

Compatibility: fields may be optional for one migration phase, but new
Orchestrator events must always populate them. Remove optionality in Phase 4.

### Phase 2: Event producers and projection

1. Generate task-local `eventSeq` and `messageIndex` in AgentActor/step runner.
2. Carry parent/index metadata through tool execution events.
3. Preserve metadata in StateActor-to-Host projection and Host runtime mapping.
4. Introduce `TimelineProjection` and its pure reducer.
5. Keep the old TUI reducers behind a temporary compatibility adapter.

### Phase 3: TUI renderer

1. Render `orderedIds` with stable ID-keyed item identities.
2. Keep assistant Markdown instances stable across updates.
3. Replace live `reconcileTranscript` with validation/finalization.
4. Retain legacy reconciliation only for resumed historical sessions.
5. Remove `streamingItemId`; the active item is identified by lifecycle event ID.

### Phase 4: Cleanup

1. Remove legacy `token`, `thinking`, `assistant_delta`, and `thinking_delta`
   paths after all consumers use structured lifecycle events.
2. Make ordering fields required.
3. Delete role/text-based matching from live code.
4. Document protocol versioning and compatibility policy.

## Primary files to change

| File | Change |
|---|---|
| `packages/orch-protocol/src/runtime-stream.ts` | Ordering metadata and runtime event contract. |
| `packages/orch-protocol/src/events.ts` | Preserve ordering in Host-visible events. |
| `packages/orchestrator/src/model/model-caller.ts` | Stable runtime message lifecycle IDs. |
| `packages/orchestrator/src/actors/agent/step-runner.ts` | Allocate message positions and emit ordered events. |
| `packages/orchestrator/src/actors/agent/tool-executor.ts` | Carry parent/tool-call positions through parallel execution. |
| `packages/orchestrator/src/actors/state/types.ts` | Store ordering fields in event-sourced events. |
| `packages/orchestrator/src/actors/state/host-events.ts` | Preserve metadata in Host projection. |
| `packages/host-runtime/src/host/run/controller.ts` | Preserve and validate ordering metadata. |
| `packages/host-tui/src/state/events.ts` | Accept ordered runtime lifecycle events. |
| `packages/host-tui/src/state/reducers/handleStream.ts` | Replace append/current-stream logic with projection upserts. |
| `packages/host-tui/src/state/reducers/handleToolCalls.ts` | Position tools by parent/index. |
| `packages/host-tui/src/timeline/transcript-reconcile.ts` | Split live finalization from legacy reconciliation. |
| `packages/host-tui/src/renderer/opentui/timeline/TimelineView.tsx` | Stable item-ID component identity. |

## Tests

### Protocol tests

- `eventSeq` increases across start/update/end/tool events.
- Message ID is unchanged across its lifecycle.
- Every tool event carries the same parent and tool position.
- Two runs may each start at sequence zero without collision.

### Projection reducer tests

- Repeated message updates do not change `orderedIds`.
- Duplicate update/end events are idempotent.
- Sequence regression produces a diagnostic/error.
- Tool start before/after another tool completion yields declaration order.
- A second model step is placed after all tools from the first step.
- Finalization does not change existing `orderedIds`.

### Integration tests

Use FauxProvider and deterministic delayed tools:

1. Assistant text-only streaming.
2. Thinking then text.
3. Tool-only assistant message.
4. Text plus one tool call plus final assistant response.
5. Three parallel tools completing in reverse order.
6. Multiple model steps with repeated empty/text-identical assistant messages.
7. Abort during assistant streaming.
8. Abort during parallel tool execution.
9. Resume a legacy session without runtime IDs.

For every case, capture `orderedIds` after each event and assert that the
relative order of existing IDs never changes.

### Renderer tests

- Track mount/dispose counts for completed and streaming timeline items.
- Streaming updates modify only the active MarkdownRenderable.
- Finalizing streaming toggles the same renderable to `streaming=false`.
- Tool result height changes do not replace adjacent timeline components.

## Diagnostics

Add development telemetry for:

```text
timeline.sequence_regression
timeline.update_without_start
timeline.duplicate_identity
timeline.missing_parent
timeline.commit_order_mismatch
```

Diagnostics include `runId`, `agentId`, `taskId`, entity ID, expected sequence,
and received sequence. Do not log model content or tool results by default.

## Acceptance criteria

The work is complete when all of the following are true:

- During streaming, existing timeline IDs never change relative order.
- Reverse-completing parallel tools display in tool-call declaration order.
- `turn_finished` causes no visible reorder and no completed item remount.
- No live assistant association uses role/text heuristics.
- One stable MarkdownRenderable is retained per active text block.
- Legacy sessions still load through the isolated compatibility path.
- A completed live projection and a projection rebuilt from its persisted
  transcript are semantically identical in entity order, kind, and status.
- `bun run fmt`, `bun run check`, and `bun run test` pass.

## Recommended implementation boundary

Implement the protocol and pure projection reducer first. Do not start by
patching scroll behavior or adding renderer delays. Scroll/layout symptoms
cannot be made deterministic while the underlying item identity and ordering
contract remains ambiguous.
