# AGENTS.md — piko project context

## Project overview

piko is a coding agent harness with a decoupled **hostd + orchd** architecture. It splits the runtime into a stateful Rust **Host daemon** (sessions, TUI protocol, settings, auth, skills, prompts, compaction) and a stream-driven Rust **Orchestrator** (agent runtime, tool routing, multi-agent supervision). A **ratatui-based TUI** connects to hostd over JSON-lines stdio.

Guiding principle: keep the host+orchestrator split clean, and keep `hostd`
authoritative for user-visible state.

## Architecture

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
| `piko-tui` | binary | Ratatui UI (Slot → Panel → Component). Talks to hostd over stdio. See `packages/tui/AGENTS.md`. |
| `piko-hostd` | lib + bin | Host daemon: sessions, settings, auth/models, prompts, compaction, queues, turn orchestration, MCP. Layering: `protocol` → `application`/`ports` ← `adapters` → `infra`; pure model in `domain`. |
| `piko-orchd` | lib | Agent runtime, tool registry, model steps, multi-agent AgentInstance tree. See `docs/multi-agent-execution-model.md`. |
| `piko-llmd` | lib | Model gateway, provider registry, OAuth, token/cost middleware. |
| `piko-protocol` | lib | Shared DTOs only. See `packages/protocol/AGENTS.md`. |
| `piko-sandbox` | lib | Fail-closed filesystem and process sandbox. |

## Coding conventions

- Rust 2024 edition; workspace via root `Cargo.toml`
- No circular crate deps (`protocol` is the shared leaf)
- Domain-driven layout: `domain/` / `ports/` / `adapters/`
- `hostd` is the binary that depends on everything; `tui` is standalone and talks to hostd over stdio
- All docs and code comments must be in English
- File size: prefer ~300–400 lines per `.rs` file; hard ceiling **500**. Split into a directory with `mod.rs` re-exports when over; do not over-split cohesive units under the ceiling

## Where to change

1. TUI ↔ hostd wire types → `packages/protocol`
2. Sessions, settings, auth, models, prompts, skills, compaction, queue, approvals, command routing → `hostd`
3. Agent loops, tool execution, multi-agent supervision → `orchd`
4. Terminal UI, panels, keybindings, focus, themes, CLI → `tui` (see `packages/tui/AGENTS.md` and `.agents/skills/tui-feature-workflow/`)
5. Provider abstraction, OAuth, token tracking → `llmd`
6. Sandboxed file/process access → `sandbox`

## Session storage

Schema **v3** under `~/.piko/sessions/<encoded-cwd>/<session-id>/`:

```text
session.json
agents/<agent_instance_id>.jsonl
```

`session.json` holds AgentInstance metadata/inbox; transcripts are append-only
JSONL. No migration from older layouts.

## Configuration

- `~/.piko/settings.toml` — global settings
- `~/.piko/auth.json` — API keys per provider
- `.piko/settings.toml` — project overrides
- `.piko/skills/*.md`, `.piko/prompts/*.md`, `.piko/themes/*.json`
- `[tui]` in settings.toml — TUI-specific settings (stored by hostd)

## Docs

Normative runtime docs live under `docs/`:

- `docs/single-agent-runtime-model.md`
- `docs/single-agent-actor-runtime-design.md`
- `docs/agent-run-atomicity-design.md`
- `docs/turn-agent-run-boundary-design.md`
- `docs/multi-agent-execution-model.md`
- `docs/tool-sets-design.md`
- `docs/agent-prompt-assembly-design.md`

Crate-local context: `packages/tui/AGENTS.md`, `packages/protocol/AGENTS.md`.

## Before committing

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Testing

```bash
cargo test --workspace

cargo test -p piko-hostd
cargo test -p piko-orchd
cargo test -p piko-tui
cargo test -p piko-llmd
cargo test -p piko-protocol
cargo test -p piko-sandbox
```
