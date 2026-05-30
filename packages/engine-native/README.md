# piko-engine-native

In-process stateless engine implementation.

## Architecture

- **State machine** — Step-based execution (provider call -> tool execution -> approval)
- **Provider runner** — LLM interaction via `@earendil-works/pi-ai`
- **Tool runner** — Native tool execution with registry
- **Transcript builder** — Generates assistant and tool result messages

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
