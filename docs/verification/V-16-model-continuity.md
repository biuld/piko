# V-16: F-02 model-continuity slice acceptance evidence

> Date: 2026-08-02
> Fixture: `piko-hostd` integration tests (`tests/model_continuity.rs`),
> `piko-hostd` `domain::sessions` unit tests
> Environment: macOS, `cargo test -p piko-hostd`

## Reproduction

```bash
cargo test -p piko-hostd --test model_continuity
cargo test -p piko-hostd --lib sessions::tests::record_turn_model_reports_previous_model_and_tracks_continuity
```

The integration test drives the real hostd submit path (`ChatSubmit` →
snapshot → durable record → timeline marker) with a capturing mock runner and
a real JSONL session repository; the unit test drives the continuity
predicate directly.

## Result

All F-02 model-continuity acceptance criteria pass:

- **Resolved record**: `build_orch_turn_runner` returns the resolved
  provider+model, `HostApp.active_model` stores it, and the session record
  round-trips through `session.json` (`lastModel`); a fresh host loading the
  same directory restores `last_model = anthropic/model-b`.
- **Single predicate, single marker**: two turns on the same model produce no
  `ModelChange` marker and no model-switch fragment; a model change
  (including a provider change with the same id — unit test) produces exactly
  one durable `ModelChange` entry and one `context.model-switch` fragment
  naming both models.
- **Config path**: `ModelEvent::ConfigChanged` + runner rebuild are
  preserved; session timeline markers are no longer written at config time
  (they are execution facts).
- **Fail closed**: with no active model, a turn records nothing, and the
  frozen prompt carries no `model:` line and no model-switch fragment.
- **Prompt integration (F-03 regression)**: first run's `state.run` shows the
  resolved model and no switch block; second run's snapshot carries the
  switch block (cache-scope and determinism guarantees from V-03 hold).

## Invariants

- The recorded model equals the runner's resolved model, not the raw
  settings string.
- Durable continuity survives restart; a pre-restart switch is not re-marked.
- Unconfigured state never fabricates a model or a change.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test --workspace` passes (llmd gateway-retry tests require
  network-capable execution; they pass outside the restricted sandbox).
