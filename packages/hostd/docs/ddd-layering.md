# hostd DDD Layering

> Status: in progress (Phase 1–4 landed: `ports/`, `adapters/turns/`,
> `adapters/prompts/`, `application/`; domain IO purged). Remaining: Phase 5
> (docs cleanup) + narrower `SessionStorePort`/`AgentRuntimePort` ports.
> Crate-local layering rules for `hostd` module ownership.

## 1. Purpose

Define where code belongs inside `hostd`, so adapters like `OrchTurnRunner` are not
mistaken for domain logic, and domain modules stay free of orchd / filesystem / MCP IO.

This document is the crate-local interpretation of the workspace rule:

> Domain-driven structure: `domain/` for business logic, `ports/` for traits,
> `adapters/` for implementations.

## 2. Current state (problems)

Today hostd uses three top-level folders:

```text
protocol/   command routing + transport
domain/     mixed: pure state + orchd wiring + commit adapters
infra/      storage + MCP
```

Violations:

| Module | Why it is not domain |
|---|---|
| `domain/turns/orch_runner.rs` | Depends on `orchd::AgentRuntime`, `SessionStore`, MCP, tool registration |
| `domain/turns/notifying_execution_commit.rs` | orchd observation hub + commit fan-out |
| `domain/turns/session_output.rs` | HostState projection mixed with storage helpers |
| `domain/agents/loader.rs` | Filesystem skill/agent discovery (IO) |
| `domain/prompts/*` | Mostly pure, but loads files from cwd |
| `domain/compaction/summarizer.rs` | Calls LLM gateway (outbound port use belongs in application/adapter) |

Earlier docs placed `OrchTurnRunner` in domain as an “adapter”. That contradiction
is what this design fixes.

## 3. Target layers

```text
packages/hostd/src/
  protocol/          # delivery: JSON-lines, Command → use-case call, ServerMessage out
  application/       # use-cases / orchestration (short-lived; may await ports)
  domain/            # pure session/turn/policy model (no orchd, no fs, no MCP)
  ports/             # traits owned by hostd (inbound + outbound)
  adapters/          # port implementations (orchd, storage, MCP, LLM, filesystem)
  infra/             # shared low-level IO utilities used by adapters (optional thin)
```

Dependency direction (strict):

```text
protocol → application → domain
                ↘ ports ← adapters
domain → ports        (domain may define or depend on abstract ports)
adapters → ports + domain types + external crates
infra → nothing above it (leaf helpers)
```

Forbidden:

```text
domain → adapters | infra | orchd | reqwest | std::fs for product paths
protocol → adapters directly for business decisions (prefer application)
adapters → protocol
```

## 4. Layer responsibilities

### 4.1 `protocol/`

- Parse/serialize `Command` / `ServerMessage`
- Transport framing (`jsonl_stdio`)
- Map protocol errors to wire strings
- Call **application** use-cases; do not embed orchd attach/start logic

Keep: `HostServer` as the composition root that constructs adapters and injects
them into application services (or a thin `App` facade).

### 4.2 `application/`

Use-case services. Examples:

| Service | Responsibility |
|---|---|
| `TurnService` | assemble prompt inputs, start turn, subscribe observation, map terminal → TurnLifecycle |
| `SessionService` | create/open/fork/import/list via storage port |
| `CompactionService` | decide + run compaction using summarizer + storage ports |
| `AuthService` | OAuth / API key flows via auth port |

Application may:

- await outbound ports
- hold short orchestration state for one command
- translate domain decisions into port calls

Application must not:

- own durable session transcript truth (that is storage / SessionActor-equivalent)
- embed orchd Actor internals

### 4.3 `domain/`

Pure host business model:

| Module | Owns |
|---|---|
| `sessions/` | `HostState`, queues, active turn markers, agent list projection rules |
| `config` | settings schemas, model resolution pure logic |
| `compaction` | cut-point / threshold policy (no LLM call) |
| `prompts` | prompt assembly from already-loaded materials (loading is adapter) |
| `commands` | slash/command catalog pure data |

Domain tests should not need tempfile, orchd, or network.

### 4.4 `ports/`

Hostd-owned traits (not orchd-api re-exports as the only abstraction).

Inbound (driven by protocol):

```rust
#[async_trait]
trait TurnUseCase: Send + Sync {
    async fn submit(&self, input: SubmitTurn) -> Result<TurnHandle, HostError>;
}
```

Outbound (driven by application/domain needs):

```rust
#[async_trait]
trait TurnRunner: Send + Sync { /* existing contract, moved here */ }

#[async_trait]
trait SessionStorePort: Send + Sync { /* SessionStore surface used by hostd */ }

#[async_trait]
trait AgentRuntimePort: Send + Sync {
    // narrow façade over AgentRuntimeApi / AgentExecutor hostd actually needs
}

trait PromptMaterialLoader: Send + Sync { /* skills, context files */ }
trait McpGateway: Send + Sync { /* optional */ }
```

