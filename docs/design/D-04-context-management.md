# D-04: Context management — transcript accounting, snapshots, and output truncation

> Status: implemented
> Implements: [F-04](../features/F-04-context-management.md)

## Goal

Give the orchd runtime transcript a trustworthy per-message token accounting,
cheap copy-on-write snapshots, and a normalized model view that truncates
oversized tool output. The fail-closed budget preflight then accounts exactly
what is dispatched, and step telemetry exposes truncation and remaining
context.

## Constraints and non-goals

- hostd stays authoritative for durable transcript content: this slice never
  rewrites, drops, or truncates the committed transcript. The full tool
  output is committed and returned exactly as today.
- Dropping old messages to fit the window is F-05 (hostd compaction), not
  orchd. Over-budget after normalization still fails closed with
  `ContextBudgetExceeded`.
- No settings/protocol churn: the truncation cap is a documented constant
  (`TranscriptPolicy::default()`), not a wire field, in this slice.
- No new tools (`get_context_remaining` / `new_context_window`) and no
  world-state diffing.

## Proposed design

### 1. Domain: `orchd/domain/transcript/tokens.rs` — one documented estimator

Move the message estimator out of `runtime/execution/budget.rs` into the
transcript domain so accounting and preflight share a single basis:

```rust
pub fn message_tokens(message: &Message) -> u64        // content + 16 framing
pub fn message_content_tokens(content: &MessageContent) -> u64
pub fn blocks_tokens(blocks: &[ContentBlock]) -> u64   // text ceil(bytes/3),
                                                       // thinking text+signature,
                                                       // image bytes+mime+512
pub fn text_tokens(text: &str) -> u64                  // ceil(bytes/3)
pub fn serialized_tokens<T: Serialize + ?Sized>(value: &T) -> u64
pub fn estimate_messages(messages: &[Message]) -> Vec<u64>
```

`budget.rs` keeps only the fixed-overhead computation (`serialized_tokens`
for the prompt and tool schemas) and imports the message estimator from the
domain. The estimator is unchanged from the pre-slice preflight so admission
behavior does not drift.

### 2. Domain: `orchd/domain/transcript/snapshot.rs` — copy-on-write view

```rust
pub struct TranscriptSnapshot {
    messages: Arc<Vec<Message>>,
    tokens: Arc<Vec<u64>>,
    total_tokens: u64,
}
```

- `new(messages, tokens)` computes `total_tokens` once.
- `messages()` / `tokens()` / `total_tokens()` accessors; `into_messages()`
  unwraps the `Arc` when unique, falls back to a clone otherwise.
- `Clone` is cheap (two `Arc` bumps); `shares_storage_with` exposes
  `Arc::ptr_eq` for tests.

### 3. Domain: `orchd/domain/transcript/normalize.rs` — model-view projection

```rust
pub const DEFAULT_MAX_TOOL_OUTPUT_TOKENS: u64 = 24_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptPolicy { pub max_tool_output_tokens: u64 }

pub struct NormalizedTranscript {
    pub snapshot: TranscriptSnapshot,
    pub truncated_outputs: usize,
}

pub fn normalize(messages: &[Message], policy: &TranscriptPolicy)
    -> (Vec<Message>, usize);
```

Truncation rules for a `ToolResult` whose text blocks exceed
`max_tool_output_tokens`:

- Budget is byte-based (`budget_bytes = cap * 3`) and consumed across text
  blocks in order; each block keeps its head cut on a character boundary.
- When the budget runs out, a single marker block is appended:
  `[Tool output truncated: retained {kept} of {total} characters. The full
  output is preserved in session history — read the file or re-run the tool
  to inspect the remainder.]`
  (`kept`/`total` are character counts across all text blocks).
- Non-text blocks (`Image`, `Thinking`) are preserved unchanged; `details`,
  `is_error`, `tool_name`, `tool_call_id` are copied verbatim.
- Below the cap, the message is cloned unchanged.

### 4. Domain: `orchd/domain/transcript/transcript.rs` — manager upgrade

`TranscriptManager` gains:

```rust
tokens: Vec<u64>,                 // per-message estimates, kept in step
generation: u64,                  // bumped on every mutation
raw_snapshot: Option<Arc<TranscriptSnapshot>>,  // invalidated on mutation
```

- Every `push_*` appends `message_tokens`; `rollback(checkpoint)` truncates
  both `messages` and `tokens`.
- `snapshot()` returns the cached `Arc` when the generation is unchanged
  (O(1) reuse across model steps, telemetry, and preflight) and rebuilds
  otherwise.
- `model_view(&TranscriptPolicy)` runs `normalize` over the committed
  messages, re-estimates the projected messages, and returns a
  `NormalizedTranscript`. It never mutates the manager.
- `checkpoint()`/`rollback()` semantics are unchanged.

### 5. Runtime: `orchd/runtime/execution/budget.rs` — account the dispatched view

```rust
pub(super) fn enforce_context_budget(
    prompt: &SemanticRunPrompt,
    transcript: &TranscriptSnapshot,   // normalized model view
    tools: &[ToolDef],
    context_window: u64,
    output_reserve: u64,
    reasoning_enabled: bool,
) -> Result<BudgetEstimate, AgentApiError>;

pub(super) struct BudgetEstimate {
    pub fixed_tokens: u64,
    pub transcript_tokens: u64,
    pub total: u64,
    pub context_remaining: u64,
}
```

- Fixed overhead logic is unchanged (prompt + tools + output + reasoning +
  safety margin); transcript cost is now `snapshot.total_tokens()`.
- Rejection message gains `context_remaining`; success returns the estimate
  for telemetry.

### 6. Runtime: `orchd/runtime/execution/actor.rs` — dispatch the model view

In `run_model_step`:

```rust
let model_view = self.state.transcript.model_view(&TranscriptPolicy::default());
let snapshot = &model_view.snapshot;
let transcript = snapshot.messages().to_vec();
...
if let Some(config) = model_config.as_ref() {
    let estimate = enforce_context_budget(
        &self.request.run_prompt, snapshot, &tools,
        config.context_window, config.max_output_tokens, thinking.is_some(),
    )?;
    span.record("context_remaining", estimate.context_remaining);
}
span.record("truncated_outputs", model_view.truncated_outputs);
span.record("transcript_tokens", snapshot.total_tokens());
```

The `GatewayRequest.transcript` is the normalized view; commit/run-result
paths keep using the full `TranscriptManager` content.

## Test plan

Unit (orchd domain):

- estimator determinism and message framing;
- accounting stays consistent across push + rollback;
- snapshot sharing (`shares_storage_with`), cache invalidation on mutation;
- normalization: small output untouched, large output truncated with marker,
  images/details/errors preserved, multi-block budget consumption,
  determinism;
- `model_view` truncated count and snapshot total match the estimator over
  the projected messages.

Unit (runtime budget): preflight accounts the snapshot, reports
`context_remaining`, rejects over-budget with "compaction required".

Integration (`tests/agent_runtime_cases/context.rs`): a scripted gateway
emits a tool call; a registered bloat provider returns output above the cap;
the second model request's transcript contains the truncation marker while
the committed transcript retains the full output and the run completes.
`tests/common/faux_provider.rs` gains a `CannedResponse::tool_calls` variant
to emit `GatewayEvent::ToolCallChunk` sequences.

## Risks

- Truncation marker length could push a message over the cap after the fact;
  the marker is small and the estimator's 16-token framing + byte-based cap
  absorb it. Preflight still fails closed if the projection is over budget.
- Byte-vs-character truncation: text is cut on character boundaries, so
  multi-byte UTF-8 never splits mid-codepoint.
