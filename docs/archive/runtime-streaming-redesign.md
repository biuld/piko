# Runtime Streaming Redesign

> Historical design note. This document references the former TypeScript
> `orch-protocol`/`orchestrator` runtime layout. The current hostd direction is
> tracked in `docs/architecture/hostd-global-plan.md`.

## Problem

piko currently streams model output through a token-oriented path:

```text
provider event -> ModelStepEvent.message_delta/thinking_delta
  -> OrchestratorEvent.task_delta
  -> HostEvent.token/thinking
  -> TUI assistant_delta/thinking_delta
```

This preserves enough information for a simple streaming transcript, but it loses
the provider's structured assistant message shape. In particular:

- Text and thinking are flattened into separate strings.
- Provider content block order is lost.
- `thinking_start` / `thinking_end` are not represented.
- Tool call streaming is not part of the main TUI streaming path.
- TUI has to reconstruct assistant messages from deltas instead of rendering the
  runtime's message lifecycle.
- The existing `HostLifecycleEvent` type is close to the desired direction, but
  it still carries `delta + isThinking` rather than a full partial assistant
  message, and it is not the main interactive streaming contract.

pi uses a different model: the agent emits message lifecycle events. During
assistant streaming, each update carries the full partial assistant message plus
the provider's current assistant message event. The UI updates one streaming
component from that structured message.

## Goals

- Make Host the boundary that projects actor/task events into UI/RPC/print-ready
  lifecycle events.
- Keep Orchestrator actor-first and event-sourced.
- Preserve structured assistant content blocks during streaming:
  text, thinking, tool calls, and future block types.
- Let TUI render ordered assistant content directly instead of accumulating
  separate `assistantText` and `thinkingText` fields.
- Maintain pi-compatible session persistence.
- Provide a staged migration path so existing stream consumers keep working
  while TUI moves to the richer protocol.

## Non-Goals

- Do not move UI concerns into Orchestrator.
- Do not make TUI consume raw provider events directly.
- Do not replace Orchestrator's task/actor event log with Host lifecycle events.
- Do not change the persisted session JSONL format unless a pi-compatible
  transcript requires it.

## Current Event Layers

### ModelStepEvent

Defined in `packages/orchestrator/src/model/types.ts`.

Current event variants:

```ts
step_start
message_delta
thinking_delta
message_end
step_end
error
provider_tool_call_delta
```

This is internal to model execution. It is too flat for pi-style streaming
because updates carry deltas instead of the provider's full partial assistant
message.

### OrchestratorEvent

Defined in `packages/orchestrator/src/actors/state/types.ts`.

This layer models actor/task state:

```ts
task_created
task_started
task_delta
task_completed
task_transcript_committed
tool_started
tool_finished
approval_requested
approval_resolved
...
```

This should remain the event-sourced orchestrator log. It can carry enough data
for Host projection, but it should not become the TUI protocol.

### HostEvent

Defined in `packages/orch-protocol/src/events.ts`.

Current host-visible stream variants include:

```ts
token
thinking
tool_start
tool_end
approval_needed
approval_resolved
task_started
task_created
task_completed
task_transcript_committed
task_failed
done
```

This is task/token oriented. It is useful for low-level observers, but it is not
rich enough for message lifecycle rendering.

### TUI Events

Defined in `packages/host-tui/src/state/events.ts`.

The interactive streaming path currently consumes:

```ts
message_delta -> assistant_delta
thinking_delta -> thinking_delta
```

Tool lifecycle events exist in state reducers but are not driven by the main
interactive stream path in the same way pi drives tool execution components from
message/tool lifecycle events.

## Target Contract

Introduce a Host runtime lifecycle stream as the UI/RPC/print contract:

