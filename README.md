# piko

Stateless engine abstraction for LLM-powered agent runtimes.

**piko** decouples the agent runtime into two clean layers: a **Host** (session management, UI, scheduling) and a **stateless Engine** (LLM interaction, tool execution, approval handling). The engine is a pure function — same input always produces the same semantic output — making it testable, composable, and remixable.

## Architecture

```
┌──────────────────────────────────────────┐
│ piko Host                                 │
│ session / TUI / approvals / scheduling    │
└────────────────┬─────────────────────────┘
                 │ EngineInput / EngineEvent / EngineStepResult
                 ▼
┌──────────────────────────────────────────┐
│ Stateless Engine                          │
│ provider orchestration / tool execution   │
│ sandbox / MCP / approvals / stop policy   │
└──────────────────────────────────────────┘
```

- **Host** owns sessions, transcripts, TUI, and step scheduling
- **Engine** owns LLM calls, tool execution, approval state machines, and transcript generation
- Communication uses a single `executeStep()` protocol: input is a full snapshot, output is an event stream + step result
- Approval is handled via explicit state (not in-memory coroutines), enabling serialization and remote engines

## Packages

| Package | Description |
|---|---|
| `engine-protocol` | Shared types, interfaces, and `EventStream` utility |
| `engine-native` | In-process stateless engine with state machine and tool runner |
| `engine-remote` | JSON-RPC client for remote engine servers |
| `host-runtime` | Scheduler, session store, model loading, approval controller |
| `host-tui` | Terminal UI (based on `@earendil-works/pi-tui`) |
| `cli` | Command-line entrypoint (`piko` binary) |

### Dependency Graph

```
engine-protocol     ← zero deps (types only)
    ↑
    ├── engine-native    ← protocol, LLM caller interface
    ├── engine-remote    ← protocol only
    ├── host-runtime     ← protocol + engine-native + engine-remote + @earendil-works/pi-ai
    └── host-tui         ← protocol + host-runtime + engine-native + @earendil-works/pi-tui
                                      ↑
                                    cli  ← host-runtime + host-tui + engine-native
```

### Package Details

#### `engine-protocol`

Pure types package defining the contract between Host and Engine:

- **Messages**: `UserMessage`, `AssistantMessage`, `ToolResultMessage` — a simplified message model
- **Tools**: `EngineTool` with pluggable executor references (`native`, `remote`, `sandbox`, `mcp`)
- **Engine interface**: `StatelessEngine` with `executeStep()`, `resolveApproval()`, `shutdown()`
- **Events**: `EngineEvent` discriminated union for streaming progress
- **Results**: `EngineStepResult` with status (`continue`, `awaiting_approval`, `completed`, `aborted`, `error`)
- **Approval**: `PendingApprovalState` and `EngineApprovalResolution` for explicit approval handoff
- **Infrastructure**: `EventStream<T, R>` — an `AsyncIterable` with a final result promise

#### `engine-native`

The default engine implementation. Runs provider calls and tool execution in-process via a state machine.

```
engine-native/src/
  engine.ts             — Factory: createNativeEngine(options)
  state-machine.ts      — Step state machine + approval resolution
  provider-runner.ts    — Converts transcript → LLM call → assistant message
  tool-runner.ts        — Executes tool calls, checks approval requirements
  approval-state.ts     — Pending approval creation and validation
  transcript-builder.ts — Message construction helpers
  types.ts              — Internal types, tool registry
  llm-caller.ts         — Abstract LLM caller interface
```

Key behaviors:
- **Step semantics**: One provider call per step, optionally followed by tool executions in the same step
- **Tool approval**: Tools with `metadata.requiresApproval` pause the step and return `awaiting_approval`
- **Stop conditions**: Configurable to stop on assistant message or tool result
- **Max steps**: Enforced per run settings

The engine is created with an `LlmCaller` — a lightweight interface that abstracts the actual LLM provider:

```typescript
const engine = createNativeEngine({
  llmCaller: createPiLlmCaller(),  // bridges to @earendil-works/pi-ai
  tools: { /* NativeToolRegistry */ },
});
```

#### `engine-remote`

JSON-RPC client for connecting to a remote engine server. Mirrors the `StatelessEngine` interface:

- `executeStep()` → `engine/execute_step` RPC + subscribes to `engine/event` notifications
- `resolveApproval()` → `engine/resolve_approval` RPC
- `shutdown()` → `engine/shutdown` RPC

Uses a pluggable `RemoteTransport` interface (WebSocket, stdio, HTTP, etc.):

```typescript
const engine = createRemoteEngine({ transport: myWebSocketTransport });
```

#### `host-runtime`

