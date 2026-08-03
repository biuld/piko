# V-18: F-05 per-model growth-defaults slice acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-hostd` unit + integration tests
> (`application/compaction.rs` tests module, `domain/compaction` defaults
> tests, `tests/settings.rs`), full workspace suite
> Environment: macOS, `cargo test --workspace` (network-capable execution
> for the `piko-llmd` gateway_retry stub servers), `cargo clippy
> --workspace --all-targets -- -D warnings`, `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib application::compaction
cargo test -p piko-hostd --lib domain::compaction
cargo test -p piko-hostd --test settings
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

The slice-2 integration test drives the real hostd resolution path: a test
provider with an 8k-window model is injected into the `ModelRegistry`, a
durable session is created through `apply_session_create`, a large branch is
committed to state, and `compact_session_if_needed` is invoked exactly like
the turn-succeeded auto-compact path.

## Result

All F-05 slice 2 acceptance criteria pass:

- **Unit — window-fraction derivation**: `min_growth_default(128_000,
  Some(0.125)) == 16_000`; sub-token fractions floor at one token; window 0
  and no fraction both fall back to the constant `16_384`
  (`domain::compaction::defaults_tests`).
- **Unit — resolution precedence**: `effective_min_growth_tokens` returns an
  explicit `min_growth_tokens` verbatim, derives `max(1, round(window ×
  fraction))` when unset (including with the documented default fraction),
  and falls back to the constant for a windowless resolution (force-compact
  callback) (`application::compaction::tests`).
- **Integration — the guard scales to the resolved model**: with a
  fraction-only default config (`min_growth_tokens` unset, fraction `0.125`,
  reserve `1_024`, keep-recent `7_000`), the resolved 8k window is used for
  the waterline; the first auto-compact triggers past the waterline and
  records rearm `9_000`; a 400-token growth past rearm holds (below the
  derived `1_024` guard); a 1_200-token growth re-triggers and lands window
  number 2. The old fixed `16_384` default could never re-trigger on this
  window (`window_fraction_guard_scales_retrigger_to_resolved_model`).
- **Regression**: `tests/settings.rs` defaults/merge assertions are green
  with the new `min-growth-fraction` field; the existing F-05 budget-window
  and compaction-reconcile integration suite (6 tests) stays green.
- `cargo test --workspace` green across all crates; `cargo clippy
  --workspace --all-targets -- -D warnings` and `cargo fmt --all` clean.
