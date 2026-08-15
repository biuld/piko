# D-48: Turn budget headroom and steer responsiveness

> Status: accepted
> Implements: [F-35](../features/F-35-turn-budget-headroom-and-steer-responsiveness.md)
> Decisions: [ADR-020](../decisions/ADR-020-bounded-output-reserve.md),
> [ADR-021](../decisions/ADR-021-respond-first-steer-steps.md)

## Goal

Deliver the two F-35 behaviors in `piko-orchd`:

1. `enforce_context_budget` reserves a bounded output allowance (one reserve,
   capped) so a 1M-window reasoning model keeps most of its window for the
   transcript.
2. After a steered user message is committed, the next model step is
   respond-only (`tool_choice = None` + a reply instruction block), and the
   turn resumes its normal loop afterward.

## Constraints and non-goals

- No wire-protocol or durable-schema changes; the changes are internal to the
  orchd execution actor.
- Steer admission, follow-up queueing, cancellation, and abort markers (F-01)
  are unchanged.
- Mid-turn auto-compaction and step caps are explicitly out of scope.

## Proposed design

### 1. Bounded output reserve (`packages/orchd/src/runtime/execution/budget.rs`)

Keep the function signature so callers and telemetry do not change:

```rust
pub(super) fn enforce_context_budget(
    prompt: &SemanticRunPrompt,
    transcript: &TranscriptSnapshot,
    tools: &[ToolDef],
    context_window: u64,
    output_reserve: u64,
    reasoning_enabled: bool,
) -> Result<BudgetEstimate, AgentApiError>
```

Change the computation:

```rust
/// Maximum tokens reserved for a single model step's output (reasoning and
/// completion share one budget). Prevents full `max_tokens` (e.g. 384K on
/// deepseek-v4-flash) from consuming the window before any transcript exists.
const OUTPUT_RESERVE_CAP: u64 = 32_768;

let output_reserve = output_reserve.min(OUTPUT_RESERVE_CAP);
// Reasoning output counts against the same provider-side completion budget,
// so no separate reasoning reserve is added.
let reasoning_reserve = 0;
```

`reasoning_enabled` remains in the signature (and the failure message keeps a
`reasoning=` field plus a new `reasoning_enabled=` flag) so a reasoning model
that exhausts the budget is diagnosable. All other failure message fields and
the `BudgetEstimate` shape are unchanged.

### 2. Respond-first steer step

#### State (`packages/orchd/src/runtime/execution/state.rs`)

Add one flag to `ExecutionState`:

```rust
/// Set when a steered user message was committed; the next model step must
/// answer it without tools (F-35).
pub respond_after_steer: bool,
```

#### Commit point (`packages/orchd/src/runtime/execution/actor/tools.rs`)

`commit_steering` sets the flag after the user message is committed:

```rust
self.commit_message(message, steering.message_id.clone()).await?;
self.state.respond_after_steer = true;
```

This covers both run-loop branches that commit steering (after tool
execution, and after a step that returned no tool calls).

#### Step construction (`packages/orchd/src/runtime/execution/actor/run.rs`)

In `run_model_step`, take the flag before building the request:

```rust
let respond_after_steer = std::mem::take(&mut self.state.respond_after_steer);
```

When set:

- Clone `self.request.run_prompt` and append a `PromptBlock` (kind
  `Instruction`, authority `Platform`, trust `Trusted`, cache scope
  `RunDynamic`, id `steer-respond`) whose content tells the model to answer
  the newly delivered user message directly in text and not to call tools.
- Use the modified prompt for both the budget estimate and the request.
- Override `options.tool_choice = ToolChoice::None` regardless of the
  configured tool choice.

#### Loop continuation (`packages/orchd/src/runtime/execution/actor/run.rs`)

The respond-only step returns no tool calls, so it falls into the existing
no-tool-call branch. That branch already commits any further pending steer and
continues; when no further steer is pending it currently ends the turn. Add
the flag so the turn resumes:

```rust
if let Some(steering) = self.state.steering.pop_front() {
    self.commit_steering(&steering).await?;
    return Ok((true, step.model));
}
if respond_after_steer {
    return Ok((true, step.model)); // answered the steer; resume normal work
}
Ok((false, step.model))
```

Defensive check: if a respond-only step still yields tool calls, fail closed
with `AgentApiError::InputRejected` instead of executing them.

## Package impact

| Package | Change |
|---|---|
| `piko-orchd` | budget.rs reserve policy; execution state flag; run loop respond-only step |

No other package changes; `piko-protocol`, `piko-hostd`, `piko-llmd`, and
`piko-sandbox` are untouched.

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- The respond-only step fails closed if the provider returns tool calls.
- Cancellation semantics are unchanged: a cancel still aborts the turn and
  commits the F-01 abort marker; pending steers already in `state.steering`
  are not committed after terminal (pre-existing behavior).
- Budget failure still fails the turn with `ContextBudgetExceeded`, but now
  only when the transcript is genuinely near the window.

## Test plan

- Unit (`runtime/execution/tests.rs`): large `output_reserve` with reasoning
  enabled reserves at most `OUTPUT_RESERVE_CAP`; a transcript sized like the
  failing session (~208K estimated) is accepted in a 1M window; failure
  message fields unchanged.
- Agent-level (`tests/agent_runtime_cases/`): a running tool-loop turn receives
  a steer (`AgentInputDelivery::Auto` while active). Assert the next captured
  `InferenceRequest` has `tool_choice == ToolChoice::None` and a
  `steer-respond` prompt block; the canned text response is committed; the
  following request has tools enabled again.
