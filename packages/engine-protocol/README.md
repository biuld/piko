# piko-engine-protocol

Shared protocol types for the piko stateless engine architecture.

## Core Definition

The Engine is a stateless Predict Machine:

```text
EngineCompute:
  EngineInput -> EventStream<EngineEvent, EngineStepResult>

Expanded:
  Snapshot x Runtime x ToolCatalog x ApprovalState
    -> Stream<Event> x StepResult
```

In code:

```typescript
export type EngineCompute = (
  input: EngineInput,
  signal?: AbortSignal,
) => EventStream<EngineEvent, EngineStepResult>;

export type EngineApprovalContinuation = (
  request: EngineApprovalResolution,
  signal?: AbortSignal,
) => Promise<EngineStepResult>;
```

The Host owns the transcript, session storage, settings, auth, skills, UI, and approval policy. The Engine computes over one complete input snapshot and returns normalized events, durable transcript delta, stop/continue status, and explicit continuation state.

## Exports

- `StatelessEngine` — Engine interface
- `EngineCompute` / `EngineApprovalContinuation` — Core compute model
- `EngineInput` / `EngineStepResult` — Step I/O
- `EngineEvent` — Streaming events during step execution
- `PendingApprovalState` / `EngineApprovalResolution` — Approval handshake
- `EngineTool` / `EngineToolExecutorRef` — Tool model
- `EngineProviderConfig` / `EngineRunSettings` — Runtime configuration