```ts
type HostRuntimeEvent =
  | { type: "agent_start"; runId: string; agentId: string }
  | { type: "agent_end"; runId: string; agentId: string; status: RunStatus }
  | { type: "turn_start"; runId: string; agentId: string; turnIndex: number }
  | { type: "turn_end"; runId: string; agentId: string; turnIndex: number }
  | { type: "message_start"; runId: string; agentId: string; message: RuntimeMessage }
  | {
      type: "message_update";
      runId: string;
      agentId: string;
      message: RuntimeMessage;
      assistantEvent?: RuntimeAssistantMessageEvent;
    }
  | { type: "message_end"; runId: string; agentId: string; message: RuntimeMessage }
  | {
      type: "tool_execution_start";
      runId: string;
      agentId: string;
      toolCallId: string;
      toolName: string;
      args: unknown;
    }
  | {
      type: "tool_execution_update";
      runId: string;
      agentId: string;
      toolCallId: string;
      toolName: string;
      args: unknown;
      partialResult: unknown;
    }
  | {
      type: "tool_execution_end";
      runId: string;
      agentId: string;
      toolCallId: string;
      toolName: string;
      result: unknown;
      isError: boolean;
    }
  | { type: "queue_update"; ... }
  | { type: "failure"; ... };
```

The important shift is `message_update`: it carries a structured `RuntimeMessage`
snapshot, not just a delta string.

## Runtime Message Shape

Use a normalized message shape at the Host boundary. It should be close to pi-ai
messages but not leak provider-only implementation details into every consumer.

```ts
type RuntimeMessage =
  | RuntimeUserMessage
  | RuntimeAssistantMessage
  | RuntimeToolResultMessage
  | RuntimeCustomMessage;

interface RuntimeAssistantMessage {
  id: string;
  role: "assistant";
  content: RuntimeAssistantContentBlock[];
  isStreaming?: boolean;
  stopReason?: string;
  errorMessage?: string;
  usage?: Usage;
  provider?: string;
  model?: string;
  timestamp?: number;
}

type RuntimeAssistantContentBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string; thinkingSignature?: string }
  | { type: "toolCall"; id: string; name: string; arguments: unknown; partialJson?: string };
```

The final session transcript should still be saved in the pi-compatible
`Message` format. Host can convert between `Message` and `RuntimeMessage` at the
boundary.

## Projection Rules

### Provider to ModelStepEvent

ModelStepExecutor should preserve the provider partial message:

- On provider `start`, emit `message_start` with the partial assistant message.
- On provider `text_delta`, `thinking_delta`, `toolcall_delta`, etc., emit
  `message_update` with the latest partial assistant message and the low-level
  assistant event metadata.
- On provider `done` / `error`, emit `message_end` with the final assistant
  message.

The existing `message_delta` and `thinking_delta` can be retained during
migration as compatibility events, but they should be derived from the structured
message stream rather than being the source of truth.

### ModelStepEvent to OrchestratorEvent

Orchestrator should remain task-centric. It can emit:

- `task_message_start`
- `task_message_update`
- `task_message_end`

or extend `task_delta` with a structured payload:

```ts
{ kind: "message_update", message, assistantEvent }
```

Prefer explicit event types if the change does not overcomplicate existing
state projection. Explicit types make event log inspection and tests clearer.

### OrchestratorEvent to HostRuntimeEvent

Host owns this projection:

- `task_started` -> `turn_start` / `agent_start` as needed.
- `task_message_start` -> `message_start`.
- `task_message_update` -> `message_update`.
- `task_message_end` -> `message_end`.
- `tool_started` -> `tool_execution_start`.
- `tool_finished` -> `tool_execution_end`.
- queue changes from Host queues -> `queue_update`.
- completion/failure -> `turn_end`, `agent_end`, `failure`.

Host should filter events by active `agentId` for interactive TUI, but preserve
agent identity for RPC and multi-agent panes.

## TUI State Changes

Timeline items should move from text-specific fields:

```ts
text?: string;
thinkingText?: string;
```

to message content:

```ts
message?: RuntimeMessage;
content?: RuntimeAssistantContentBlock[];
```

Rendering should be block-based:

- `text` blocks render as markdown.
- `thinking` blocks render as italic thinking markdown or a hidden-thinking
  label.
- `toolCall` blocks create/update tool execution UI.

During streaming, TUI should update one timeline item by `message.id`, matching
pi's single streaming assistant component model.

## Compatibility Plan

1. Add new protocol types without changing behavior.
2. Add structured events at ModelStepExecutor while still emitting existing
   delta events.
