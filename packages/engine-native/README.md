# piko-engine-native

In-process stateless engine implementation.

## Core Definition

`piko-engine-native` implements the protocol compute model:

```text
EngineCompute:
  EngineInput -> EventStream<EngineEvent, EngineStepResult>
```

It is stateless across calls. Any state needed after a pause is returned as `EngineStepResult.engineState` and must be supplied again by the Host.

## Architecture

- **State machine** — Step-based execution (provider call -> tool execution -> approval)
- **Provider runner** — LLM interaction via `@earendil-works/pi-ai`
- **Tool runner** — Native tool execution with registry
- **Transcript builder** — Generates assistant and tool result messages

## Owns

- Provider stream normalization
- Tool lifecycle normalization
- Approval pause/resume mechanics
- Transcript delta generation
- Runtime limit enforcement

## Does Not Own

- Session persistence
- Settings/auth/skills discovery
- Approval UI or long-term approval policy
- TUI rendering
- Resume/fork/compaction

## Usage

```typescript
import { createNativeEngine } from "piko-engine-native";

const engine = createNativeEngine({
  tools: {
    my_tool: async (args) => ({ result: args.query }),
  },
});

const stream = engine.executeStep(input);
for await (const event of stream) {
  // handle events
}
const result = await stream.result();
```
