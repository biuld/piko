# orchd — Host ↔ Orchestrator interface

## Overview

orchd is a **Rust library** linked directly into hostd (same process). The interface is
a set of Rust function calls on `OrchCore`, not an RPC protocol.

```
hostd                                    orchd (lib)
─────                                    ─────
core.run_streaming(prompt, opts)
  → impl Stream<Item = Event>   root_agent_stream()
        │                                       │
        │  while let Some(event) =              │  stream! macro:
        │    stream.next().await {              │    loop {
        │      emit_to_tui(event)               │      yield Event::TextDelta...
        │    }                                  │      yield Event::ToolStart...
        │                                       │    }
        ▼                                       ▼
     TUI                                    LLM Gateway
```

orchd doesn't know about sessions, users, auth, or the TUI. It just produces a
`Stream<Event>` that hostd consumes.

## Configuration

### One-time: `OrchCore::from_config()`

```rust
let core = OrchCore::from_config(model_executor, OrchdConfig {
    providers: HashMap<String, ProviderConfig>,
    agents: HashMap<String, AgentSpec>,
    default_model: ModelRef,
    default_settings: ModelRunSettings,
    runtime: RuntimeConfig,
    sandbox: SandboxConfig,
    thinking_level_map: ThinkingLevelMap,
}).await;
```

### Per-turn: `core.run_streaming()`

```rust
let mut stream: impl Stream<Item = Event> = core
    .run_streaming(&prompt, Some(OrchRunOptions {
        command: OrchRunCommandOptions {
            target_agent_id: Some("main".into()),
        },
        history: None,
        host_context: Some(HostTaskContext {
            session_id: "session_1".into(),
            turn_id: "turn_1".into(),
        }),
    }))
    .await;

tokio::pin!(stream);
while let Some(event) = stream.next().await {
    // Forward to TUI via JSONL
}
```

No `subscribe()`, no `begin_run()` / `end_run()`, no channel setup needed.

## API surface

```rust
impl OrchCore {
    // ── Lifecycle ──
    pub async fn from_config(executor, config) -> Arc<Self>;
    pub async fn register_agent(&self, spec: AgentSpec);
    pub async fn unregister_agent(&self, agent_id: &str);

    // ── Task execution ──
    pub async fn run_streaming(&self, prompt, opts) -> impl Stream<Item = Event>;
    pub async fn run(&self, prompt, opts) -> OrchRunResult;  // convenience: collects stream
    pub async fn spawn(&self, task) -> (TaskId, Option<Value>);
    pub async fn spawn_detached(&self, task) -> TaskId;
    pub async fn await_task(&self, task_id) -> Option<Value>;

    // ── Steering ──
    pub async fn steer_task(&self, task_id, source_task_id, source_agent_id, message) -> bool;
    pub async fn cancel_task(&self, task_id, reason);

    // ── Tools ──
    pub async fn register_tool_set(&self, tool_set);
    pub async fn register_provider(&self, provider);

    // ── State ──
    pub async fn snapshot(&self) -> OrchState;
    pub async fn get_graph(&self) -> GraphSnapshot;
}
```

## Event stream

orchd emits `piko_protocol::Event` variants. The full event vocabulary is defined in
`packages/protocol/src/event.rs`. Key event categories:

| Category | Events |
|---|---|
| Task lifecycle | `TaskCreated`, `TaskStarted`, `TaskCompleted`, `TaskFailed`, `TaskCancelled`, `TaskJoined` |
| Model output | `MessageStart`, `TextDelta`, `ThinkingDelta`, `MessageEnd`, `AssistantMessageCompleted` |
| Tool execution | `ToolStart`, `ToolEnd`, `ApprovalRequested`, `ApprovalResolved` |
| Steering | `TaskSteered` |
| Turn lifecycle | `TurnStarted`, `TurnCompleted`, `TurnFailed` |
| Transcript | `TaskTranscriptCommitted`, `ToolResultCommitted` |

Events are produced directly by the agent loop via `yield` in the `stream!` macro —
no event sink, no listener registry, no channel bridge.

## Steering

Steer messages (follow-up instructions from user or parent agent) are sent
through an `mpsc::UnboundedSender<SteerMessage>` stored on `OrchCore` per-run.
The agent loop drains this channel at the start of each step:

```rust
// Inside stream! macro:
while let Ok(msg) = steer_rx.try_recv() {
    transcript.push(Message::User { ... });
    yield Event::TaskSteered { ... };
}
```

## Child tasks

When a tool spawns a sub-task (`spawn_detached`), the child runs as a `tokio::spawn`-ed
task. Child events are forwarded to the parent's `child_tx` channel, and the parent
drains `child_rx` in its main loop. (Full event forwarding for child tasks is pending.)

## Design principles

1. **No actors, no spawn inside orchd** — agent execution is poll-driven via Stream.
2. **Single Stream chain** — events flow LLM → agent → hostd → TUI without channel bridges.
3. **orchd never touches env / keychain / filesystem config** — all from Host.
4. **orchd doesn't know about sessions / users / projects** — only processes Tasks.
5. **Agent system prompts from Host** — orchd uses them as-is.
