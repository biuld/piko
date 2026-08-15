# V-47: Turn budget headroom and steer responsiveness — verification

> Implements: [F-35](../features/F-35-turn-budget-headroom-and-steer-responsiveness.md)
> Design: [D-48](../design/D-48-turn-budget-headroom-and-steer-responsiveness.md)
> Decisions: [ADR-020](../decisions/ADR-020-bounded-output-reserve.md),
> [ADR-021](../decisions/ADR-021-respond-first-steer-steps.md)

## Production evidence (session `75455f6c`, 2026-08-16)

The failing turn recorded `execution_finished` with:

```text
model context budget exceeded: estimated request=1005030, fixed=796468,
transcript=208562, context_remaining=0, window=1000000; compaction required
```

`fixed=796468` decomposed as output reserve 384,000 + reasoning reserve
384,000 + margin 20,000 + prompt/tools ~8.5K. Provider-reported input at the
last step was 174,358 tokens (~17% of the window). With the capped reserve the
same shape now passes: fixed ≈ 52.8K, context remaining ≈ 747K.

The same turn received two steered user messages ("汇报一下情况" at 04:31:53,
"回报情况" at 04:33:47). Both were committed and acknowledged in the model's
reasoning, but no text answer was ever produced; the turn kept calling tools.

## Automated verification

### Budget preflight (unit, `piko-orchd` lib)

- `context_budget_caps_output_reserve_for_large_max_tokens_with_reasoning`:
  window 1M, `max_output_tokens` 384K, reasoning enabled → fixed ≈ 52.8K
  (capped 32,768 + margin 20,000 + prompt/tools), remaining > 900K.
- `context_budget_accepts_long_transcript_shape_from_production_turn`: ~200K+
  transcript estimate in a 1M window accepted (failing session shape).
- `context_budget_failure_message_reports_budget_fields_and_reasoning_flag`:
  fail-closed message keeps `prompt/tools/output/reasoning/margin/window` and
  adds `reasoning_enabled`.
- Existing preflight tests still pass (small windows, small reserves
  unchanged).

### Respond-first steer (integration, `agent_runtime` test binary)

`steered_message_is_answered_before_further_tool_work`:

1. A tool-loop turn blocks inside a test tool.
2. A steer (`AgentInputDelivery::Auto`) is queued; receipt disposition is
   `Queued` (F-01 boundary semantics).
3. The tool releases; the next captured `InferenceRequest` has
   `tool_choice == None` and a `steer.respond` instruction block; the canned
   text answer is committed.
4. The following request has tools enabled again (`tool_choice != None`).

## Acceptance criteria

- [x] Unit: capped reserve for large `max_output_tokens` with reasoning.
- [x] Unit: failure message fields retained.
- [x] Agent-level: steer → respond-only step → tools re-enabled.
- [x] The failing session's budget shape is accepted.
