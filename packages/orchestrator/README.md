# Orchestrator

Orchestrator is piko's actor-first agent runtime.

It is the layer between Host and the LLM. Host owns UI, sessions,
settings, auth, and persistence. The ModelStepExecutor (internal subsystem)
handles one LLM step. Orchestrator owns agents, tasks, actor coordination,
tool routing, event state, and graph projection. Tool sensitivity is
coordinated by ToolActor, while runtime user approval is requested through
the Host-provided ApprovalGateway.

The current design intentionally does not preserve earlier Orchestrator code
shape.

## Core Direction

- Use a generic actor kernel as the execution substrate.
- Support concurrent work across actors while keeping each actor internally
  sequential.
- Keep the public Orchestrator facade thin.
- Put runtime behavior in actors.
- Use `async/await` as the pause/resume mechanism for waiting on tools,
  Host/user input, subagents, and state ingestion.
- Model public state with `StateActor`, not a thick facade-owned reducer.
- Keep Host out of actor internals.
- Keep ModelStepExecutor (internal) stateless and step-oriented.

## Architecture

```mermaid
flowchart TD
  Host[Host]
  Facade[Orchestrator facade<br/>thin ActorSystem adapter]

  Host -->|run / dispatch / cancel / snapshot| Facade

  subgraph Kernel[actor kernel]
    ActorSystem[ActorSystem<br/>Mailbox · Envelope · PendingAsk · Stop]

    subgraph Actors[piko actors]
      Main[orchestrator:main]
      State[orchestrator:state<br/>EventLog · reduce() · subscriptions]
      Agent[agent:&lt;id&gt;]
      SubAgent[agent:&lt;subagent&gt;]
      Tool[tool:registry]
    end
  end

  Facade --> ActorSystem
  Facade -->|ask run / dispatch / cancel / registerAgent| Main
  Facade -->|ask snapshot / renderGraph / dumpEvents| State

  Main -->|ask| Agent
  Main -->|ask| State
  Agent -->|ask| Tool
  Agent -->|ask| SubAgent
  Tool -. asks .-> ApprovalGateway[ApprovalGateway<br/>policy approval]
  Tool -. calls .-> HostProvider[HostToolProvider<br/>ask_user · model-requested approval]
  Agent -->|await emit event| State
  Tool -->|await emit event| State

  Agent -. calls .-> ModelExecutor[ModelStepExecutor<br/>internal subsystem]
  Tool -. calls .-> Executor[ToolExecutor / host tool bridge]
```

## Source Shape

```text
packages/orchestrator
  kernel/
    actor-system.ts
    mailbox.ts
    envelope.ts
    errors.ts

  actors/
    main.ts
    state.ts
    agent.ts
    tool.ts
    timer.ts

  orchestration/
    orchestrator.ts
    events.ts
    state.ts
    registry.ts

  docs/
    architecture.md
    actor-kernel.md
    tools/
    actors/
    events-and-state.md
    host-integration.md
```

The `kernel/` layer must not import engine, host, or piko-specific agent types.
Business actors live above the kernel.

## Design Docs

- [Architecture](docs/architecture.md) - boundaries, facade shape, source layout.
- [Actor Kernel](docs/actor-kernel.md) - actor IDs, envelopes, mailbox semantics,
  communication, failure, cancellation.
- [Actors](docs/actors/) - MainActor, AgentActor, ToolActor, StateActor,
  subagents, watches.
- [Tools](docs/tools/) - ToolProvider, ToolSet, and preset tool inventory.
- [Events And State](docs/events-and-state.md) - OrchestratorEvent,
  `StateActor`, event ingestion, reducer, snapshot, graph.
- [Host Integration](docs/host-integration.md) - Host responsibilities and
  forbidden coupling.

## ModelStepExecutor

The orchestrator's model interaction is through the `ModelStepExecutor`
interface (internal subsystem). See [docs/model-step-executor.md](docs/model-step-executor.md).

The `ModelStepExecutor` may have local/native/remote implementations, but
that is **not** the runtime protocol boundary. The orchestrator's remote
boundary is its public API (registerAgent, run, subscribe, snapshot).

## Public API Sketch

```ts
export interface Orchestrator {
  registerAgent(spec: AgentSpec): void;
  unregisterAgent(agentId: string): void;
  registerToolSet(toolSet: ToolSet): void;
  unregisterToolSet(toolSetId: string): void;
  setModelConfig(config: OrchModelConfig): void;
  setApprovalGateway(gateway: ApprovalGateway | undefined): void;
  registerProvider(provider: ToolProvider): void;
  dispatch(task: AgentTask): Promise<AgentTaskId>;
  run(prompt: string, opts?: OrchRunOptions): Promise<OrchRunResult>;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
}
```

The API returns promises where calls cross actor boundaries. The facade should
not run its own scheduler; it should translate API calls into actor messages.
