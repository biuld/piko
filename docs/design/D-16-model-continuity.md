# D-16: Unified model-continuity management

> Status: accepted
> Implements: [F-02](../features/F-02-model-gateway.md) (model-continuity
> slice) and extends [F-03](../features/F-03-prompt-assembly.md)

## Goal

Turn model selection and model change into one hostd-owned management
surface: a single resolution point (the model the runner executes with), a
single durable per-session record, and one change predicate that drives both
the prompt model-switch fragment and the durable `ModelChange` timeline
marker.

## Constraints and non-goals

- No schema-version migration: `session.json` stays v3; new fields are
  serde-defaulted so old files load unchanged.
- No orchd change: per-agent model overrides (`AgentSpec.model`) are not yet
  resolved by the runtime and remain out of scope; the slice records the
  resolved default the runner executes with.
- Config-change live event (`ModelEvent::ConfigChanged`) and runner rebuild
  semantics are preserved.
- Non-goal: per-turn model *history* (only the last executed model is
  recorded), prewarm, sticky routing.

## Proposed design

### Ownership and data flow

```text
settings + registry ──► build_orch_agent_runner ──► runner + active_model
                                                     (provider + model id)
                                                            │
                                              turn submission (submit.rs)
                                                            │
                                          record_turn_model(session, active)
                                                     │            │
                                           previous != current?   ──► prompt snapshot
                                                     │                 model-switch block
                                          durable session record
                                          (session.json lastModel) ──► JSONL ModelChange
                                                                        timeline entry
```

- **Resolution**: `build_orch_agent_runner` returns the resolved
  `SessionModelRef` alongside the runner. `HostApp.active_model` stores it
  and is refreshed at startup (`jsonl_stdio.rs`) and on config change
  (`ModelRunnerObserver`).
- **Recording**: `submit.rs` calls `HostState::record_turn_model(session,
  active_model)` once per accepted turn. `SessionState.last_model` is the
  in-memory record.
- **Durability**: `SessionManifest.last_model` (serde-defaulted) persists the
  record; `JsonlSessionRepository::set_last_model` writes it and
  `load_session_dir` restores it, so a restarted host preserves continuity.
- **Predicate + consumers**: `previous != current` (provider or id) is
  computed once in `submit.rs`. It gates the `context.model-switch` prompt
  block (via `PromptSnapshotOptions.previous_model`) and appends exactly one
  `ModelChange` timeline entry through `append_config_metadata` at turn
  submission.
- **Config path**: `ModelRunnerObserver` keeps rebuilding the runner and
  emitting `ModelEvent::ConfigChanged`; `SessionStorageObserver` stops
  writing model markers at config time (thinking-level markers stay).

### Types

- `domain::sessions::SessionModelRef { provider, model_id }` — the session
  model record; shared by domain, storage DTOs, and the composition root.
- `SessionState.last_model: Option<SessionModelRef>` replaces the previous
  `Option<String>`; `SessionManifest.last_model` mirrors it with
  `#[serde(default)]`.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | None |
| `piko-hostd` | `SessionModelRef` + `SessionState.last_model`; `SessionManifest.last_model`; `set_last_model` on `SessionRepositoryPort` + adapters; `HostApp.active_model` + `set_active_model`; `build_orch_agent_runner` returns the resolved model; startup/config observers refresh it; `submit.rs` records + derives prompt/marker; config no longer writes timeline markers at settings time |
| `piko-orchd` | None |
| `piko-llmd` | None |
| `piko-sandbox` | None |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- **Unconfigured model**: `active_model` is `None`; `record_turn_model`
  records nothing, no fragment, no marker, no manifest write (fail closed).
- **Storage failure**: `set_last_model`/`append_config_metadata` errors are
  swallowed at submit time (in-memory record still drives the prompt); the
  next turn re-derives the same state. Config-time storage errors keep their
  existing logged behavior.
- **Restart**: the durable record restores continuity; a model switch that
  happened before the restart is not re-marked.
- **Non-root turns**: every accepted turn records the session model; a
  subagent turn with a future per-agent override will record that model
  (per-session record remains single-valued by design).

## Verification

- Unit: `record_turn_model` continuity (same model, model change, provider
  change, `None` never erases).
- Integration (`tests/model_continuity.rs`): two turns with a mock runner —
  first prompt has no model-switch and shows model-a; second prompt has one
  model-switch naming both; exactly one durable `ModelChange` entry; the
  record round-trips through `session.json` and is preserved by a fresh host;
  unconfigured active model emits no fragments.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.

## Alternatives considered

- **Derive continuity from config-time settings comparison.** Rejected: it
  records what the settings *say*, not what the runner *executes*, and it
  diverges from the prompt fragment's predicate. Kept only as the live event.
- **Persist per-turn model history.** Deferred: the prompt fragment and
  marker need only the last executed model; history is an observability
  concern (F-15 usage/rollout records).
- **Resolve the model inside `submit.rs` from `model_registry`.** Rejected in
  favor of returning it from `build_orch_agent_runner`: the record must equal
  the runner's own resolution, and the registry instance used at build time
  is the authoritative one.

## Rollout

1. `SessionModelRef` + domain record + manifest field + storage methods.
2. `active_model` + `build_orch_agent_runner` return value + observer wiring.
3. `submit.rs` record + derived prompt/marker; config path cleanup.
4. Integration/durability tests; PRD/roadmap/verification updates.