Rules:

- Prefer **narrow** ports over re-exporting entire `AgentRuntimeApi` into domain.
- `orchd-api` traits may be used **inside adapters**; application depends on
  hostd ports that adapters implement by delegating to orchd-api.

### 4.5 `adapters/`

| Adapter | Implements | Notes |
|---|---|---|
| `adapters/turns/orch_runner/` | `TurnRunner` / `AgentRuntimePort` | today’s `OrchTurnRunner` |
| `adapters/turns/notifying_commit.rs` | commit + observation fan-out | today’s notifying port |
| `adapters/storage/session_store.rs` | `SessionStorePort` | wrap `infra/storage/session_store` |
| `adapters/storage/jsonl_sessions.rs` | session CRUD | today’s `JsonlSessionRepository` |
| `adapters/prompts/fs_loader.rs` | `PromptMaterialLoader` | skills/agents from disk |
| `adapters/mcp/` | `McpGateway` | move from `infra/mcp` or keep infra + thin adapter |
| `adapters/llm/` | summarizer gateway | compaction |

### 4.6 `infra/`

Keep only leaf IO that adapters share:

- JSONL read/write primitives
- path encoding
- tempfile-unrelated filesystem helpers

Do not put use-case orchestration in `infra/`.

## 5. Target placement for today’s problem files

```text
domain/turns/runner.rs                    → ports/turn_runner.rs
domain/turns/orch_runner.rs               → adapters/turns/orch_runner/
domain/turns/notifying_execution_commit.rs → adapters/turns/notifying_commit.rs
domain/turns/session_output.rs            → split:
                                              domain: projection pure helpers
                                              adapters: anything touching SessionStore
domain/turns/approval.rs                  → adapters/turns/approval.rs (landed, Phase 4)
domain/agents/loader.rs                   → adapters/prompts/agent_loader.rs (landed, Phase 4)
domain/prompts/{mod,skills}.rs fs loaders → adapters/prompts/{loader,skill_loader}.rs (landed, Phase 4)
infra/storage/session_store.rs            → stay infra OR adapters/storage (either ok if ports wrap it)
protocol/commands/turns.rs               → landed: application/turns.rs (impl HostApp)
protocol/commands/sessions.rs            → landed: application/sessions/ (impl HostApp)
protocol/commands/compaction.rs          → landed: application/compaction.rs (impl HostApp)
```

## 6. Bounded contexts inside hostd

hostd is one deployable, but still has internal contexts:

```text
Session Management     HostState, open/fork/import, tree navigation
Turn Orchestration     submit/cancel/steer/follow-up queues, TurnLifecycle events
Agent Projection       AgentInfo list/subscribe views for TUI
Config & Auth          settings, models, OAuth
Compaction             threshold + summary persistence
Runtime Integration    orchd AgentRuntime attach/exec/commit (adapter context)
```

Cross-context rules:

- Turn Orchestration talks to Runtime Integration only through ports.
- Agent Projection is updated from committed observation / durable agent commands,
  not by polling orchd mailboxes.
- Session Management owns durable paths; Runtime Integration must not invent a
  second session store.

## 7. Composition root

`main.rs` / `HostServer::new`:

1. Build infra (paths, settings)
2. Build adapters (SessionStore, OrchTurnRunner, MCP)
3. Build application services with ports injected
4. Hand services to protocol router

No `lazy_static` global orchd handle inside domain modules.

## 8. Phased migration

### Phase 0 — Document + guardrails

- Accept this layering doc.
- Add a simple architecture test or module `use` lint note: `domain` must not
  `use orchd::` / `use crate::infra::`.

### Phase 1 — Extract ports

- Move `TurnRunner` + `TurnRunInput` to `ports/`.
- Introduce `SessionStorePort` matching the methods application actually calls.

### Phase 2 — Move OrchTurnRunner

- Relocate `orch_runner` + notifying commit to `adapters/turns/`.
- `domain` no longer depends on orchd.

### Phase 3 — Application services (landed)

- `application::HostApp` now owns all mutable host state and constructors
  (`new`, `with_storage`, `with_turn_runner`, `with_storage_and_runner`,
  `with_storage_runner_settings`, `set_model_executor`). `protocol::HostServer`
  is a thin `pub struct HostServer(pub(crate) HostApp)` newtype with `Deref` /
  `DerefMut` to `HostApp`; it keeps command routing/transport and forwards
  turn/session/compaction use-cases to `self.0`.