The Host runtime — the scheduler that drives the engine step by step.

Key modules:
- **`PikoHost`**: High-level API (`run()`, `streamPrompt()`) combining engine + config + sessions
- **`scheduler`** (`runScheduler`): Core step loop — builds input, runs engine, appends messages, handles approvals, enforces max steps
- **`SessionManager`**: Full session lifecycle — create, open, list, delete, fork, clone, branch, rename
- **`file-session-store`**: JSONL-based persistent storage under `~/.piko/agent/sessions/`
- **`session-store`**: In-memory session state with pure functions
- **`model-config`**: `HostConfig` combining model + provider + settings
- **`model-loader`**: Loads models from `@earendil-works/pi-ai`, resolves API keys from env
- **`approval-controller`**: Approval handler interface + auto-accept/decline implementations
- **`pi-llm-caller`**: Bridges `LlmCaller` interface to `@earendil-works/pi-ai` streaming
- **`bridge`**: Message type converters between protocol types and pi-ai types

Session features:
- JSONL format with entry types: `session` header, `message`, `model_change`, `session_info`
- Branching — navigate to any point in the tree via `/tree <entry-id>`
- Forking — create a new session from any user message via `/fork <entry-id>`
- Cloning — duplicate current branch into a new session via `/clone`
- Session naming — `/name <title>`

#### `host-tui`

Terminal UI built on `@earendil-works/pi-tui`:

- **Chat view**: Markdown-rendered conversation with streaming assistant responses
- **Editor**: Multi-line input with slash-command autocomplete
- **Overlays**: Session selector, tree navigator, fork picker, rename prompt
- **Session tree**: Threaded display of forked/cloned sessions

#### `cli`

Command-line entrypoint with modes:

```bash
piko                          # Interactive TUI (new or continue session)
piko -c                       # Continue most recent session
piko --session <id>           # Resume a specific session
piko -p "prompt"              # Single-shot non-interactive run
piko -m <model>               # Specify model
piko --list-models            # List available models
```

## Protocol

### Core Interface

```typescript
interface StatelessEngine {
  readonly capabilities: EngineCapabilities;
  executeStep(input: EngineInput, signal?: AbortSignal): EventStream<EngineEvent, EngineStepResult>;
  resolveApproval?(request: EngineApprovalResolution, signal?: AbortSignal): Promise<EngineStepResult>;
  shutdown?(): Promise<void>;
}
```

### Step Flow

```
Host builds EngineInput (full transcript snapshot)
  → Engine.executeStep(input)
    → emits EngineEvents (provider_request, message_delta, tool_call_start, etc.)
    → returns EngineStepResult
      status: "continue"        → Host loops to next step
      status: "completed"       → Host finalizes run
      status: "awaiting_approval" → Host collects user decision, calls resolveApproval()
      status: "error"           → Host handles error
```

### Design Principles

- **Stateless**: Engine output depends only on input. `engineState` is an opaque blob that the Host stores and passes back but never interprets
- **Transcript ownership**: Engine generates all assistant and tool-result messages; Host only persists
- **Explicit approval**: No in-memory suspended coroutines. Approval state is serialized in `PendingApprovalState`
- **Step-based**: One provider call per step. Tools in the same step. Max steps enforced by Host

## Development

### Prerequisites

- Node.js ≥ 20
- npm ≥ 10

### Setup

```bash
npm install
```

### Build

```bash
npm run build          # TypeScript project references build
npm run check          # Type-check only (no emit)
npm run clean          # Remove dist directories
```

### Project Structure

```
piko/
  packages/
    engine-protocol/     # Shared types, zero runtime deps
    engine-native/       # In-process engine
    engine-remote/       # JSON-RPC remote engine client
    host-runtime/        # Scheduler, sessions, model loading
    host-tui/            # Terminal UI
    cli/                 # CLI entrypoint
  docs/
    engine-abstraction.md        # Architecture spec
    stateless-engine-rollout.md  # Implementation plan
  tsconfig.base.json     # Shared compiler options
  tsconfig.json          # Project references root
```

### Session Storage

Sessions are stored as JSONL files under `~/.piko/agent/sessions/<encoded-cwd>/`. Each line is a JSON entry:

- `session` — header with id, version, timestamp, cwd, optional parent
- `message` — user, assistant, or tool result messages
- `model_change` — marks model switches
- `session_info` — session name/title

## Upstream Dependencies

piko depends on stable components from `@earendil-works`:

- `@earendil-works/pi-ai` — LLM provider abstraction, streaming, model catalog
- `@earendil-works/pi-tui` — Terminal UI primitives (TUI, Editor, Markdown, SelectList)

These are consumed as npm packages without forking.

## License

MIT
