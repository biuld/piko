# orchd — Host ↔ Orchestrator interface design

## orchd's role

```
           ┌──────────────────────────┐
           │       Host               │
           │  session, auth, TUI,     │
           │  settings, skills,       │
           │  prompts, compaction     │
           └──────────┬───────────────┘
                      │  configure  │  run  │  events
                      ▼
           ┌──────────────────────────┐
           │       orchd              │
           │  agent runtime,          │
           │  tool execution,         │
           │  model calling,          │
           │  sub-agent coordination  │
           └──────────────────────────┘
```

orchd is a **stateless (across sessions) agent runtime**.
It doesn't know who the user is, what session they're in, where API keys come from,
or what the TUI looks like. It does exactly one thing: **given agent definitions +
model backends + tools + a task → produce results**.

The Host owns all "outside world" knowledge: users, projects, auth, UI.
orchd owns all "AI world" knowledge: agent loops, tool execution, model calling.

---

## Input

All orchd input falls into two categories: **one-time configuration** and **per-task input**.

### Startup configuration (one-time)

```rust
/// Full configuration passed by Host at startup.
/// This is everything orchd knows about the outside world.
pub struct OrchdConfig {
    /// LLM providers with credentials and endpoints.
    pub providers: HashMap<ProviderId, ProviderConfig>,

    /// Agent definitions: model, system prompt, tool sets.
    pub agents: HashMap<AgentId, AgentSpec>,

    /// Tool sets (workspace, MCP, plugins) with policies.
    pub tool_sets: HashMap<ToolSetId, ToolSetSpec>,

    /// Runtime limits (max concurrent agents, step timeout).
    pub runtime: RuntimeConfig,
}
```

#### ProviderConfig

```rust
/// Connection info for one LLM provider.
pub struct ProviderConfig {
    /// Provider type: openai / anthropic / openrouter / gemini / ...
    pub kind: String,

    /// API key (resolved by Host; orchd never touches env / keychain).
    pub api_key: String,

    /// Custom endpoint (OpenRouter, Azure, local proxy).
    pub base_url: Option<String>,

    /// Extra HTTP headers (e.g. OpenRouter HTTP-Referer).
    pub headers: Option<HashMap<String, String>>,
}
```

#### AgentSpec

```rust
/// Complete definition of one agent.
pub struct AgentSpec {
    /// Identifier ("main", "code-reviewer", "researcher", etc.).
    pub id: AgentId,

    /// System prompt (assembled by Host, includes skills, context files).
    pub system_prompt: String,

    /// Which model to use (references a provider from ProviderConfig).
    pub model: ModelRef,

    /// Runtime settings (thinking level, tool choice, etc.).
    pub settings: ModelRunSettings,

    /// Which tool sets are available (references ToolSetSpec ids).
    pub tool_sets: Vec<ToolSetId>,

    /// Optional entry condition (for router agents).
    pub entry_condition: Option<String>,
}
```

#### ToolSetSpec

```rust
/// Definition of a tool set (implementation lives in ToolProvider).
pub struct ToolSetSpec {
    pub id: ToolSetId,

    /// Tool names included in this set (for filtering).
    pub tools: Vec<String>,

    /// Which agents can use this set (empty = all).
    pub agent_filter: Vec<AgentId>,

    /// Approval policy: never / on_request / always / dangerous.
    pub approval: ApprovalPolicy,
}
```

### Task input (per-call)

```rust
/// Input for a single run / spawn operation.
pub struct TaskInput {
    /// User or upstream agent prompt.
    pub prompt: String,

    /// Target agent (default = "main").
    pub target_agent_id: Option<AgentId>,

    /// Optional conversation history (multi-turn, context injection).
    pub history: Option<Vec<Message>>,

    /// Per-task overrides (overrides AgentSpec settings).
    pub overrides: Option<TaskOverrides>,

    /// Parent task ID (set for sub-agent calls).
    pub parent_task_id: Option<TaskId>,
}

pub struct TaskOverrides {
    pub model: Option<ModelRef>,
    pub settings: Option<ModelRunSettings>,
    pub system_prompt_append: Option<String>,
}
```

---

## Output

orchd produces two things: an **event stream** and a **final result**.

### Event stream (real-time push)

