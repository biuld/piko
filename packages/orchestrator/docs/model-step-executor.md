# ModelStepExecutor — orchestrator internal subsystem

## Overview

`ModelStepExecutor` is the LLM interaction subsystem inside the orchestrator. It
**does not** execute tools or handle approval — it only:

1. Calls the LLM provider (pi-ai)
2. Streams deltas (text, thinking, tool calls)
3. Validates returned tool calls against registered `ToolDef` schemas
4. Produces `continue` status when tool calls are detected (so the agent can execute them and loop back)
5. Produces `completed` / `error` / `aborted` status

Tool execution and approval are handled by `ToolRegistryImpl.executeTool()` and `AgentActor`.

## Types

All types live in `packages/orchestrator/src/model/types.ts`:

- `ModelStepExecutor` — interface with `capabilities` and `executeStep`
- `ModelStepInput` — step input (runId, stepId, transcript, systemPrompt, model, provider, settings, tools, engineState)
- `ModelStepEvent` — streaming events: `step_start`, `message_start`, `message_delta`, `thinking_delta`, `message_update`, `provider_tool_call_delta`, `message_end`, `step_end`, `error`
- `ModelStepResult` — step result with `status` (`continue` | `completed` | `aborted` | `error`), `appendedMessages`, `usage`, `engineState`
- `ModelStepCompute` — function signature: `(input, signal?) => EventStream<ModelStepEvent, ModelStepResult>`
- `ModelContinuationState` — type alias for `ReadyContinuationState`

## Public API (from `piko-orchestrator`)

```typescript
import type {
  ModelStepExecutor,
  ModelStepInput,
  ModelStepEvent,
  ModelStepResult,
} from "piko-orchestrator";
import {
  createModelCaller,
  getModel,
  getModels,
  getProviders,
  getEnvApiKey,
} from "piko-orchestrator";
```

## Model Caller

The model caller (`createModelCaller`) wraps the pi-ai LLM caller in
a state machine that produces an `EventStream`. It is **not** the only possible
implementation — the `ModelStepExecutor` interface allows for:

- Remote executors (JSON-RPC)
- Faux/mock executors (for tests)
- Custom providers

The `ModelStepExecutor` interface is the orchestrator's internal boundary for
LLM interaction. The remote boundary for the orchestrator as a whole is defined
by the `Orchestrator` class's public API.

## Flow

```mermaid
sequenceDiagram
  participant AgentActor
  participant Executor as ModelStepExecutor
  participant ToolExecutor as ToolRegistry.executeTool
  participant Provider as ToolProvider

  loop Until Completed / Max Steps
    AgentActor->>Executor: executeStep(input, signal)
    Executor-->>AgentActor: EventStream&lt;ModelStepEvent, ModelStepResult&gt;
    AgentActor->>AgentActor: iterate stream, emit deltas
    AgentActor->>AgentActor: await stream.result()
    alt status === "continue" (tool calls present)
      AgentActor->>ToolExecutor: executeTool(call, context, route, signal)
      ToolExecutor->>Provider: execute(call, context, signal)
      Provider-->>ToolExecutor: result
      ToolExecutor-->>AgentActor: ToolExecResult
      AgentActor->>AgentActor: append tool results to transcript
    else status === "completed" / "error" / "aborted"
      AgentActor->>AgentActor: finalize task
    end
  end
```

## Public Types

Protocol-facing types live in `piko-orchestrator-protocol` (`ModelRunSettings`,
`ModelProviderConfig`, `Model`). ModelStepExecutor internals are imported from
`piko-orchestrator`.
