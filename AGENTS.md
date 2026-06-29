# AGENTS.md — piko project context

## Project overview

piko is a coding agent harness with a **hostd + orchd** architecture. It reimplements [pi](https://github.com/earendil-works/pi-mono) by splitting the monolithic runtime into layers: a stateful Rust **Host daemon** (sessions, TUI protocol, settings, auth, skills, prompts, compaction) and a stream-driven Rust **Orchestrator** (agent runtime, tool routing, task delegation, runtime state). A **ratatui-based TUI** connects to hostd over JSON-lines stdio.

The guiding principle: **replicate pi's functionality, keep the host+orchestrator split clean, and keep `hostd` authoritative for user-visible state.**

## Architecture

```mermaid
graph TD
  TUI[tui (ratatui)] --> Hostd[hostd]
  Hostd --> Orch[orchd]
  Hostd --> LLMD[llmd]
  Hostd --> Proto[protocol]
  Orch --> LLMD
  Orch --> Proto
  Orch --> Sandbox[sandbox]
  LLMD --> Proto
  Hostd --> Session[JSONL sessions]
```

### Crate dependency graph

```
tui ──────────────→ protocol
hostd ──→ llmd ──→ protocol
hostd ──→ orchd ──→ protocol
                  orchd ──→ llmd
                  orchd ──→ sandbox
sandbox (leaf)
```

| Crate | Type | Description |
|---|---|---|
| `tui` | binary | Ratatui terminal UI with a flat layout system (Slot → Panel → Component). Panels fill layout slots; overlays temporarily replace slots. Includes BottomBar, AgentPanel, NotificationRow, Editor, CommandPalette, ModelSelector, and more. Connects to hostd via JSON-lines stdio. See `packages/tui/docs/concepts.md` for terminology. |
| `hostd` | lib + bin | Host daemon: JSON-lines server, session storage, settings, auth/model resolution, prompt resources, compaction, queues, turn orchestration, MCP support. |
| `orchd` | lib | Orchestrator runtime: Stream\<Event\>-driven agent loop, tool registry, model steps, multi-agent task delegation. No actors, no spawn — single stream chain from LLM to hostd. |
| `llmd` | lib | LLM daemon library: model gateway abstraction, provider registry, OAuth, token/cost middleware, multi-provider catalog (OpenAI, Anthropic, Google, etc.). |
| `protocol` | lib | Pure serializable DTOs: commands, events, snapshots, messages, sessions, model config, agent state, tool definitions. Shared across all crates. |
| `sandbox` | lib | Fail-closed filesystem and process sandbox. Enforces access policy for tool execution. |

## Coding conventions

- **Rust 2024 edition** across all crates
- **Workspace** managed via root `Cargo.toml`
- **No circular dependencies** between crates (protocol is the only shared leaf)
- **Tests** use `cargo test --workspace`; integration tests in `tests/` dirs
- **Domain-driven** structure: `domain/` for business logic, `ports/` for traits, `adapters/` for implementations
- **hostd** is the sole binary that depends on everything; **tui** is a standalone binary that talks to hostd over stdio
- Stream processing in orchd uses `tokio_stream` / `async-stream`; hostd uses `tokio` channels

## When adding features

1. If it involves TUI/hostd wire types → `packages/protocol` (both crates depend on it)
2. If it involves session storage, settings, auth, models, prompts, skills, compaction, queue, approval state, or command routing → `hostd`
3. If it involves LLM interaction, agent loops, task orchestration, tool execution, multi-agent supervision → `orchd`
4. If it involves terminal UI, panels, rendering, keybindings, focus, themes, CLI parsing → `tui`
   - `panels/` — all visible elements (widget panels + overlay panels)
   - `components/` — reusable building blocks used by panels (FilterableList, etc.)
   - `config/` — TUI-specific config (namespace `tui.*`, stored on hostd)
   - `layout.rs` — flat layout engine (Slot allocation)
   - `render.rs` — top-level render dispatch
   - `input/` — editor, focus manager, keymap, completion engine
   - `docs/concepts.md` — terminology reference (Slot / Panel / Component)
5. If it involves model provider abstraction, OAuth, token tracking, multi-provider routing → `llmd`
6. If it involves sandboxed file/process access → `sandbox`
7. Types shared across `tui ↔ hostd` or `hostd ↔ orchd` → `piko-protocol` (add DTOs, re-export)

## Session storage

Sessions are stored as JSONL under `~/.piko/sessions/<encoded-cwd>/<session-id>.jsonl`. The format is pi-compatible.

## Configuration

- `~/.piko/settings.toml` — global settings (default model, theme, thinking level, compaction)
- `~/.piko/auth.json` — API keys per provider
- `.piko/settings.toml` — project settings (overrides global)
- `.piko/skills/*.md` — project skills
- `.piko/prompts/*.md` — project prompt templates
- `.piko/themes/*.json` — project themes
- `[tui]` section in settings.toml — TUI-specific settings (BottomBar items, notification behavior, etc.)

## Before committing

Always run formatting and lint before committing:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Testing

```bash
# Full suite
cargo test --workspace

# Per crate
cargo test -p hostd
cargo test -p orchd
cargo test -p tui
cargo test -p llmd
cargo test -p piko-protocol
cargo test -p piko-sandbox
```

## Pi reference

When implementing features from pi-mono, the reference files are at:
- `/Users/biu/Projects/pi-mono/packages/agent/src/agent-loop.ts`
- `/Users/biu/Projects/pi-mono/packages/agent/src/harness/agent-harness.ts`
- `/Users/biu/Projects/pi-mono/packages/coding-agent/src/`


