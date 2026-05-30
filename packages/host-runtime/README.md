# piko-host-runtime

Host scheduler that drives a stateless engine in a step-based loop.

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