- `protocol/commands/turns.rs` body moved to `application/turns.rs` as
  `impl HostApp` (turn submission, execution observation, queue draining).
- `protocol/commands/sessions.rs` body moved to `application/sessions/` as
  `impl HostApp` (create/open/list/fork/import/rename/delete/navigate/label/
  snapshot).
- `protocol/commands/compaction.rs` body moved to `application/compaction.rs`
  as `impl HostApp` (`compact_session_if_needed`).
- `now_ms` / `send_event` / `storage_error` moved to a crate-root `util`
  module so neither `application` nor `protocol` depends on the other; both
  reach the helpers via `crate::util`.
- `protocol/commands/auth.rs` and `protocol/commands/config.rs` stay on
  `HostServer` for now (OAuth device flow + config-patch observers) — they
  read/write `HostApp` fields through `Deref`. Candidate for a future
  `AuthService` / config application service, tracked as debt below.

### Phase 4 — Purge remaining domain IO (landed)

- `domain/turns/approval.rs` → `adapters/turns/approval.rs` (re-exported from
  `adapters::turns` and `adapters` as before). `domain/turns/` is deleted
  (it only ever held the approval adapter).
- `domain/agents/loader.rs` → `adapters/prompts/agent_loader.rs`.
  `domain/agents/` is deleted (it only held the loader).
- `domain/prompts/mod.rs` filesystem loaders (`load_context_files`,
  `load_prompt_templates`, `home_dir`, `piko_dir`, `find_workspace_root`) →
  `adapters/prompts/loader.rs`. Pure formatting/parsing (`build_system_prompt`,
  `expand_prompt_template`, `parse_frontmatter_result`, template/arg
  substitution helpers) stays in `domain/prompts`.
- `domain/prompts/skills.rs` filesystem loader (`load_skills`,
  `scan_directory`, `load_skill_from_file`) → `adapters/prompts/skill_loader.rs`.
  Pure `Skill`/`SkillDiagnostic`/`LoadSkillsResult` types and
  `format_skills_for_prompt` stay in `domain/prompts/skills`.
- New `adapters/prompts/mod.rs` re-exports `load_agents`, `load_context_files`,
  `load_prompt_templates`, `load_skills` as the adapter-facing surface.

### Phase 5 — Cleanup docs

- Update root `AGENTS.md` hostd bullet to mention `ports/` + `adapters/`.

## 9. Explicit non-goals

- Do not invent a second Event Sourcing stack inside hostd.
- Do not move orchd’s Actor model into hostd domain.
- Do not require hexagonal purity for tiny helpers (logging, `now_ms`).
- Do not rename every `infra` file in Phase 1; move boundaries first.

## 10. Acceptance criteria

1. `rg "use orchd" packages/hostd/src/domain` returns empty. ✅
2. `rg "SessionStore" packages/hostd/src/domain` returns empty (except docs/comments). ✅
3. `OrchTurnRunner` lives under `adapters/`. ✅ (`adapters/turns/orch_runner/`)
4. Protocol turn submit goes through an application service. ✅
   (`HostServer::apply_command_stream` calls `self.0.apply_turn_submit(...)`
   on `application::HostApp`.)
5. Domain unit tests run without tempfile/orchd. ✅ (`domain/` no longer has
   `agents/` or `turns/`; `domain/prompts` fs loading moved to `adapters/prompts`.)
6. `cargo clippy --workspace --all-targets -- -D warnings` and hostd tests pass. ✅

Remaining debt (tracked, not blocking Phase 3–4):

- `protocol/commands/auth.rs` and `protocol/commands/config.rs` still live on
  `HostServer` directly instead of an `AuthService` / config application
  service (Phase 3 only required turn/session/compaction extraction).
- `build_orch_turn_runner` still lives in `protocol/mod.rs` and constructs an
  `adapters::OrchTurnRunner` directly — a `protocol → adapters` edge the
  target layering forbids for business decisions; candidate to move into
  `application` alongside a `TurnRunnerFactory`-style port.
- `infra/storage/jsonl_repository/` and `adapters/turns/orch_runner/`
  call `adapters::prompts::agent_loader::load_agents` directly (no
  `PromptMaterialLoader` port yet); acceptable per §4.5 but not yet behind a
  port.
- `domain/compaction/summarizer.rs` still calls the LLM gateway directly
  (outbound port use noted as a Phase-3-adjacent violation in §2); out of
  scope for this pass.
- No `SessionStorePort` / `AgentRuntimePort` abstraction yet — `application`
  depends on `adapters::OrchTurnRunner` via the existing `ports::TurnRunner`
  trait plus direct `infra::storage` types, matching "Application MAY use
  adapters and infra for now" from the Phase 3–4 task brief.
