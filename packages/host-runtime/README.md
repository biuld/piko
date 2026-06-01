# piko-host-runtime

Host scheduler that drives a stateless engine in a step-based loop.

## Boundary

The Host drives an Engine with the protocol compute model:

```text
EngineCompute:
  EngineInput -> EventStream<EngineEvent, EngineStepResult>
```

Host responsibilities are everything around that computation:

- Build the complete `EngineInput` snapshot
- Persist transcript and session state
- Render or forward `EngineEvent`
- Ask users or policy for approvals
- Re-enter the Engine with `EngineApprovalResolution`
- Handle resume, fork, compaction, settings, auth, skills, and prompt loading

The Host must not reconstruct durable transcript facts from UI events. Durable facts come from `EngineStepResult.transcriptDelta` during the migration, with `appendedMessages` kept for compatibility.

## Components

- **PikoHost** — Main entry point, accepts engine + config + prompt
- **Scheduler** — Step loop: build EngineInput, call executeStep, handle approval
- **SessionStore** — Minimal in-memory session/transcript persistence
- **ApprovalController** — Approval handler interface + auto-accept/decline implementations
- **ModelConfig** — Host configuration helpers

## Usage

```typescript
import { PikoHost, createHostConfig } from "piko-host-runtime";
import { createNativeEngine } from "piko-engine-native";

const host = new PikoHost({
  engine: createNativeEngine(),
  config: createHostConfig(model),
});

const result = await host.run("Hello, world!");
console.log(result.messages);
```
