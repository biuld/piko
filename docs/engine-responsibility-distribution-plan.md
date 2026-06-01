# Engine Responsibility Distribution Plan

## Goal

把 stateless Engine 扩展成完整的 **Predict Machine**：Engine 不持久化 session，但完整负责一次 agent 计算如何发生。

本方案把以下 5 个职责收拢到 Engine：

1. Provider streaming normalization
2. Tool lifecycle normalization
3. Approval continuation state
4. Transcript delta generation
5. Runtime limit enforcement

执行原则：

- Engine 输入必须是完整快照。
- Engine 输出必须是事件流和可持久化 delta。
- Host 只负责上下文装配、用户交互和持久化。
- 不把 session、settings 分层、auth 存储、TUI、skills 加载、compaction 策略放进 Engine。

## Target Boundary

```text
Host
  owns:
    session storage
    resume / fork / branch tree
    settings merge
    auth credential loading
    skills / prompts / context file loading
    approval UI and approval policy
    event rendering

Engine Protocol
  owns:
    stable input / event / result types
    normalized provider events
    normalized tool events
    transcript delta types
    runtime limit types
    approval continuation types

Native Engine
  owns:
    provider stream normalization
    tool lifecycle orchestration
    approval pause / resume mechanics
    transcript delta construction
    runtime limit enforcement

Remote Engine
  owns:
    transport mapping only
    no business logic
```

## Current Starting Point

Relevant files:

- `packages/engine-protocol/src/engine.ts`
- `packages/engine-protocol/src/types.ts`
- `packages/engine-native/src/engine.ts`
- `packages/engine-native/src/state-machine.ts`
- `packages/engine-native/src/provider-runner.ts`
- `packages/engine-native/src/tool-runner.ts`
- `packages/engine-native/src/approval-state.ts`
- `packages/engine-native/src/transcript-builder.ts`
- `packages/host-runtime/src/loop/agent-loop.ts`
- `packages/host-runtime/src/loop/engine-events.ts`

Current gaps:

- Provider normalization is mixed directly into `provider-runner.ts`.
- Tool lifecycle events are too small to represent validation, stdout/stderr, timeout, skipped calls, and normalized errors.
- Approval continuation stores raw `engineState` without a typed protocol shape.
- `EngineStepResult.appendedMessages` works, but it does not distinguish durable transcript facts from rendering events.
- Runtime limits are partially represented by settings, but enforcement is not centralized.

## Phase 1: Protocol Types

Objective: make the 5 responsibilities visible in `engine-protocol` before moving behavior.

### Files

- Edit `packages/engine-protocol/src/engine.ts`
- Edit `packages/engine-protocol/src/index.ts` only if exports need adjustment.

### Add Types

Add normalized provider events:

```ts
export type EngineProviderEvent =
  | { type: "provider_request_start"; provider: string; model: string }
  | { type: "provider_response_start"; status?: number; headers?: Record<string, string> }
  | { type: "provider_text_delta"; messageId: string; delta: string }
  | { type: "provider_thinking_delta"; messageId: string; delta: string }
  | { type: "provider_tool_call_delta"; id: string; name?: string; argsDelta?: string }
  | { type: "provider_message_end"; message: Message; usage?: TokenUsage }
  | { type: "provider_error"; message: string; retryable: boolean };
```

Add normalized tool lifecycle events:

```ts
export type EngineToolEvent =
  | { type: "tool_validation_start"; id: string; name: string }
  | { type: "tool_validation_end"; id: string; ok: boolean; error?: string }
  | { type: "tool_call_start"; id: string; name: string; args: Record<string, unknown> }
  | { type: "tool_stdout"; id: string; delta: string }
  | { type: "tool_stderr"; id: string; delta: string }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | { type: "tool_call_skipped"; id: string; reason: "approval_required" | "disabled" | "limit" | "invalid" };
```

Add transcript delta:

```ts
export type TranscriptDelta =
  | { kind: "assistant_message"; message: Message }
  | { kind: "tool_result"; message: Message; toolCallId: string }
  | { kind: "approval_record"; requestId: string; decision: "accept" | "decline" | "acceptForSession" };
```

Add typed engine continuation state:

```ts
export interface EngineContinuationState {
  version: 1;
  pendingToolCalls?: PendingToolCallState;
  counters?: EngineRuntimeCounters;
}

export interface PendingToolCallState {
  assistantMessage: Message;
  remainingToolCallIds: string[];
  toolCalls: Array<{ id: string; name: string; args: Record<string, unknown> }>;
  settings: { parallelTools?: boolean };
}
```

Add runtime limits:

```ts
export interface EngineRuntimeLimits {
  maxModelCalls?: number;
  maxToolCalls?: number;
  maxWallClockMs?: number;
  maxConsecutiveErrors?: number;
  maxApprovalRequests?: number;
  perToolTimeoutMs?: number;
}

export interface EngineRuntimeCounters {
  modelCalls: number;
  toolCalls: number;
  approvalRequests: number;
  consecutiveErrors: number;
  startedAt: number;
}
```

Update `EngineRunSettings`:

```ts
runtimeLimits?: EngineRuntimeLimits;
```

Update `EngineStepResult`:

```ts
transcriptDelta?: TranscriptDelta[];
engineState?: EngineContinuationState;
```

Keep `appendedMessages` during migration for compatibility. Treat it as derived from `transcriptDelta`.

### Acceptance Criteria

- `npm run check` passes.
- Existing callers still compile.
- No Host package imports from `engine-native`.
- Protocol package still has no dependency on Host packages.

## Phase 2: Provider Streaming Normalization

Objective: isolate provider-specific stream handling behind a normalized provider adapter.

### Files

- Add `packages/engine-native/src/provider/types.ts`
- Add `packages/engine-native/src/provider/pi-ai-adapter.ts`
- Edit `packages/engine-native/src/provider-runner.ts`
- Edit `packages/engine-native/src/state-machine.ts` only if event names change.

### Target Shape

```text
provider-runner.ts
  calls provider adapter
  receives EngineProviderEvent
  maps provider completion to ProviderResult

provider/pi-ai-adapter.ts
  imports @earendil-works/pi-ai
  knows pi-ai stream event names
  emits normalized EngineProviderEvent
```

### Rules

- `state-machine.ts` must not inspect pi-ai event names.
- `host-runtime` must not inspect provider event names.
- Provider errors must be classified as `retryable: boolean`.
- Provider usage must be attached to the final provider message event and `EngineStepResult.usage`.

### Acceptance Criteria

- Faux or pi-ai provider tests can assert only normalized Engine events.
- Removing pi-ai-specific event names from `provider-runner.ts` should not affect state machine semantics.
- Existing TUI rendering continues through `host-runtime/src/loop/engine-events.ts`.

## Phase 3: Tool Lifecycle Normalization

Objective: make all tool execution pass through one normalized lifecycle.

### Files

- Edit `packages/engine-native/src/tool-runner.ts`
- Edit `packages/engine-native/src/tools/registry.ts`
- Edit `packages/engine-native/src/types.ts`
- Edit `packages/engine-native/src/transcript-builder.ts`

### Target Flow

```text
assistant tool call
  -> validate tool exists
  -> validate arguments
  -> check runtime limits
  -> check approval requirement
  -> emit lifecycle events
  -> execute through registry
  -> normalize success/error result
  -> build transcript delta
```

### Rules

- Unknown tools become tool-result errors, not uncaught Engine errors.
- Invalid arguments become tool-result errors unless the protocol explicitly requires halt.
- Approval-required tools emit `tool_call_skipped` with `reason: "approval_required"` and then `approval_requested`.
- Tool execution must be addressable by tool call ID.
- Parallel execution must preserve deterministic transcript ordering by assistant tool call order.

### Acceptance Criteria

- Add tests for unknown tool, invalid args, successful tool, failing tool, approval-required tool.
- The state machine receives a single normalized `ToolExecutionBatchResult`.
- Host still receives enough events to render tool start and tool result.

## Phase 4: Approval Continuation State

Objective: replace untyped approval `engineState` snapshots with a typed continuation state.

### Files

- Edit `packages/engine-native/src/approval-state.ts`
- Edit `packages/engine-native/src/state-machine.ts`
- Edit `packages/engine-protocol/src/engine.ts`
- Edit `packages/host-runtime/src/approval-controller.ts` if it assumes raw state shape.

### Target Flow

```text
Engine detects approval-required tool
  -> stores EngineContinuationState.pendingToolCalls
  -> emits approval_requested
  -> returns awaiting_approval

Host asks user / policy
  -> calls resolveApproval with requestId and EngineContinuationState

Engine resolves approval
  -> accept: executes remaining pending tool calls
  -> acceptForSession: same as accept; Host owns session-level policy memory
  -> decline: emits a tool-result denial message
  -> returns transcriptDelta and continue/completed status
```

### Rules

- Engine owns pause/resume mechanics.
- Host owns whether a future call should skip prompting because of session policy.
- `acceptForSession` must not make Engine store cross-step permission memory.
- Decline must produce a durable transcript delta so the model can see the denial.

### Acceptance Criteria

- Approval resume can work after process restart if Host persisted `pendingApproval` and `engineState`.
- `resolveApproval` does not need in-memory closures from the previous `executeStep`.
- Tests cover accept, decline, and acceptForSession.

## Phase 5: Transcript Delta Generation

Objective: make Engine the only layer that decides which facts are appended to the transcript.

### Files

- Edit `packages/engine-native/src/transcript-builder.ts`
- Edit `packages/engine-native/src/state-machine.ts`
- Edit `packages/engine-protocol/src/engine.ts`
- Edit `packages/host-runtime/src/loop/agent-loop.ts`

### Target Flow

```text
Engine emits UI events during execution
Engine returns transcriptDelta at step end
Host persists transcriptDelta
Host does not derive transcript messages from UI events
```

### Rules

- `message_delta` and `thinking_delta` are rendering events only.
- `message_end`, `tool_call_end`, and `approval_requested` are not persistence APIs by themselves.
- `transcriptDelta` is the persistence API.
- During migration, `appendedMessages` remains populated from `transcriptDelta`.

