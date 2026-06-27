# orchd architecture

## What orchd is

```
┌──────────────────────────────────┐
│             piko                 │
│  ┌──────────┐  ┌──────────────┐  │
│  │   Host   │  │    orchd     │  │
│  │ (TS/RS)  │◀─│  (Rust)      │  │
│  │          │─▶│              │  │
│  │ session  │  │ agent loop   │  │
│  │ auth     │  │ tool exec    │  │
│  │ TUI      │  │ model call   │  │
│  │ skills   │  │ sub-agents   │  │
│  └──────────┘  └──────┬───────┘  │
│                       │          │
│                 LLM Providers    │
│              (OpenAI, Anthropic) │
└──────────────────────────────────┘
```

orchd is piko's **AI agent runtime**. It's a standalone Rust binary/library
that handles:

- **Agent loop** — receive prompt, iterate LLM calls + tool execution until done
- **Tool execution** — discovery, approval, parallel/sequential execution
- **Model calling** — OpenAI / Anthropic API via the `self-llm` adapter
- **Sub-agent coordination** — multi-agent task delegation (spawn / join)
- **Runtime event emission** — notify Host about task, model, and tool activity

orchd does **not** handle:

- User auth / API key management (Host provides)
- Session management / conversation persistence (Host manages)
- TUI rendering / user interaction (notifies Host via event stream)
- Project config / skills / system prompt assembly (Host assembles and passes in)

## Module structure

orchd is organized into six logical layers:

| Layer | Path | Purpose |
|---|---|---|
| **Protocol** | `protocol/` | Pure data types — config, events, messages, tool definitions, state. Zero business logic. Shared between Host and orchd. |
| **Model** | `model/` | LLM calling layer — `ModelStepExecutor` trait + `SelfLlmExecutor` (wraps `self-llm`). |
| **Tools** | `tools/` | Tool providers — `ToolRegistryImpl` for discovery/execution/approval, plus `TaskControlProvider`, `WorkspaceToolProvider`, `UserInteractionProvider`. |
| **Actors** | `actors/agent/` | Agent actor system — `AgentActor` (tokio-actors), `runner`, `engine_loop`, `step_runner`, `tool_executor`. |
| **Orchestrator** | `orchestrator/` | Orchestration core — `OrchCore` (central runtime), plus `agent`, `task`, `tool`, `state` sub-modules. |
| **RPC** | `rpc/` | JSON-RPC transport — stdio server + method dispatch handlers. |

## Core data flows

### Startup

```
Host
 │
 │── OrchdConfig ──► OrchCore::from_config()
 │   {                     │
 │     providers: [...]    ├── SelfLlmExecutor::from_providers()
 │     agents: {...}       ├── register_agent() for each agent spec
 │     default_model: ...  ├── register TaskControlProvider
 │     runtime: ...        ├── register WorkspaceToolProvider
 │   }                     └── register UserInteractionProvider
 │
 │── subscribe(listener) ─► register event listener
 │
 ▼
 orchd ready
```

### Task execution

```
Host ── run(TaskInput) ──► OrchCore
                              │
                              ▼
                         AgentActor::Dispatch
                              │
                    ┌─────────┴───────────┐
                    │  engine_loop        │
                    │                     │
                    │  loop:              │
                    │    discover tools   │
                    │    call model       │──► SelfLlmExecutor ──► self-llm ──► API
                    │    process outcome  │
                    │      ├─ text → emit │
                    │      ├─ tool → exec │──► ToolRegistry ──► ToolProvider
                    │      └─ done → break│
                    │                     │
                    └─────────┬───────────┘
                              │
                         TaskResult
                              │
                              ▼
                            Host
```

### Runtime events

```
Runtime activity
      │
      ▼
AgentActor / ToolRegistry
      │
      └── notify listeners         # push piko-protocol::Event to Host
```

Typical event sequence: `TaskCreated → [AssistantMessageDelta / ToolCallRequested / ToolResultCommitted]* → AssistantMessageCompleted`.
These events are not orchd-owned durable state. Hostd is responsible for turning
session facts into `SessionTreeEntry` JSONL records.

## Key traits

### OrchRuntime (Host → orchd)

```rust
pub trait OrchRuntime: Send + Sync {
    async fn configure(&self, config: OrchdConfig) -> Result<(), OrchdError>;
    async fn run(&self, input: TaskInput) -> Result<TaskResult, OrchdError>;
    async fn spawn(&self, input: TaskInput) -> Result<TaskId, OrchdError>;
    async fn join(&self, task_id: &TaskId) -> Result<TaskResult, OrchdError>;
    async fn cancel(&self, task_id: &TaskId, reason: &str) -> Result<(), OrchdError>;
    async fn snapshot(&self) -> Result<OrchState, OrchdError>;
    async fn subscribe(&self, listener: ...) -> Result<..., OrchdError>;
}
```

### ModelStepExecutor (orchd → LLM)

```rust
pub trait ModelStepExecutor: Send + Sync {
    fn execute_step(&self, input: ModelStepInput, cancel: ...)
        -> EventStream<ModelStepEvent, ModelStepResult>;
}
```

### ToolProvider (orchd → tools)

```rust
pub trait ToolProvider: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn discover(&self, context: ToolDiscoveryContext) -> ...;
    fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ...;
}
```

### Runtime state projection

`OrchCore::snapshot()` reads the current in-memory runtime projection: registered
agents, registered tool sets, and currently known task states. It is diagnostic
runtime state, not a recovery log. Restart recovery must come from hostd's
session JSONL.

## Transport

| Transport | Scenario | Implementation |
|---|---|---|
| JSON-RPC / stdio | Current: TS Host via child process | `rpc/server.rs` + `rpc/handlers.rs` |
| In-process | Future: Rust Host calling directly | `OrchCore` implements `OrchRuntime` |
| WebSocket | Future: Remote Host | JSON-RPC over WebSocket |

## Configuration

### OrchdConfig (provided once by Host)

```json
{
  "providers": {
    "openai": {
      "kind": "openai",
      "apiKey": "sk-...",
      "baseUrl": null
    }
  },
  "agents": {
    "main": {
      "id": "main",
      "name": "Main",
      "role": "assistant",
      "systemPrompt": "You are a helpful coding assistant...",
      "model": "gpt-4o",
      "toolSetIds": ["builtin", "workspace"]
    }
  },
  "defaultModel": { "provider": "openai", "modelId": "gpt-4o" },
  "defaultSettings": { "allowToolCalls": true },
  "runtime": { "maxConcurrentAgents": 4 }
}
```

## Dependencies

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `tokio-actors` | Actor system (one actor per agent) |
| `self-llm` | OpenAI / Anthropic API adapter |
| `serde` / `serde_json` | Serialization |
| `tracing` | Structured logging + distributed tracing |
| `uuid` | Task ID generation |
| `chrono` | Timestamps |
| `piko-sandbox` | Filesystem security policy (workspace provider) |

## Related docs

- [Host ↔ orchd interface](host-interface.md) — OrchdConfig, TaskInput, event stream
- [Runtime events & observability](event-sourcing-observability.md) — Runtime notifications, task projection, tracing
