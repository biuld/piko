# D-03: Prompt-assembly fragment catalog breadth

> Status: accepted
> Implements: [F-03](../features/F-03-prompt-assembly.md)

## Goal

Deliver the F-03 fragment-catalog slice: a **world-state** fragment
(`state.run`), an **environment-context** fragment (`environment.host`), and
a **model-switch** fragment (`context.model-switch`) in the hostd-owned
frozen per-run prompt snapshot, with hostd-authoritative model continuity so
the model-switch fragment is emitted exactly when the session's model
changed.

## Constraints and non-goals

- No orchd behavioral change: assembly stays on the hostd
  `PromptAssemblyPort`; the frozen snapshot flows through
  `AgentRunInput.prompt_resources` unchanged.
- No new crate dependencies.
- The three new blocks are `RunDynamic`: they must never invalidate the
  stable cache prefix (`semantic_prefix_digest`).
- Non-goals: world-state diffing (F-04), inter-agent fragments (F-10),
  token/rollout-budget fragments (M1), mention-syntax parsing.

## Proposed design

### Ownership

- **hostd domain** owns fragment shape and capture. `PromptSnapshotOptions`
  gains optional run facts, a host environment capture, and the previous
  model; `snapshot_prompt_resources` emits the three new blocks.
- **hostd session domain** (`HostState`/`SessionState`) owns the last model
  used by each session (hostd is authoritative for user-visible state). A new
  accessor returns the previous model and records the current one.
- **piko-protocol** owns only the typed DTOs that cross the boundary. The
  new blocks reuse the existing `PromptBlock` and `PromptBlockKind`
  (`Context`, `Environment`); no new wire types are required beyond an
  assembly-version bump.

### Data flow

```text
submit.rs (hostd application)
  │  settings.default_model ──► HostState::record_turn_model(session, model)
  │                               (returns previous model)
  ▼
PromptSnapshotOptions {
  session_id, agent_instance_id, operation_id, model,
  previous_model, continuation,
  environment: EnvironmentSnapshot::capture(),
  ...existing fields
}
  ▼
snapshot_prompt_resources ──► PromptResourceSnapshot
  • state.run            (when any durable fact present)
  • environment.host     (when any captured fact present)
  • context.model-switch (when previous_model != model, both known)
  ▼
AgentRunInput.prompt_resources ──► orchd PromptAssemblyPort ──►
assemble_agent_run_prompt (unchanged semantics; new blocks flow through)
```

### Fragment builders (hostd `domain/prompts`)

New file `environment.rs`:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnvironmentSnapshot {
    pub os: Option<String>,
    pub arch: Option<String>,
    pub shell: Option<String>,
    pub hostname: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
}

