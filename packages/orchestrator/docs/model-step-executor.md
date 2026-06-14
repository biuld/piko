# ModelStepExecutor — orchestrator internal subsystem

## Overview

`ModelStepExecutor` is the LLM interaction subsystem inside the orchestrator. It
**does not** execute tools or handle approval — it only:

1. Calls the LLM provider (pi-ai)
2. Streams deltas (text, thinking, tool calls)
3. Validates returned tool calls against registered `ToolDef` schemas
4. Produces `continue` status when tool calls are detected (so the agent can execute them and loop back)
5. Produces `completed` / `error` / `aborted` status

Tool execution and approval are handled by `ToolActor` and `AgentActor`.

## Types

All types live in `packages/orchestrator/src/model/types.ts`:

- `ModelStepExecutor` — interface (was `StatelessEngine`)
- `ModelStepInput` — step input (was `EngineInput`)
- `ModelStepEvent` — streaming events (was `EngineEvent`)
- `ModelStepResult` — step result (was `EngineStepResult`)
- `ModelRunSettings` — step configuration (was `EngineRunSettings`)
- `ModelProviderConfig` — LLM provider config (was `EngineProviderConfig`)
- `ModelContinuationState` — step state machine state (was `EngineContinuationState`)

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
  EventStream,
} from "piko-orchestrator";
```

## Model Caller (formerly Native Executor)

The model caller executor (`createModelCaller`) wraps the pi-ai LLM caller in
a state machine. It is **not** the only possible implementation — the
`ModelStepExecutor` interface allows for:

- Remote executors (JSON-RPC)
- Faux/mock executors (for tests)
- Custom providers

The `ModelStepExecutor` interface is the orchestrator's internal boundary for
LLM interaction. The remote boundary for the orchestrator as a whole is defined
by the `Orchestrator` class's public API (registerAgent, run, subscribe, etc.).

## Flow

```mermaid
sequenceDiagram
  participant AgentActor
  participant Executor as ModelStepExecutor
  participant ToolActor
  participant Provider as ToolProvider

  loop Until Completed / Max Steps
    AgentActor->>Executor: executeStep(input with transcript)
    Executor-->>AgentActor: EventStream&lt;ModelStepEvent, ModelStepResult&gt;
    alt tool calls present
      AgentActor->>ToolActor: executeToolCalls(toolCalls)
      ToolActor->>Provider: execute(call)
      Provider-->>ToolActor: result
      ToolActor-->>AgentActor: result
      AgentActor->>AgentActor: append tool results to transcript
    end
  end
```

## Public Types

Protocol-facing types live in `piko-orchestrator-protocol`. ModelStepExecutor
internals are imported from `piko-orchestrator`.
