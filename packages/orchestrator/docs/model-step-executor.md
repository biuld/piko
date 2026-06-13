# ModelStepExecutor — orchestrator internal subsystem

## Overview

`ModelStepExecutor` is the LLM interaction subsystem inside the orchestrator. It
**does not** execute tools or handle approval — it only:

1. Calls the LLM provider (pi-ai)
2. Streams deltas (text, thinking, tool calls)
3. Validates returned tool calls against registered `ToolDef` schemas
4. Produces `awaiting_resource` results when tool calls are detected
5. Generates tool result transcript messages from resolved results
6. Produces `completed` / `error` / `aborted` results

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
  createNativeModelExecutor,
  EventStream,
} from "piko-orchestrator";
```

## Native executor

The native executor (`createNativeModelExecutor`) wraps the pi-ai LLM caller in
a state machine. It is **not** the only possible implementation — the
`ModelStepExecutor` interface allows for:

- Remote executors (JSON-RPC)
- Faux/mock executors (for tests)
- Custom providers

The `ModelStepExecutor` interface is the orchestrator's internal boundary for
LLM interaction. The remote boundary for the orchestrator as a whole is defined
by the `Orchestrator` class's public API (registerAgent, run, subscribe, etc.).

## Flow

```
AgentActor
  └─ modelExecutor.executeStep(input)
       └─ EventStream<ModelStepEvent, ModelStepResult>
            └─ if awaiting_resource: AgentActor calls ToolActor
                 └─ ToolActor dispatches to ToolProvider
                 └─ AgentActor calls modelExecutor.resolveResource(results)
                      └─ produces tool result messages, continues
```

## Backward compatibility

The old `piko-protocol` package re-exports these types with their legacy names
(`StatelessEngine`, `EngineInput`, etc.) as aliases. New code should import from
`piko-orchestrator` directly.
