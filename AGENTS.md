# AGENTS.md — piko project context

## Project overview

piko is a coding agent harness with a decoupled **hostd + orchd** architecture. It splits the runtime into a stateful Rust **Host daemon** (sessions, settings, auth, prompts, skills, compaction, queues, turn orchestration) and a stream-driven Rust **Orchestrator** (agent runtime, tool routing, multi-agent supervision). The terminal client connects to hostd over JSON-lines stdio.

Guiding principle: keep the host+orchestrator split clean, and keep `hostd`
authoritative for user-visible state.

Goal: a general-purpose agent, not a codex replica. PRD-first (see
Documentation workflow). codex-rs is a modeling reference: reference its core
design and build piko's own modeling; details do not need 1:1 parity, and
codex-rs architecture or coupling is never translated.

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
| `piko-hostd` | lib + bin | Host daemon: sessions, settings, auth/models, prompts, compaction, queues, turn orchestration, MCP. Layering: `protocol` → `application`/`ports` ← `adapters` → `infra`; pure model in `domain`. |
| `piko-orchd` | lib | Agent runtime, tool registry, model steps, multi-agent AgentInstance tree. See `docs/codex-agent-core-digest.md`. |
| `piko-llmd` | lib | Model gateway, provider registry, OAuth, token/cost middleware. |
| `piko-sandbox` | lib | Fail-closed filesystem and process sandbox. |
| `piko-protocol` | lib | Shared DTOs only. See `packages/protocol/AGENTS.md`. |
| `piko-tui` | binary | Ratatui product UI over hostd. See `packages/tui/AGENTS.md`. |
| `piko-tui-layout` | lib | Product-agnostic flex layout + focus generics. See `packages/tui-layout/AGENTS.md`. |
| `piko-desktop` | binary | macOS GPUI desktop shell over hostd (F-42/D-59); island-rs for reusable chrome, client-core for projections. |

## Coding conventions

- Rust 2024 edition; workspace via root `Cargo.toml`
- No circular crate deps (`protocol` is the shared leaf)
- Domain-driven layout: `domain/` / `ports/` / `adapters/`
- `hostd` is the binary that depends on everything; `tui` is a standalone client over JSON-lines stdio
- All docs and code comments must be in English
- File size: prefer ~300–400 lines per `.rs` file; hard ceiling **500**. Split into a directory with `mod.rs` re-exports when over; do not over-split cohesive units under the ceiling

## Documentation workflow

All documentation is written in English and follows a PRD-first lifecycle:

1. Write the feature PRD (behavior contract, technology-agnostic) in `docs/features/`.
2. Write the implementation design in `docs/design/`.
3. Implement, recording technical decisions as ADRs in `docs/decisions/`.
4. Verify and update the PRD so it reflects implemented behavior.

Cross-package and system-level features live under the root `docs/` tree.
Package-local UI features use the same PRD-first lifecycle in
`packages/tui/docs/`. Feature PRDs derived from
another codebase (e.g. codex-rs core) carry a `Source` header for differential
validation.

codex-rs is a modeling reference, not a specification (ADR-002): distill its
core design into piko's modeling; do not chase 1:1 detail parity, and do not
translate its architecture or coupling. A behavior enters piko only when the
Feature PRD intentionally keeps it. At implementation time, design from
piko's architecture first: `hostd` stays authoritative for durable
user-visible state, `orchd` owns the agent runtime, `protocol` carries shared
wire types, and each feature persists results through channels it already
owns. When codex-rs modeling conflicts with piko, or a design point is
unclear, stop and discuss it with the user, defaulting to industry best
practice — keep the design that is best for piko.

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
7. Terminal UI, panels, keybindings, themes, CLI, product compose of layout → `tui` (see `packages/tui/AGENTS.md`)
8. Terminal flex layout, shell split, modal z-stack, generic focus stack → `tui-layout` (`piko-tui-layout`); product surfaces/recipes remain in `tui`
9. Product-independent GPUI runtime, layout, focus, chrome, theme, controls,
   and reusable desktop components → `island-rs`; the piko GUI owns only piko
   domain IDs and intents, host projections and transport, localization, and
   product composition. If a second GPUI application could use a component
   without piko's domain model, implement it in Island rather than privately in
   piko.

   Desktop UI must not import `gpui`/`gpui-base` directly. Reusable desktop
   runtime, layout, theme, chrome, controls, and form inputs live in `island-rs`
   and are consumed from there; piko only composes them (product IDs, intents,
   projections, localization). Lower-level GPUI primitives a product needs are
   re-exported through Island's facade rather than depended on directly.

## Session storage

Schema **v4** under `~/.piko/agents/sessions/<encoded-cwd>/<session-id>/`:

```text
session.json
events/<start>-<end>.jsonl
events/<start>-open.jsonl
readmodels/head.json
readmodels/catalog.json
readmodels/current.json
readmodels/trajectory.json
writer.lock
```

The append-only event journal is the sole durable authority. `session.json`
contains immutable identity only. Query paths read write-time projections
under `readmodels/`. No migration from older layouts.

## Configuration

- `~/.piko/settings.toml` — global settings
- `~/.piko/auth.json` — API keys per provider
- `.piko/settings.toml` — project overrides
- `.piko/skills/*.md`, `.piko/prompts/*.md`, `.piko/themes/*.json`
- `[tui]` in settings.toml — TUI-specific settings (stored by hostd)

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
Crate-local context: `packages/tui/AGENTS.md`, `packages/protocol/AGENTS.md`.

## Local development (TUI)

The client talks to a separate `piko-hostd` binary over stdio. Rebuilding
`piko-tui` alone does **not** refresh orchd or multi-agent tools.

```bash
./scripts/dev-tui.sh          # cargo build -p piko-hostd -p piko-tui && run
./scripts/dev-tui.sh -c       # args forwarded to the client
```

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
