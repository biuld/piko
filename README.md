# piko

A coding agent harness with a **Host + Engine** architecture. piko is a ground-up reimplementation of [pi](https://github.com/earendil-works/pi-mono) that replaces pi's monolithic runtime with a clean two-layer design: a stateful **Host** (session, TUI, scheduling, configuration) and a **stateless Engine** (LLM calls, tool execution, approval state machine).

> **Status:** Core feature-complete. Matching pi's coding-agent + agent-core behavior on most dimensions. See [docs/implementation-gaps.md](docs/implementation-gaps.md) for remaining delta.

## Architecture

```
┌──────────────────────────────────────────────────┐
│ piko Host                                         │
│ ┌──────────────┐ ┌──────────┐ ┌───────────────┐  │
│ │ PikoHost     │ │ TUI      │ │ CLI           │  │
│ │ scheduler    │ │ chat     │ │ args / modes  │  │
│ │ sessions     │ │ overlays │ │ file-processor│  │
│ │ settings     │ │ commands │ │               │  │
│ │ auth/models  │ │ themes   │ │               │  │
│ │ skills/prompts││ extensions│ │               │  │
│ └──────────────┘ └──────────┘ └───────────────┘  │
└────────────────────┬─────────────────────────────┘
                     │ EngineInput / EngineEvent / EngineStepResult
                     ▼
┌──────────────────────────────────────────────────┐
│ Stateless Engine                                  │
│ LLM provider call → tool execution → approval     │
│ Pure function: same input = same result           │
└──────────────────────────────────────────────────┘
```

- **Host** owns sessions, transcripts, TUI, settings, auth, skills, prompts, and the step scheduler
- **Engine** owns LLM calls, tool execution, approval state machines, and transcript generation
- Communication uses `executeStep()`: input is a full snapshot, output is an event stream + step result
- Approval is explicit state (not in-memory coroutines), enabling serialization and remote engines

## Quick Start

```bash
# Install
npm install -g piko

# Set API key
export ANTHROPIC_API_KEY=sk-ant-...
# or use /login in TUI

# Start
piko                          # new interactive session
piko -c                       # continue most recent session
piko -m claude-sonnet-4-5-20250929  # specify model
piko --thinking high          # set thinking level
piko --name "my session"     # name the session
piko --no-context-files       # skip AGENTS.md loading
```

## Packages

| Package | Description |
|---|---|
| `engine-protocol` | Pure types: `EngineInput`, `EngineEvent`, `EngineStepResult`, `StatelessEngine` |
| `engine-native` | In-process engine: state machine + tool runner + provider call |
| `engine-remote` | JSON-RPC client for remote engine servers |
| `host-runtime` | Host core: scheduler, session tree, model registry, settings, auth, skills, prompts, compaction |
| `host-tui` | Terminal UI: chat view, overlays, extensions, themes, tool rendering |
| `cli` | CLI entrypoint: argument parsing, model resolution, TUI launch |

### Dependency Graph

```
engine-protocol     ← zero deps (types only)
    ↑
    ├── engine-native    ← protocol + pi-ai (LLM providers)
    ├── engine-remote    ← protocol only
    ├── host-runtime     ← protocol + engine-native + pi-ai + pi-tui
    └── host-tui         ← protocol + host-runtime + engine-native + pi-tui
                                      ↑
                                    cli  ← host-runtime + host-tui
```

## Features

### Coding Tools

Built-in tools for code editing and exploration:

| Tool | Description | Approval |
|---|---|---|
| `read` | Read file contents with offset/limit | — |
| `bash` | Execute shell commands with optional timeout | ✅ requires |
| `edit` | Precise text replacements with `oldText`/`newText` | ✅ requires |
| `write` | Create or overwrite files | ✅ requires |
| `grep` | Search file contents for patterns | — |
| `find` | Find files by glob pattern | — |
| `ls` | List directory contents | — |

Destructive tools (bash, edit, write) require user approval by default. Extensions can register additional tools via `registerTool()`.

### Session Management

Full session tree with branching/forking/cloning:

```bash
/resume          # resume a session (with search)
/tree            # navigate session tree
/fork <entry>    # fork from a message
/clone           # clone current branch
/name <title>    # name the session
```

Sessions are stored as JSONL files under `~/.piko/sessions/<encoded-cwd>/`.

### Configuration

Layered configuration (defaults → global → project → CLI flags):

```
~/.piko/
  settings.json    # Global settings
  auth.json        # API keys
  skills/          # Global skills (*.md)
  prompts/         # Global prompt templates (*.md)
  themes/          # Global themes (*.json)
  AGENTS.md        # Global context instructions

.piko/
  settings.json    # Project settings (overrides global)
  skills/          # Project skills
  prompts/         # Project prompt templates
  themes/          # Project themes
  AGENTS.md        # Project context instructions
```

### System Prompt

The system prompt aggregates:
- **Context files**: `AGENTS.md` / `CLAUDE.md` from project, ancestors, and global `~/.piko/`
- **Skills**: `.piko/skills/*.md` with YAML frontmatter (name, description, model, tools)
- **Prompt templates**: `.piko/prompts/*.md` available as slash commands (with `$1`, `$@` argument substitution)
- **Tools**: Available tool descriptions and usage guidelines

### Compaction

Automatic context window management:
- Token-aware cut point detection
- LLM-based branch summarization on tree navigation
- Configurable thresholds via settings (`reserveTokens`, `keepRecentTokens`)

### Extensions

Extensions can register:
- **Commands** (`/mycommand`) via `registerCommand()`
- **Custom tools** via `registerTool()` — added to engine's tool set
- **Event hooks** via `on()` — `message`, `tool_call_start`, `tool_call_end`, `turn_end`
- **Widgets** (above/below editor), **status slots**, custom **footer** and **editor**
- **Dialogs**: select, confirm, input, custom overlays

### Themes

Built-in dark and light themes. Load external themes from `.piko/themes/*.json`:

```json
{
  "name": "my-theme",
  "colors": {
    "accent": "#ff6b6b",
    "border": "#5f87ff",
    "text": "#d4d4d4"
  }
}
```

Switch with `/theme <name>` or Ctrl+T.

### @file Syntax

Type `@path/to/file` in the editor to include file contents in your prompt. Supports relative and absolute paths.

## CLI Reference

```
piko [options]

Options:
  -m, --model <id>               Model ID (e.g. "claude-sonnet-4-5-20250929")
  --provider <name>              Provider name (e.g. "anthropic")
  -c, --continue                 Continue most recent session
  --session <id>                 Resume a specific session
  --thinking <level>             off | minimal | low | medium | high | xhigh
  --api-key <key>                API key for the provider
  --system-prompt <text>         Custom system prompt (replaces default)
  --append-system-prompt <text>  Append to default system prompt
  --name <name>                  Set session name
  --no-context-files             Skip AGENTS.md / CLAUDE.md loading
  --no-tools                     Disable tool calling
  --session-dir <path>           Custom session storage directory
  --list-models                  List available models
  -h, --help                     Show help
```

## Development

### Prerequisites
- Node.js ≥ 20
- npm ≥ 10

### Setup
```bash
npm install
npm run build
```

### Project Structure
```
piko/
  packages/
    engine-protocol/     # Shared types (zero runtime deps)
    engine-native/       # In-process engine + built-in tools
    engine-remote/       # JSON-RPC remote engine client
    host-runtime/        # Scheduler, sessions, settings, auth, skills, prompts, compaction
    host-tui/            # Terminal UI, overlays, extensions, themes
    cli/                 # CLI entrypoint
  docs/
    engine-abstraction.md          # Architecture spec
    missing-features.md            # Feature parity checklist
    implementation-gaps.md         # Action roadmap
  tsconfig.base.json
```

### Build & Test
```bash
npm run build          # TypeScript project references build
npm run check          # Type-check only
npm run clean          # Remove dist directories

# Run tests per package
cd packages/engine-native && npx vitest run
cd packages/host-runtime && npx vitest run
```

## Architecture Decisions

- **Stateless engine**: The engine receives a full transcript snapshot in every `executeStep()` call. It does not hold session state. This makes engines testable, composable, and remotable.
- **Host owns state**: Sessions, settings, auth, model registry, skills, and prompts all live in the Host layer. The engine only sees the final messages list.
- **Explicit approval**: Approval is not an in-memory suspend/resume. The engine returns `awaiting_approval` with serializable state, and the Host calls `resolveApproval()` to continue.
- **Step-based**: One LLM call per step. Tool calls execute in the same step. Max steps enforced by the Host.

## Upstream Dependencies

- `@earendil-works/pi-ai` — LLM provider abstraction, streaming, model catalog
- `@earendil-works/pi-tui` — Terminal UI primitives (TUI, Editor, Markdown, SelectList)

## License

MIT