3. Project structured events through Orchestrator and Host.
4. Add a new TUI reducer path for `message_start/update/end`.
5. Switch `ActionService` to consume the lifecycle stream.
6. Keep old `assistant_delta` / `thinking_delta` handlers for compatibility.
7. Remove or demote token-oriented stream events once RPC/print/TUI are migrated.

## Implementation Phases

### Phase 1: Types and Adapters

- Add runtime message and lifecycle event types to protocol or host-runtime.
- Decide whether these types belong in `orch-protocol` or
  `host-runtime`.
- Add conversion helpers:
  - `Message -> RuntimeMessage`
  - `RuntimeMessage -> Message`
  - provider partial assistant message -> `RuntimeAssistantMessage`

Recommended placement:

- Shared message/lifecycle types: `orch-protocol`
- Host projection helpers: `host-runtime`
- TUI view-model adapters: `host-tui`

Current status:

- Shared message/lifecycle types live in
  `packages/orch-protocol/src/runtime-stream.ts`.

### Phase 2: Structured Model Streaming

- Extend `ModelStepEvent` with `message_start`, `message_update`,
  `message_end` carrying structured messages.
- Update `model-caller.ts` to forward provider `event.partial`.
- Preserve old delta events temporarily.
- Add tests for:
  - text-only streaming
  - thinking-only before text
  - thinking + text order
  - tool call partial arguments

### Phase 3: Orchestrator Projection

- Add orchestrator event variants or structured `task_delta` payloads.
- Update AgentActor step runner to forward structured message lifecycle.
- Keep transcript commit behavior unchanged.
- Add state actor tests for structured message lifecycle events.

### Phase 4: Host Lifecycle Stream

- Add `streamPromptLifecycle()` or replace `streamPrompt()` result type behind
  an adapter.
- Keep existing `streamPrompt()` returning old `ModelStepEvent` until callers
  migrate.
- Update `PikoHost.prompt()` and resources to select the lifecycle path for TUI.

### Phase 5: TUI Migration

- Add lifecycle events to `packages/host-tui/src/state/events.ts`.
- Update timeline builder to build assistant items from `RuntimeMessage`.
- Render ordered content blocks in `AssistantMessageView` (Currently flattens text/thinking for backwards compatibility, with `RuntimeMessage` and `RuntimeAssistantContentBlock[]` fully stored in TUI state to support block-based rendering as a future/pending task).
- Route tool call blocks to tool timeline items or inline tool components,
  matching the desired pi-compatible UX.
- Keep old delta reducers until all stream callers are migrated.

### Phase 6: Cleanup

- Remove duplicate text/thinking accumulation where no longer needed.
- Reconcile transcript using final structured messages.
- Update docs and feature parity notes.

## Open Decisions

- Should `RuntimeMessage` live in `orch-protocol` or host-runtime?
  - Use `orch-protocol` if Orchestrator emits structured message events.
  - Use host-runtime if Host alone projects raw task events into runtime messages.
- Should Orchestrator introduce explicit `task_message_*` events or encode them
  inside `task_delta`?
- Should tool calls render inline inside assistant messages, as content blocks,
  or as separate timeline items linked by `toolCallId`?
- Should hidden thinking be a persisted display preference only, or should the
  runtime event carry a redacted/hidden marker?
- How much provider-specific assistant event metadata should be exposed to TUI?

## Risks

- Migrating every stream consumer at once is high risk. Keep adapters.
- If Host lifecycle events are placed too low, Orchestrator may absorb Host/UI
  concerns.
- If lifecycle events are placed too high, RPC/print may keep depending on
  token-level events and diverge from TUI.
- Reconcile logic can duplicate assistant messages if message IDs are not stable
  across streaming and final transcript commit.

## Success Criteria

- Thinking streams in the same order the provider emits content blocks.
- Text, thinking, and tool calls update one stable streaming assistant item.
- Final transcript reconciliation does not visibly replace or duplicate the
  streaming message.
- Existing session JSONL remains pi-compatible.
- TUI no longer needs to infer assistant message structure from separate
  `assistantText` and `thinkingText` accumulators.
- Tests cover structured streaming from model caller through TUI reducer.
