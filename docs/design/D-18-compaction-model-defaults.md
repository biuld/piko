# D-18: Per-model compaction growth defaults

> Status: accepted
> Implements: [F-05](../features/F-05-compaction.md) (slice 2)

## Goal

Make the F-05 hysteresis guard (`min_growth_tokens`) scale with the resolved
model instead of a fixed `16_384` default: when the setting is unset, derive
it as a fraction of the same context window that drives the waterline check.
An explicit configuration keeps winning verbatim, and a constant fallback
preserves today's behavior when the window cannot be resolved.

## Constraints and non-goals

- hostd stays authoritative for user-visible state and settings (ADR-002):
  the resolution happens in the hostd application layer that already
  assembles `domain::compaction::CompactionSettings` from config.
- The fraction is a *default* derivation, not a new trigger mode: the
  budget-window decision (`compact_trigger`) keeps its exact semantics.
- The window basis is the existing `resolved_model_context_window()`
  (default-model resolution), so the waterline and the guard can never
  diverge. Switching the waterline to the per-session active model is a
  separate F-02 continuity concern and out of scope here.
- Non-goals: token-budget prompt fragments (already rejected in F-05),
  per-model config tables, TUI settings surfaces (they expose only
  enabled/reserve/keep today), and protocol wire changes (hostd-only
  setting).

## Proposed design

### Config (`domain/config/settings.rs`)

`CompactionSettings` (config struct) gains:

```rust
/// Ratio of the resolved context window used as the hysteresis guard when
/// `min_growth_tokens` is unset (default 0.125).
pub min_growth_fraction: Option<f64>,
```

`merge_compaction` merges it like the other fields
(`overrides.min_growth_fraction.or(base.min_growth_fraction)`).
`default_settings()` moves `min_growth_tokens` to `None` and ships
`min_growth_fraction: Some(0.125)`, so new/legacy configs without an
explicit guard pick up the window-derived default.

`get_compaction_settings()` keeps returning the windowless constant
`16_384` fallback for its callers (no window context in scope); the
window-aware derivation lives one layer up where `context_window` exists.

### Domain (`domain/compaction/mod.rs`)

A pure function owns the derivation so the trigger tests and the application
share one definition:

```rust
pub const DEFAULT_MIN_GROWTH_TOKENS: u64 = 16_384;
pub const DEFAULT_MIN_GROWTH_FRACTION: f64 = 0.125;

pub fn min_growth_default(context_window: u64, fraction: Option<f64>) -> u64 {
    match fraction {
        Some(fraction) if context_window > 0 => {
            ((context_window as f64) * fraction).round().max(1.0) as u64
        }
        _ => DEFAULT_MIN_GROWTH_TOKENS,
    }
}
```

`fraction` is the effective configured value (or the documented default)
passed in; the function stays a single pure mapping from window to guard.

### Application (`application/compaction.rs`)

`compact_session_if_needed` already receives `context_window` and builds the
domain settings from config in one block. The `min_growth_tokens` line
becomes:

```rust
min_growth_tokens: compaction
    .and_then(|c| c.min_growth_tokens)
    .or_else(|| {
        let fraction = compaction
            .and_then(|c| c.min_growth_fraction)
            .unwrap_or(DEFAULT_MIN_GROWTH_FRACTION);
        Some(min_growth_default(context_window, Some(fraction)))
    })
    .unwrap_or(DEFAULT_MIN_GROWTH_TOKENS),
```

- Force compacts (`session.compact`, `new_context_window` callback) pass
  `context_window: 0`; the derivation then falls back to the constant, and
  since force paths skip `compact_trigger` the value is inert.
- Auto-compact paths pass the resolved window, so the guard and the
  waterline share the same basis.

## Acceptance evidence

- Unit: `min_growth_default` derives `max(1, round(window × f))`, returns
  the constant for window 0, and rounds to at least 1.
- Unit/integration: an explicit `min_growth_tokens` wins over the fraction
  (existing `compact_session_if_needed` resolution with a configured
  override).
- Integration: with the default (fraction-only) config, a resolved 8k
  window produces a re-armable guard (auto-compact re-triggers after
  `1_024` growth), fixing the never-re-trigger case for small windows.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all` clean.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| When does the fraction apply? | Only when `min_growth_tokens` is unset | Explicit operator intent always wins; the slice changes defaults, not overrides |
| Default fraction | `0.125` (12.5% ≈ the documented `16_384` at 128k) | One documented ratio scales the guard to any window and keeps the reference-window default on the same scale as before |
| Where does the window come from? | The existing `resolved_model_context_window()` | Same basis as the waterline check; no new resolution path |
| Windowless fallback | Constant `16_384` | Preserves unconfigured behavior exactly where no window is resolvable |

## Fusion decisions (codex-rs)

codex-rs has no direct analog for a window-relative growth guard; this slice
is a piko adaptation of F-05's own open question and stays inside the
existing budget-window model. No codex-rs mechanism is translated.
