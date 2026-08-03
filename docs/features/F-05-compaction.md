# F-05: Compaction — budget windows, inline compact, and model-visible context tools

> Status: implemented (slice 1); implemented (slice 2, per-model growth
> defaults)
> Priority: P0
> Source evidence: codex-rs `core/src/compact.rs`,
> `core/src/compact_token_budget.rs`, `core/src/compact_model_fallback.rs`,
> `core/src/compact_remote*.rs`, `core/src/state/auto_compact_window.rs`,
> `core/src/session/token_budget.rs`,
> `core/src/tools/handlers/{get_context_remaining,new_context_window}.rs`

## Summary

Compaction keeps a long session inside the model's context window by
rewriting the projected history: old messages are summarized into a
structured checkpoint entry (or dropped for a fresh window), recent messages
are retained, and hostd stays authoritative for the durable transcript.
This slice makes compaction a *budget-window* process instead of a single
threshold: auto-compact fires at most once per window, only after enough new
context has accumulated since the last compaction, never concurrently, and
with a recorded reason. It adds a token-budget inline compact
(`new_context_window` without summarization), a configurable summarizer model
with fallback to the default model (piko's adaptation of "remote
compaction"), the two model-visible context tools (`get_context_remaining`
and `new_context_window`), and a `[transcript]` setting for the F-04
truncation cap. Slice 2 makes the hysteresis guard model-aware: when
`min_growth_tokens` is not configured, it derives from the resolved model's
context window as a fraction instead of a fixed default (closing the F-05
open question).

## Problem

1. **The auto-compact trigger is a single naive threshold.** hostd compacts
   whenever `estimate + reserve > window`. There is no notion of a compaction
   window, so a session that hovers near the waterline can be rewritten every
   turn, and two turns racing through the same session can both run
   compaction. Nothing records which trigger fired or how many tokens were
   in play, so the decision is not auditable.
2. **Only a user can act when the window is nearly full.** F-04's budget
   preflight rejects an over-budget turn with "compaction required", but the
   model itself cannot see how much context remains or request a fresh
   window. Long autonomous runs stall on a user action that the model could
   take itself.
3. **Summarization always uses the default model, and failure is silent.**
   Compaction cannot be pointed at a cheaper or faster model, and if the
   summarizer call fails, `compact_session_if_needed` abandons compaction
   without a fallback attempt or any trace.
4. **The truncation cap is a hardcoded constant.** F-04 shipped
   `DEFAULT_MAX_TOOL_OUTPUT_TOKENS` with the settings wiring deferred; there
   is no way for an operator to tune how much tool output the model sees.

## User journeys

1. A long agentic session runs for hours. hostd auto-compacts when the
   branch crosses the window waterline, records the window number and reason
   on the checkpoint entry, and does not compact again until the session has
   grown materially past the retained baseline — no thrashing, no concurrent
   rewrites.
2. An agent realizes context is running low mid-turn. It calls
   `get_context_remaining`, sees the estimated headroom, and asks for a fresh
   window via `new_context_window`; hostd drops the old history (keeping the
   latest user message) and the run continues without a user round-trip.
3. An operator configures `[compaction] summarizer-model` to a fast model
   and `[transcript] max-tool-output-tokens` to their preferred cap. If the
   summarizer fails, compaction retries with the default model instead of
   silently skipping.

## In scope

- Budget-window auto-compact trigger with hysteresis, in hostd: trigger at
  the high waterline (`window − reserve`), a per-session rearm baseline
  (estimated tokens retained by the last compaction), a minimum-growth guard
  before the next auto-compact in the same window, a per-session window
  counter, a pending guard against concurrent compactions, and a recorded
  trigger reason on the checkpoint entry.
- Inline compact: the existing `session.compact` (manual summarize-and-keep)
  plus a token-budget mode that starts a fresh window without summarization,
  keeping the most recent user message.
- Model-visible tools in orchd: `get_context_remaining` reports the
  estimated tokens left before the window fills; `new_context_window`
  requests the host-side token-budget compact through a callback, keeping
  hostd authoritative for the rewrite.
- Summarizer model override and fallback: `[compaction] summarizer-model` /
  `summarizer-provider` select the model used for the summary call; on
  failure hostd retries once with the default model and logs the fallback.
- Truncation-cap settings wiring: `[transcript] max-tool-output-tokens`
  reaches the orchd model view that F-04 established.
- Settings defaults that preserve today's behavior for unconfigured
  sessions, and wire-compatible protocol changes.
- Slice 2: per-model growth defaults — `[compaction] min-growth-fraction`
  derives the hysteresis guard from the resolved model's context window when
  `min_growth_tokens` is unset; an explicit `min_growth_tokens` always wins.

## Out of scope

- Provider-side "remote compaction" (codex-rs Responses `compact` /
  compaction v2): rejected — piko has no cloud session or server-side
  compaction consumer; the summarizer-model override is the piko-native
  adaptation (see Fusion decisions).
- Token-budget reminder fragments injected into the prompt (codex-rs
  `token_budget_context`); the model-visible tools cover the need in piko.