### Acceptance Criteria

- Host persistence can be implemented using only `EngineStepResult.transcriptDelta`.
- A test proves replaying `oldTranscript + transcriptDelta` gives the expected next Engine input.
- TUI rendering does not affect transcript persistence.

## Phase 6: Runtime Limit Enforcement

Objective: centralize run limits inside Engine.

### Files

- Add `packages/engine-native/src/runtime-limits.ts`
- Edit `packages/engine-native/src/state-machine.ts`
- Edit `packages/engine-native/src/tool-runner.ts`
- Edit `packages/engine-protocol/src/engine.ts`

### Limits To Enforce

- `maxModelCalls`
- `maxToolCalls`
- `maxWallClockMs`
- `maxConsecutiveErrors`
- `maxApprovalRequests`
- `perToolTimeoutMs`
- `AbortSignal`

### Target Behavior

When a limit is reached:

```ts
{
  status: "completed" | "aborted" | "error",
  stopReason: "max_steps" | "abort" | "error",
  transcriptDelta,
  engineState
}
```

Use `error` only for actual failures. Use `completed` with a limit stop reason when the Engine stopped cleanly because a configured boundary was reached.

### Rules

- Limit counters live in `EngineContinuationState.counters`.
- Counters reset only when Host starts a new Engine run without prior `engineState`.
- Per-tool timeout wraps executor calls in Engine, not in Host.
- `AbortSignal` must short-circuit provider and tool execution where possible.

### Acceptance Criteria

- Tests cover max model calls, max tool calls, wall-clock timeout, tool timeout, and abort.
- Limit stop events are visible to Host.
- No Host scheduler code needs to inspect internal tool/provider progress to enforce these limits.

## Phase 7: Host Simplification

Objective: remove duplicated agent semantics from Host after Engine owns the 5 responsibilities.

### Files

- Edit `packages/host-runtime/src/loop/agent-loop.ts`
- Edit `packages/host-runtime/src/loop/engine-events.ts`
- Edit `packages/host-runtime/src/turn-state.ts`
- Edit `packages/host-tui/src/chat-view.ts` only if rendering event shape changes.

### Host Should Keep

- Build `EngineInput`.
- Consume `EngineEvent` for UI.
- Ask user for approvals.
- Persist `TranscriptDelta`.
- Continue scheduling when Engine returns `status: "continue"`.

### Host Should Drop

- Provider stream interpretation.
- Tool result construction.
- Approval resume mechanics beyond passing decisions back.
- Runtime limit enforcement that duplicates Engine behavior.
- Transcript reconstruction from render events.

### Acceptance Criteria

- Host loop reads like a scheduler, not an agent runtime.
- Engine tests cover agent semantics.
- Host tests cover persistence, approval UI handoff, and scheduling.

## Execution Order

1. Phase 1: Protocol Types
2. Phase 2: Provider Streaming Normalization
3. Phase 3: Tool Lifecycle Normalization
4. Phase 4: Approval Continuation State
5. Phase 5: Transcript Delta Generation
6. Phase 6: Runtime Limit Enforcement
7. Phase 7: Host Simplification

Do not start Phase 7 before Phases 2-6 have tests. Host simplification without Engine tests makes regressions hard to localize.

## Test Plan

Minimum package-level commands:

```bash
npm run check
cd packages/engine-native && npx vitest run
cd packages/host-runtime && npx vitest run
```

Required Engine test cases:

- Provider text streaming becomes normalized events.
- Provider thinking streaming becomes normalized events.
- Provider tool call streaming becomes normalized tool call state.
- Unknown tool returns a tool-result error.
- Invalid tool arguments return a tool-result error.
- Tool approval pauses with typed continuation state.
- Approval accept resumes after a fresh Engine instance is created.
- Approval decline appends a denial tool result.
- Transcript delta includes assistant and tool result messages in deterministic order.
- Max tool calls stops execution.
- Tool timeout stops or errors according to configured policy.
- AbortSignal stops provider/tool execution.

## Migration Safety

During migration:

- Keep `EngineStepResult.appendedMessages`.
- Add `transcriptDelta` without immediately forcing Host to use it.
- Keep existing `EngineEvent` names until TUI mapping is updated.
- Prefer additive protocol changes first, removal later.
- Every phase should end with `npm run check` passing.

Removal checklist after all phases:

- Remove raw `engineState?: unknown` from public protocol if no longer needed.
- Remove Host code that derives persisted messages from UI events.
- Remove pi-ai-specific event handling outside provider adapter.
- Remove duplicate runtime limits from Host scheduler.

## Done Definition

The distribution is complete when:

- Engine owns provider normalization, tool lifecycle, approval continuation, transcript delta, and runtime limits.
- Host can be described as context assembler, scheduler, approval UI, renderer, and persistence layer.
- `engine-remote` can replay the same protocol without sharing native Engine internals.
- A process restart during approval does not lose the ability to resume.
- Tests for Engine semantics live in `packages/engine-native`, not in Host/TUI.
