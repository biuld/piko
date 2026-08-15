# ADR-020: Context preflight reserves a bounded output allowance

> Status: accepted
> Date: 2026-08-16

## Context

`enforce_context_budget` reserved the provider's full `max_output_tokens`
twice for reasoning models (`output_reserve` + `reasoning_reserve`). For
`deepseek-v4-flash` (window 1M, max tokens 384K) the fixed overhead reached
~796K, leaving only ~200K of usable transcript. A production turn died with
`ContextBudgetExceeded` at roughly 17% of the provider's real window even
though the largest observed per-step output was 2,558 tokens. The reserve
existed to guarantee an in-flight response never overflows the window, but at
full `max_tokens` it makes the window unusable before any transcript exists.

## Decision

- The preflight reserves a single output allowance capped at 32,768 tokens:
  `min(model.max_output_tokens, OUTPUT_RESERVE_CAP)`.
- Reasoning does not add a second reserve; provider `max_tokens` budgets
  reasoning and completion together on piko's reasoning models, so one capped
  allowance covers both.
- Fail-closed behavior, error fields, and the returned `BudgetEstimate` are
  unchanged; the preflight still rejects a request whose estimated input plus
  reserve exceeds the window.

## Consequences

- A 1M-window reasoning model keeps ~940K tokens for the transcript instead of
  ~200K, so long tool loops survive far longer before compaction is required.
- The reserve is a heuristic: if a model ever produces more than the cap in
  one response, the provider (not the preflight) rejects the overflow. The
  cap is 13x the largest observed single-step output, so this is a safe
  trade-off.
- Small models with `max_output_tokens` below the cap are unaffected.