```rust
/// Events pushed from orchd to Host.
pub enum OrchEvent {
    // ── Text streaming ──
    MessageStart { message_id: String, agent_id: AgentId, task_id: TaskId },
    TextDelta { message_id: String, delta: String },
    ThinkingDelta { message_id: String, delta: String },
    MessageEnd { message_id: String, stop_reason: String },

    // ── Tool execution ──
    ToolStart { tool_call_id: String, tool_name: String, agent_id: AgentId },
    ToolEnd { tool_call_id: String, ok: bool, output: Value },

    // ── User interaction (callback) ──
    AskUser { question: String, agent_id: AgentId },
    RequestApproval { action: String, details: Option<String>, agent_id: AgentId },

    // ── Sub-agents ──
    SubAgentSpawned { task_id: TaskId, agent_id: AgentId, mode: SpawnMode },
    SubAgentCompleted { task_id: TaskId, agent_id: AgentId, result: Value },

    // ── State ──
    PlanUpdated { agent_id: AgentId, task_id: TaskId, plan: Vec<Value> },

    // ── Lifecycle ──
    TaskError { task_id: TaskId, error: String },
    TaskEnd { task_id: TaskId, status: TaskStatus, usage: Usage },
}

pub enum SpawnMode { Call, Detach }
pub enum TaskStatus { Completed, Aborted, Error }
```

### Final result

```rust
pub struct TaskResult {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub messages: Vec<Message>,
    pub total_steps: u32,
    pub usage: Usage,
}
```

---

## Protocol flow

### Startup

```
Host                                 orchd
 │                                     │
 │──── configure(OrchdConfig) ────────►│  Initialize providers, agents, tools
 │◄─── Ok ─────────────────────────────│  Ready
 │                                     │
 │──── subscribe(listener) ───────────►│  Register event listener
 │◄─── Ok ─────────────────────────────│
```

### Task execution

```
Host                                 orchd
 │                                     │
 │──── run(TaskInput) ────────────────►│  Start agent loop
 │                                     │
 │◄─── MessageStart ───────────────────│
 │◄─── TextDelta ──────────────────────│  "I'll help you..."
 │◄─── ThinkingDelta ──────────────────│  (thinking)
 │◄─── ToolStart("read") ──────────────│
 │◄─── ToolEnd("read", ok=true) ───────│
 │◄─── MessageEnd ─────────────────────│
 │                                     │
 │◄─── AskUser("Which file?") ─────────│  User input needed (optional)
 │──── respond(user_input) ───────────►│  Host sends user response back
 │                                     │
 │◄─── SubAgentSpawned ────────────────│  Sub-agent started
 │◄─── SubAgentCompleted ──────────────│  Sub-agent finished
 │                                     │
 │◄─── TaskEnd(Completed, usage) ──────│  Task finished
 │◄─── result(TaskResult) ─────────────│  Final result
```

### User interaction (bidirectional)

orchd never interacts with users directly. When an agent needs user input,
orchd emits `AskUser` / `RequestApproval` events through the event stream.
The Host collects the user's response via the TUI and sends it back through
a separate channel.

This channel depends on the transport:
- **RPC mode**: Host calls `respond_user(task_id, response)` RPC method
- **In-process mode**: orchd calls registered `UserInteractionCallbacks`

---

## Interface abstraction

### Host-facing trait

```rust
/// Full public API surface exposed by orchd to Host.
pub trait OrchRuntime: Send + Sync {
    // ── Lifecycle ──
    async fn configure(&self, config: OrchdConfig) -> Result<(), Error>;
    async fn shutdown(&self) -> Result<(), Error>;

    // ── Task execution ──
    async fn run(&self, input: TaskInput) -> Result<TaskResult, Error>;
    async fn spawn(&self, input: TaskInput) -> Result<TaskId, Error>;
    async fn join(&self, task_id: &TaskId) -> Result<TaskResult, Error>;
    async fn cancel(&self, task_id: &TaskId, reason: &str) -> Result<(), Error>;

    // ── User interaction ──
    async fn respond_user(&self, task_id: &TaskId, response: UserResponse)
        -> Result<(), Error>;

    // ── State queries ──
    async fn snapshot(&self) -> Result<OrchState, Error>;
    async fn graph(&self) -> Result<GraphSnapshot, Error>;
}

pub enum UserResponse {
    AskUser { answer: String },
    RequestApproval { approved: bool, reason: Option<String> },
}
```

### Transport adapters

A single `OrchRuntime` trait, multiple transport implementations:

| Transport | Scenario | Implementation |
|---|---|---|
| **RPC** | TS host (cross-process) | JSON-RPC over stdio |
| **In-process** | Rust host (same process) | Direct `OrchCore` calls |
| **WebSocket** | Remote host (future) | JSON-RPC over WebSocket |

---

## Design principles

1. **orchd never touches env / keychain / filesystem config**. All credentials and
   configuration come from the Host.
2. **orchd doesn't know about sessions / users / projects**. It only processes Tasks.
3. **orchd never interacts with users directly**. When user input is needed, it
   notifies the Host through the event stream.
4. **Agent system prompts are assembled by the Host** (including skills, context files).
   orchd uses them as-is.
5. **Model calling is an orchd internal implementation detail**. The Host doesn't know
   about self-llm, HTTP clients, or retry logic.
6. **Transport is pluggable**. The same `OrchRuntime` trait, different wire implementations.
