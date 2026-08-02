# V-03: F-03 prompt-assembly fragment-breadth acceptance evidence

> Date: 2026-08-02
> Fixture: `piko-hostd` integration tests (`tests/resources.rs`) and
> `domain::sessions` unit tests
> Environment: macOS, `cargo test -p piko-hostd`

## Reproduction

```bash
cargo test -p piko-hostd --test resources
cargo test -p piko-hostd --lib sessions::tests::record_turn_model_reports_previous_model_and_tracks_continuity
```

The resources tests drive the real hostd prompt assembler
(`snapshot_prompt_resources` + `assemble_agent_run_prompt`) with explicit
run facts, host captures, and cache-plan assertions; the sessions unit test
drives hostd-authoritative model continuity directly.

## Result

All F-03 acceptance criteria for this slice pass:

- **World-state fragment (superseded by F-04 slice 2)**: the run facts were
  originally emitted as a frozen `state.run` block; F-04 slice 2 (D-17)
  moved them to a retained transcript Context message (full on the first
  run, diff afterwards). The fact set and fixed line order are unchanged
  and now unit-tested in `domain/prompts/world_state.rs` (V-17).
- **Environment-context fragment**: a snapshot with host facts emits exactly
  one `environment.host` block (kind `Environment`) with the captured facts
  in fixed key order; unavailable facts are omitted.
- **Empty state**: a default snapshot omits the environment and model-switch
  blocks; world-state is injected by the turn submit path, not the prompt
  snapshot.
- **Model-switch fragment**: `context.model-switch` is emitted exactly when
  the previous and current models are both known and differ, naming both
  models inside a `<model_switch>` wrapper; first runs and unchanged-model
  runs emit none.
- **Cache safety**: the RunDynamic blocks (`environment.host`,
  `context.model-switch`) — changing model or host facts changes
  `source_digest` but never
  `semantic_prefix_digest`; changing project context still invalidates the
  prefix (regression guard).
- **Determinism**: identical inputs produce byte-identical block content and
  digests; fixed key order verified by exact string assertions.
- **Model continuity**: `record_turn_model` returns the previous session
  model and records the current one; a `None` model never erases history
  (unit test).
- **Assembly version**: `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` is `4` (was `3`
  at F-03 landing; bumped when `state.run` left the block catalog in
  F-04 slice 2), so old frozen prompts are not replayed under the new block
  catalog.

## Differential validation

The model-switch behavior mirrors the codex-rs fixture
`model_change_appends_model_instructions_developer_message` at the block
level: the second turn after a model change carries a dedicated model-switch
notification naming both models, and no such notification exists on the
first turn.

## Invariants

- New fragments are Trusted, authority `None`, and never part of the stable
  cache prefix.
- Capture is whitelisted (OS/arch consts, `SHELL`, hostname, timezone,
  locale); no process environment or credentials are ever included.
- Missing facts fail closed (block omitted), never fabricated.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test --workspace` passes (llmd gateway-retry tests require
  network-capable execution; they pass outside the restricted sandbox).