- Compaction hooks (pre/post compact) and compaction events beyond the
  existing `SessionReconciled` flow.
- Compaction of non-root AgentInstances (unchanged; the root shard owns the
  session tree projection).
- World-state diffing across turns (owned by F-04 follow-ons).

## Behavior and states

### Budget-window trigger (hostd)

Each session carries a compaction window state: `window_number`,
`rearm_tokens` (the estimated tokens retained by the most recent
compaction, if any), and `pending`.

```text
decision(estimate, window, settings, state):
  disabled                    → Disabled
  estimate + reserve <= window → Hold (under high waterline)
  state.rearm_tokens is None  → Trigger (first window)
  estimate − rearm >= min_growth → Trigger (new window of growth)
  else                        → Hold (hysteresis)
```

- The high waterline is `context_window − reserve_tokens` (unchanged).
- The rearm baseline is recorded at the moment a compaction lands, using the
  same estimator as F-04 so the trigger and the transcript accounting can
  never diverge.
- `min_growth_tokens` defaults to `16_384` and is configurable; it prevents
  flapping around the waterline.
- `pending` is set while summarization/rewrite runs and cleared on
  completion, so a second turn cannot start a concurrent rewrite.
- Successful compactions advance `window_number` and record
  `{"trigger", "windowNumber", "tokensBefore", "tokensAfter"}` in the
  checkpoint entry's `details`.

### Inline compact (hostd)

`session.compact` keeps its current behavior (summarize and keep the recent
tail) and gains a mode:

- `Summarize` (default): summarize the dropped prefix into a structured
  checkpoint and keep the recent tail (existing flow).
- `NewContextWindow`: drop the prefix without calling the model, keep the
  most recent user message, and append a checkpoint whose summary is the
  fixed message "A new context window was started without summarizing
  conversation history." If the branch has no user message, the command
  fails closed.

Both modes emit the existing `SessionReconciled` rewrite so clients rebuild
their view.

### Model-visible context tools (orchd)

- `get_context_remaining` — no arguments. Returns the estimated tokens left
  before the window fills: `context_window − fixed − transcript` using the
  same budget basis as the F-04 preflight for the current step
  (`{"tokens_left": N}`), or `{"tokens_left": null}` when the window is not
  resolvable. It is read-only and never triggers a rewrite.
- `new_context_window` — no arguments. Returns the fixed confirmation
  message; the tool provider routes the request to hostd through a callback
  that runs the token-budget compact on the root shard. If the session is not
  eligible (no user message to retain) the tool fails with an explicit,
  non-retryable error.
- Both tools are always registered in the single-agent tool set; hostd's
  `active_tool_names` filter governs exposure like every other tool.

### Summarizer model override and fallback (hostd)

`[compaction] summarizer-model` / `summarizer-provider` (both optional)
select the model for the summarization call. When unset, the default model
is used. When the summarizer call fails:

1. If a non-default summarizer was configured, retry once with the default
   model and log a warning (`compaction.summarizer_fallback`).
2. If the default model itself fails, abort compaction for this turn (no
   rewrite), leaving the session unchanged.

### Truncation-cap settings wiring

`[transcript] max-tool-output-tokens` (default `24_000`, matching F-04's
documented constant) is resolved by hostd and carried into the orchd run so
the model view uses the configured cap. Unconfigured sessions behave exactly
as today.

### Per-model growth defaults (slice 2)

The hysteresis guard prevents auto-compact from flapping around the
waterline, but a fixed `16_384` default does not scale with the model: on an
8k-window model the guard can never be satisfied, so after the first
compaction the session would never re-trigger; on a 1M-window model the
guard is a rounding error. Slice 2 makes the default model-aware:

```text
effective_min_growth(config, window):
  config.min_growth_tokens is Some(n)  → n          (explicit override)
  window > 0 and min_growth_fraction is Some(f) →
      max(1, round(window × f))
  else                                 → 16_384     (windowless fallback)
```

- `[compaction] min-growth-fraction` (default `0.125`, i.e. 12.5% ≈ the
  documented `16_384` default at a 128k window) is the ratio of the resolved
  context window used as the growth guard when `min_growth_tokens` is unset.
- The window is the same `resolved_model_context_window()` used for the
  waterline check, so the trigger and the guard always share one basis.
- An explicitly configured `min_growth_tokens` is used verbatim (today's
  behavior); the fraction only changes the *default*.
- When the window cannot be resolved (model lookup fails, or a force-compact
  callback without a window), the previous constant `16_384` fallback
  applies, preserving behavior for unconfigured sessions.

### Error and cancellation states

- Summarizer failure: compaction skipped, session unchanged, warning logged
  (after the single fallback attempt).
- No eligible cut point in `NewContextWindow` mode: command fails with a
  clear error; no rewrite.
- Compaction already pending: a new request returns without rewriting and
  without erroring (the pending run owns the rewrite).
- Restart/resume: window state is derived from the last checkpoint in the
  tree; no separate durable file is needed.

