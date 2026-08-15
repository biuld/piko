# F-35: Turn budget headroom and steer responsiveness

> Status: implemented (F-35/D-48)
> Priority: P1
> Source evidence: piko production session 75455f6c (2026-08-16); F-01 steer
> admission; F-04 budget preflight

## Summary

Two runtime behaviors keep a long turn productive and answerable. First, the
per-step context preflight reserves a **bounded** allowance for model output
instead of the full model `max_tokens`, and reasoning shares that single
allowance instead of doubling it, so a 1M-token window keeps roughly 940K
tokens for the transcript rather than roughly 200K. Second, a user message
steered into a running turn is **answered before further tool work**: the
model step immediately after a steered message is committed runs without tool
access and with an explicit instruction to reply to that message in text, and
only then does the turn resume its normal tool loop.

## Problem

A production session (2026-08-16, session `75455f6c`, model
`deepseek-v4-flash`) exposed two failures in the same turn:

1. **The budget preflight strangled the turn early.** `enforce_context_budget`
   reserved `max_output_tokens` (384,000) for output **and** another 384,000
   for reasoning, plus a 20,000 safety margin. Fixed overhead was ~796,000
   tokens, leaving ~200K of usable transcript in a 1M window. The model had
   produced only ~38K output tokens (max 2.5K in any step) when the estimate
   crossed the ceiling; the turn died with
   `model context budget exceeded ... compaction required` at roughly 17% of
   the provider's real window, after 72 steps and 107 tool calls, with no
   user-visible answer.
2. **A steered user message did not get answered.** The user asked
   "汇报一下情况" and again "回报情况" while the turn ran. Both messages were
   committed and visible to the next model step, but the model acknowledged
   them in its reasoning and continued calling tools. Nothing in the runtime
   forced a reply, so the interrupt was buried under seven minutes of tool
   work.

## User journeys

1. An agent runs a long investigative turn. The user sends a mid-turn message
   asking for a status report. The runtime commits the message at the next
   model-step boundary (unchanged), the next step produces a text answer to
   that message without tool calls, and the turn then resumes its tool work.
2. An agent is deep in a tool loop. Its transcript grows past 200K tokens on
   a 1M-window reasoning model without the turn being killed early; the
   preflight only fails closed when the transcript is genuinely close to the
   window.

## In scope

- Bounded output/reasoning reserve in the orchd per-step context preflight.
- Respond-first model step after a steered user message: tools disabled for
  that step, explicit reply instruction, then normal loop continuation.

## Out of scope

- Mid-turn auto-compaction, step caps, or stall detection (future work; the
  failing session also lacked those, but they are separate features).
- Changes to steer admission, follow-up queueing, or cancellation semantics
  (F-01).
- Transcript truncation or `max_tool_output_tokens` (F-04).
- Provider behavior when a model returns no text for a respond-only step.

## Behavior and states

### Budget preflight

- Fixed overhead = prompt + tools + safety margin + **one** output reserve.
- The output reserve is `min(model.max_output_tokens, OUTPUT_RESERVE_CAP)`
  where `OUTPUT_RESERVE_CAP` is a module constant (32,768 tokens). Reasoning
  consumes the same reserve; it is not added a second time.
- Failure messages keep the existing fields
  (`estimated request`, `fixed`, `transcript`, `context_remaining`, `window`).
- Success returns the estimate for telemetry, unchanged.

### Respond-first steering

- A steered message is committed at the next model-step boundary in submission
  order (unchanged, F-01).
- After a steer is committed, the **next** model step is a respond-only step:
  `tool_choice = None` regardless of the configured tool choice, plus one
  instruction block that tells the model to answer the newly delivered user
  message directly in text.
- The respond-only step does not commit tool calls (tools are disabled); its
  assistant message is committed like any other.
- After the respond-only step completes, the turn continues with normal model
  steps (tools re-enabled). A second steer commits and triggers another
  respond-only step at the next boundary.
- If the provider nevertheless returns tool calls for a respond-only step,
  the step fails closed rather than executing tools.

## Acceptance criteria

- [ ] Unit: `enforce_context_budget` with a large `max_output_tokens` and
      reasoning enabled reserves at most `OUTPUT_RESERVE_CAP` total for
      output+reasoning; a transcript that fits under the capped ceiling is
      accepted.
- [ ] Unit: the failure message still reports
      `estimated request/fixed/transcript/context_remaining/window`.
- [ ] Agent-level: a running turn that receives a steer issues its next model
      request with `tool_choice = None` and the reply instruction block,
      commits the text answer, and issues subsequent requests with tools
      enabled again.
- [ ] The preflight accepts the real failing session's shape (1M window,
      384K max tokens, ~208K transcript estimate) instead of failing.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| How much output headroom should the preflight reserve? | Bounded constant 32,768 tokens, not full `max_tokens` | Observed per-step output was ≤2.5K; 32K is generous headroom and keeps ~94% of a 1M window usable. |
| Does reasoning need a separate reserve? | No; one reserve covers reasoning + completion | Provider `max_tokens` budgets reasoning and completion together (deepseek catalog). |
| Does the turn end after answering a steer? | No; it resumes normal steps | F-01 steers redirect a running turn; ending the turn would turn course corrections into restarts. |

## Fusion decisions (codex-rs)

codex-rs steers input into a running turn without a forced-response contract
(F-01 models the admission, not the reply priority). This feature keeps the
admission modeling and adds a piko-native reply guarantee.

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Steer delivery into a running turn | kept (adapted) | F-01 admission unchanged; F-35 adds respond-first step in orchd. |
| Token budget fail-closed preflight | kept (adapted) | F-04 preflight stays; reserve is bounded so the window is not wasted. |

## Open questions

1. Should a respond-only step that produces no text (thinking-only) be
   retried or surfaced as an error? Current decision: surface as normal turn
   output; enforcement beyond `tool_choice = None` is provider-dependent.

## Reference evidence

- Session `75455f6c` journal:
  `~/.piko/agent/sessions/cwd_Users-biu-Projects-piko/1786825587015_75455f6c-8f2e-4484-ac6e-2434c9df8562/events/00000000000000000001-open.jsonl`
  (execution_finished reports `model context budget exceeded`; user steers at
  04:31:53 and 04:33:47 were acknowledged but not answered).
- `packages/orchd/src/runtime/execution/budget.rs` (current double reserve).
- `packages/orchd/src/runtime/execution/actor/run.rs` (step loop and steer
  commit points).