impl EnvironmentSnapshot {
    pub fn capture() -> Self { /* whitelisted consts + env vars */ }
}
```

`capture()` reads only:

- `std::env::consts::OS`, `std::env::consts::ARCH`
- `SHELL`
- `HOSTNAME` (fallback `COMPUTERNAME` on Windows)
- `chrono::Local::now().offset()` formatted as `+HH:MM`/`-HH:MM`
- `LANG` (fallback `LC_ALL`)

Values are trimmed; empty values are `None`. Nothing else from the process
environment is read.

`PromptSnapshotOptions` grows:

```rust
pub struct PromptSnapshotOptions {
    pub operator_instructions: Vec<String>,
    pub cwd: PathBuf,
    pub context_files: Vec<ContextFile>,
    pub skills: Vec<Skill>,
    pub prompt_templates: Vec<PromptTemplate>,
    // new, all optional / defaulted:
    pub session_id: Option<String>,
    pub agent_instance_id: Option<String>,
    pub operation_id: Option<String>,
    pub model: Option<String>,
    pub previous_model: Option<String>,
    pub continuation: bool,
    pub environment: EnvironmentSnapshot,
}
```

Block emission in `snapshot_prompt_resources` (order after the catalogs,
before `environment.run`):

1. `state.run` — `PromptBlockKind::Context`, authority `None`, trust
   `Trusted`, source `run-state` / `hostd/session`, scope `RunDynamic`.
   Content lines in fixed order: `session_id`, `agent_instance_id`,
   `operation_id`, `run_kind` (only when `continuation`), `model`. Emitted
   when at least one fact is present.
2. `environment.host` — `PromptBlockKind::Environment`, authority `None`,
   trust `Trusted`, source `environment` / `host`, scope `RunDynamic`.
   Content lines in the `EnvironmentSnapshot` field order, omitting `None`.
   Emitted when at least one fact is present.
3. `context.model-switch` — `PromptBlockKind::Context`, authority `None`,
   trust `Trusted`, source `environment` / `model-switch`, scope `RunDynamic`.
   Emitted only when `previous_model` and `model` are both `Some` and differ.
   Content is a fixed `<model_switch>` wrapper naming both models.

The existing `block()` helper already trims content and computes stable
content digests, so the new fragments get digests for free. `cache_segments`
already excludes `RunDynamic` from prefix segments — no cache-plan change.

### Session model continuity (hostd `domain/sessions`)

`SessionState` gains `last_model: Option<String>` (default `None`). A new
`HostState` method:

```rust
/// Returns the previously recorded model for the session (if any) and
/// records the current model. A `None` model does not overwrite history.
pub fn record_turn_model(
    &mut self,
    session_id: &str,
    model: Option<&str>,
) -> Result<Option<String>, ProtocolError>;
```

`submit.rs` calls it once per accepted root turn, immediately before building
the snapshot, using `settings.get_default_model()`. The returned previous
model is passed as `previous_model`. `last_model` is in-memory for this slice
(durable per-session model tracking is a follow-up owned with F-01/F-04
persistence).

### Assembly version

`AGENT_RUN_PROMPT_ASSEMBLY_VERSION` bumps `2 → 3`: the block catalog shape
changed (new block ids), so old frozen prompts must not be replayed as if
they were assembled under the new catalog.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` 2 → 3 only; no new DTOs |
| `piko-hostd` | `domain/prompts/environment.rs` (new), `types.rs` options, `build.rs` emissions, `SessionState.last_model`, `HostState::record_turn_model`, `submit.rs` wiring |
| `piko-orchd` | None |
| `piko-llmd` | None |
| `piko-sandbox` | None |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- **Capture failure**: `EnvironmentSnapshot::capture` never fails; each fact
  is independently `None` when unavailable, so a partial capture still
  produces a valid fragment (or omits the block entirely).
- **Missing model**: `model`/`previous_model` `None` only suppress the
  model-switch block; the run prompt is still valid.
- **Session absent**: `record_turn_model` returns the existing
  `ProtocolError::SessionNotFound` path; `submit.rs` already treats session
  lookup errors as terminal.
- **Determinism guarantee**: fixed key order and stable block ids keep the
  source digest and cache plan reproducible; nothing in this slice is
  order- or time-dependent beyond the existing date line.

## Verification

- Unit/integration tests in `packages/hostd/tests/resources.rs`:
  - `state.run` emission with all facts and fixed order.
  - `environment.host` emission with partial capture (omits `None`).
  - Model-switch emitted on change, absent on first run and on no change.
  - RunDynamic scope regression: new blocks never change
    `semantic_prefix_digest`; project context changes still do.
  - Determinism: identical inputs → identical blocks/digests.
  - `run_kind: continuation` vs `initial`.
- Differential test: two-turn model change emits a model-switch block naming
  both models (mirrors the codex-rs fixture shape at the block level).
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.

## Alternatives considered

- **Pass previous model through `PromptAssemblyRequest` and assemble in
  orchd.** Rejected: would add a wire field and force the orchd fallback
  assembler to reason about model continuity, inverting the hostd-authoritative
  rule. The snapshot is already the hostd-owned per-run freeze point.
- **Dump process env into the environment fragment.** Rejected: leaks
  secrets and breaks determinism; the whitelisted capture is fail-closed.
- **Persist `last_model` in `session.json` now.** Deferred within this slice
  (serde-defaulted fields keep v3 files loadable); landed with the F-02
  model-continuity slice ([D-16](D-16-model-continuity.md)), which makes the
  record durable and hostd-authoritative.

## Rollout

1. `EnvironmentSnapshot` + options + block emissions (pure, unit-tested).
2. `SessionState.last_model` + `record_turn_model` + `submit.rs` wiring.
3. Assembly-version bump + cache-scope regression tests.
4. PRD/README/roadmap status updates; verification evidence `V-03`.
