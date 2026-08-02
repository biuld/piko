# AGENTS.md — piko project context

## Project overview

piko is a coding agent harness with a decoupled **hostd + orchd** architecture. It splits the runtime into a stateful Rust **Host daemon** (sessions, settings, auth, prompts, skills, compaction, queues, turn orchestration) and a stream-driven Rust **Orchestrator** (agent runtime, tool routing, multi-agent supervision). Terminal (Ratatui) and desktop (GPUI) clients connect to hostd over JSON-lines stdio.

Guiding principle: keep the host+orchestrator split clean, and keep `hostd`
authoritative for user-visible state.

Current direction: reimplement agent-runtime capabilities distilled from the
codex-rs core, PRD-first (see Documentation workflow). codex-rs is evidence,
not specification.

## Architecture

```
tui ──────────────→ protocol
gui ──────────────→ protocol
hostd ──→ llmd ──→ protocol
hostd ──→ orchd ──→ protocol
                  orchd ──→ llmd
                  orchd ──→ sandbox
sandbox (leaf)
```

| Crate | Type | Description |
|---|---|---|
| `piko-hostd` | lib + bin | Host daemon: sessions, settings, auth/models, prompts, compaction, queues, turn orchestration, MCP. Layering: `protocol` → `application`/`ports` ← `adapters` → `infra`; pure model in `domain`. |
| `piko-orchd` | lib | Agent runtime, tool registry, model steps, multi-agent AgentInstance tree. See `docs/codex-agent-core-digest.md`. |
| `piko-llmd` | lib | Model gateway, provider registry, OAuth, token/cost middleware. |
| `piko-sandbox` | lib | Fail-closed filesystem and process sandbox. |
| `piko-protocol` | lib | Shared DTOs only. See `packages/protocol/AGENTS.md`. |
| `piko-tui` | binary | Ratatui UI (Slot → Panel → Component). Talks to hostd over stdio. See `packages/tui/AGENTS.md`. |
| `piko-gui` | binary | GPUI desktop client (app / features / shell). Talks to hostd over stdio via client-core. See `packages/gui/AGENTS.md`. |
| `island` | external lib | GPUI Islands infrastructure (theme, island panel, layout tree, overlay surface, widgets). No product ids/messages. Developed in the sibling `island-rs` repository. |

## Coding conventions

- Rust 2024 edition; workspace via root `Cargo.toml`
- No circular crate deps (`protocol` is the shared leaf)
- Domain-driven layout: `domain/` / `ports/` / `adapters/`
- `hostd` is the binary that depends on everything; `tui` and `gui` are standalone clients over JSON-lines stdio
- All docs and code comments must be in English
- File size: prefer ~300–400 lines per `.rs` file; hard ceiling **500**. Split into a directory with `mod.rs` re-exports when over; do not over-split cohesive units under the ceiling

## Documentation workflow

All documentation is written in English and follows a PRD-first lifecycle:

1. Write the feature PRD (behavior contract, technology-agnostic) in `docs/features/`.
2. Write the implementation design in `docs/design/`.
3. Implement, recording technical decisions as ADRs in `docs/decisions/`.
4. Verify and update the PRD so it reflects implemented behavior.

Cross-package and system-level features live under the root `docs/` tree.
Package-local UI features use the same PRD-first lifecycle in their package
docs (`packages/tui/docs/`, `packages/gui/docs/`). Feature PRDs derived from
another codebase (e.g. codex-rs core) carry a `Source` header for differential
validation.

codex-rs is evidence, not specification: its behavior is distilled into PRDs; its architecture and coupling are not translated. A behavior enters piko only when the Feature PRD intentionally keeps it.

Legacy pre-PRD-first design documents are removed as PRDs and designs land;
behavior is re-specified by Feature PRDs (`docs/features/F-NN`) and
implementation designs (`docs/design/D-NN`). Kept documents carry a `Status`
header; a landed PRD supersedes earlier conflicting documents.

## Where to change

1. New behavior starts as a Feature PRD in `docs/features/`, then a design in `docs/design/` (see Documentation workflow).
2. Agent loops, tool execution, multi-agent supervision → `orchd`
3. Sessions, settings, auth, models, prompts, skills, compaction, queue, approvals, command routing → `hostd`
4. Provider abstraction, OAuth, token tracking → `llmd`
5. Sandboxed file/process access → `sandbox`
6. Wire types shared across packages → `packages/protocol`
7. Terminal UI, panels, keybindings, focus, themes, CLI → `tui` (see `packages/tui/AGENTS.md`)
8. Desktop GUI (GPUI), islands, overlays, Settings, `[gui]` → `gui` (see `packages/gui/AGENTS.md`)
9. Reusable Islands infrastructure (panel, theme, generic layout, overlay surface) → sibling `island-rs` repository (`island` crate); product ids/messages stay in `gui`
10. Island infrastructure docs → sibling `island-rs/docs/` (`features/` · `design/` · `roadmap/`)

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
- `[gui]` in settings.toml — GUI-specific settings (stored by hostd)

## Docs

Normative docs live under `docs/`:

- `docs/features/` — feature PRDs (numbered `F-NN`; create from `_TEMPLATE.md`)
- `docs/design/` — implementation designs (numbered `D-NN`)
- `docs/decisions/` — architecture decision records (numbered `ADR-NNN`)
- `docs/verification/` — acceptance and differential validation evidence

Global view and sequencing:

- `docs/codex-agent-core-digest.md` — codex-rs agent core split into functional
  blocks with piko coverage
- `docs/agent-runtime-roadmap.md` — milestone plan and per-block feature
  decomposition

UI and settings docs live in their package docs:

- `packages/tui/docs/` — TUI features/design
- `packages/gui/docs/` — GUI features/design/ui-guidelines

Crate-local context: `packages/tui/AGENTS.md`, `packages/gui/AGENTS.md`, `packages/protocol/AGENTS.md`.

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
cargo test -p piko-gui
cargo test -p piko-llmd
cargo test -p piko-protocol
cargo test -p piko-sandbox
```