## Acceptance criteria

- [ ] Unit evidence: the trigger decision respects `enabled`, the high
      waterline, and the minimum-growth guard; the first compaction
      triggers, and after a recorded rearm baseline the same estimate does
      not re-trigger.
- [ ] Unit evidence: `get_context_remaining` reports
      `window − fixed − transcript` from the same budget basis as the F-04
      preflight, and `null` when the window is unknown.
- [ ] Integration evidence: two racing compacts on one session result in one
      rewrite (pending guard).
- [ ] Integration evidence: `session.compact` with `NewContextWindow` mode
      drops the old prefix, retains the last user message, and emits
      `SessionReconciled` without invoking the summarizer.
- [ ] Integration evidence: a configured summarizer model is used for the
      summary; when it fails, hostd retries with the default model and the
      compaction lands (scripted gateway).
- [ ] End-to-end evidence: a running agent calls `get_context_remaining` and
      receives the expected remaining estimate; calling `new_context_window`
      produces the fresh-window rewrite and the run continues.
- [ ] End-to-end evidence: `[transcript] max-tool-output-tokens` reaches the
      orchd model view (an oversized tool result is truncated at the
      configured cap).
- [ ] Unit evidence (slice 2): `min_growth_tokens` unset + fraction
      configured derives `max(1, round(window × fraction))`; an explicit
      `min_growth_tokens` beats the fraction; window 0 falls back to the
      constant `16_384`.
- [ ] Integration evidence (slice 2): with a default (fraction-only)
      config, a small-window model re-triggers auto-compact after growth of
      the window-derived amount; the trigger decision uses the resolved
      window basis.
- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets --
      -D warnings` clean; `cargo test --workspace` green.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does the window state live? | hostd session state, derived from checkpoint entries on resume | hostd is authoritative for user-visible state; orchd stays transient |
| What prevents thrash? | Rearm baseline + `min_growth_tokens` guard per window | A compaction is only worth a rewrite after real growth; the guard keeps decisions auditable |
| What does `new_context_window` retain? | The most recent user message | Mirrors codex-rs `BeforeLastUserMessage` without duplicating initial-context machinery |
| Default `min_growth_tokens` | Window-derived: `max(1, round(window × 0.125))` when unset (≈ `16_384` at 128k); explicit config wins; `16_384` fallback when the window is unknown | A fixed guard breaks small-window models (never re-triggers) and is a rounding error on huge windows; a fraction scales the guard to the resolved model (slice 2) |
| How does the model learn about context? | Tools, not prompt fragments | piko has no token-budget fragment consumer; tools reuse the F-04 budget basis |
| Summarizer model failure | Fall back once to the default model, then abort | Mirrors codex-rs `compact_model_fallback` without analytics coupling |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `state/auto_compact_window.rs` window ids + prefill baseline | **kept (adapted)** | per-session `window_number` + rearm baseline in hostd state; prefill-vs-server-usage distinction is not needed because hostd estimates the whole branch with the F-04 estimator |
| `compact_token_budget.rs` manual/token-budget compact | **kept (adapted)** | `session.compact { mode: NewContextWindow }` drops history without a model call; the checkpoint entry replaces the fresh-window install so the durable tree stays valid |
| `compact_model_fallback.rs` | **kept (adapted)** | configured summarizer model with one fallback to the default model; telemetry stays on hostd logs |
| `compact_remote.rs` / `compact_remote_v2*.rs` provider-side compaction | **rejected** | requires a cloud/server compaction consumer piko does not have; the summarizer-model override covers the ops need (ADR-002: reject codex-shaped mechanisms with no piko consumer) |
| `get_context_remaining` / `new_context_window` tools | **kept (adapted)** | built-in orchd tools; `new_context_window` routes through a hostd callback so the rewrite stays hostd-owned |
| `token_budget_context` reminder fragments | **rejected (this slice)** | no fragment consumer in piko; the tools satisfy the model-visible need |

## Open questions

None. The per-model default question was resolved in slice 2 (window
fraction with explicit override); token-budget prompt fragments remain
rejected per the Fusion decisions above.

## Reference evidence

- codex-rs `core/src/state/auto_compact_window.rs` (window ids/prefill
  baseline), `core/src/compact.rs` (trigger + lifecycle),
  `core/src/compact_token_budget.rs` (fresh window without summarization),
  `core/src/compact_model_fallback.rs` (fallback retry),
  `core/src/compact_remote*.rs` (rejected mechanism),
  `core/src/tools/handlers/{get_context_remaining,new_context_window}.rs`
  (tool semantics).
- piko `packages/hostd/src/domain/compaction/mod.rs` and
  `packages/hostd/src/application/compaction.rs` (pre-slice summarizer),
  `packages/orchd/src/runtime/execution/budget.rs` (budget basis),
  `packages/orchd/src/domain/transcript/normalize.rs` (truncation cap).
- Slice 2: design [D-18](../design/D-18-compaction-model-defaults.md),
  verification [V-18](../verification/V-18-compaction-model-defaults.md).
