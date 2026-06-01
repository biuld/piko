# piko-engine-remote

JSON-RPC client that maps a remote stateless engine to the `StatelessEngine` interface.

## Core Definition

Remote engines expose the same compute model over transport:

```text
EngineCompute:
  EngineInput -> EventStream<EngineEvent, EngineStepResult>
```

The remote client owns only transport mapping. It must not add Host behavior or native Engine business logic.

## Protocol

### Methods (client -> server)
- `engine/execute_step` — Execute a step
- `engine/resolve_approval` — Resolve a pending approval
- `engine/shutdown` — Shut down the remote engine

### Notifications (server -> client)
- `engine/event` — Stream events during step execution

## Usage

```typescript
import { createRemoteEngine } from "piko-engine-remote";

const engine = createRemoteEngine({
  transport: myTransport,
});

const stream = engine.executeStep(input);
for await (const event of stream) {
  // handle events
}
const result = await stream.result();
```
