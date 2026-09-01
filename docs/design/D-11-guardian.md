# D-11: Guardian auto-review loop

> Status: accepted
> Implements: [F-11](../features/F-11-guardian.md) (slice 1)

## Goal

Turn on-request tool approvals into model-reviewed, fail-closed decisions
when the operator enables the guardian:

1. `[guardian]` settings control enablement, the review model, the review
   deadline, and the circuit-breaker threshold.
2. The hostd approval gateway runs a bounded review model call over a
   bounded slice of the durable session transcript plus the tool request,
   and maps a strict-JSON answer to a deterministic tool outcome.
3. Consecutive non-accepting outcomes trip a per-session circuit breaker
   that escalates to the user; any user decision resets it.

## Constraints and non-goals

- hostd stays authoritative: the review runs host-side (compaction-summarizer
  pattern) over the durable session tree; orchd only observes the resulting
  decision.
- The F-07 user flow and approval store are unchanged; store auto-accepts
  short-circuit before the guardian.
- Guardian allows never write session/workspace/permanent grants.
- No new client-facing protocol events in this slice; outcomes surface as
  deterministic tool errors plus `tool.approval` tracing.
- Non-goals: durable breaker state across hostd restarts, guardian prompt
  reminders, spawned review agent sessions, F-12 safety/elicitation.

## Proposed design

### 1. Settings: `[guardian]`

`HostSettings` gains a `guardian: Option<GuardianSettings>` section:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GuardianSettings {
    /// Master switch for guardian auto-review. Default: false.
    pub enabled: Option<bool>,
    /// Reviewer model id; falls back to the session default model.
    pub model: Option<String>,
    /// Reviewer provider; falls back to the session default provider.
    pub provider: Option<String>,
    /// Review deadline in seconds. Default: 30.
    pub timeout_secs: Option<u64>,
    /// Consecutive non-accepts (denies + failures) that trip the breaker.
    /// Default: 3.
    pub max_consecutive_denials: Option<u32>,
}
```

- Field-level merge like `approvals`/`retry`; `installed_settings_fixture()`
  documents the section; `resources/settings.toml` gains `[guardian]`.
- `orch_factory.rs` reads `settings.guardian` and passes the resolved values
  into `OrchAgentRunRunner::new_with_mcp` (new optional param), which builds
  a small resolved `GuardianConfig { enabled, timeout, max_consecutive_denials }`.

### 2. `piko-orchd-api`: guardian decisions

`ToolApprovalDecision` gains two variants (hostd ↔ orchd interface only;
`piko-protocol` `ApprovalDecision` is untouched):

```rust
ToolApprovalDecision::GuardianDenied { reason: String },
ToolApprovalDecision::GuardianUnavailable,
```

`is_approval_accepted` excludes both. The orchd registry maps them to
deterministic, non-retryable errors:

| Decision | Tool error |
|---|---|
| `GuardianDenied { reason }` | `guardian_denied` — "Guardian denied approval: {reason}" |
| `GuardianUnavailable` | `guardian_unavailable` — "Guardian review failed; failing closed" |

### 3. hostd domain: `domain/guardian/mod.rs`

Pure logic with unit tests:

- `GuardianDecision { allow: bool, reason: String }` and
  `parse_decision(text: &str) -> Result<GuardianDecision, String>` — strict
  JSON parse of `{"allow": bool, "reason": string}`; any deviation (missing
  fields, wrong types, trailing content) is an error (fail closed).
- `REVIEW_PROMPT` — the guardian system prompt: review the tool request
  against the provided transcript, answer EXACTLY
  `{"allow": true|false, "reason": "..."}`, never auto-approve destructive
  or out-of-workspace operations, never answer with anything but JSON.
- `build_review_context(entries, max_entries, max_chars_per_entry)` — bounded
  transcript projection over `context_entries_after_compaction`: the most
  recent `max_entries` message/context entries, each truncated to
  `max_chars_per_entry` (defaults: 20 entries, 2000 chars).
- `GuardianState { consecutive_denials: u32, tripped: bool }` with
  `record_non_accept(max_consecutive_denials)` (increments; trips at
  threshold) and `reset()` (clears both).
- `run_review(model_executor, model, context, tool_name, tool_args)` — builds
  the user message from the transcript + tool request and calls
  `llm_call` (same shape as `domain/compaction/summarizer`).

### 4. HostApp: review callback

The runner never touches the model executor or the session tree directly; a
callback wires the two (same pattern as `set_context_window_callback`):

```rust
pub type GuardianReviewCallback = Arc<
    dyn Fn(String /* session_id */, GuardianReviewRequest)
        -> Pin<Box<dyn Future<Output = Result<GuardianDecision, String>> + Send>>
        + Send
        + Sync,
>;
```

`HostApp::wire_guardian_callback()` registers a callback that:

1. Locks `state`, reads the session's `entries`, and builds the bounded
   review context (guardian domain helper).
2. Resolves the reviewer model: `[guardian] model`/`provider` override with
   default-model fallback (mirrors the F-05 summarizer override).
3. Calls `run_review` through `model_executor`, parses the strict JSON, and
   returns `GuardianDecision`; any failure (no executor, model error,
   malformed JSON) is `Err(String)` → `GuardianUnavailable`.

### 5. Runner gateway: `approval_gateway.rs`

`OrchAgentRunRunner` gains:

```rust
guardian_config: Option<GuardianConfig>,
guardian_review: Arc<std::sync::RwLock<Option<GuardianReviewCallback>>>,
guardian_states: Arc<std::sync::Mutex<HashMap<String, GuardianState>>>, // by session_id
```

In `request_tool_approval`, after the store auto-accept check and before a
pending entry is created:

```text
if guardian enabled and a callback is wired and the session breaker is not
tripped:
    review = timeout(guardian.timeout, callback(session_id, request))
    match review:
        Ok(GuardianDecision { allow: true, .. })  -> Accept (one-shot, no grant)
        Ok(GuardianDecision { allow: false, reason })
            -> record non-accept; return GuardianDenied { reason }
        Err(_)  -> record non-accept; return GuardianUnavailable
```

The user flow then runs unchanged for (a) disabled guardian, (b) tripped
breaker, or (c) a request that still needs a human. Every resolved user
decision (accept, decline, or expired) calls `reset()` on the session breaker
state so the loop re-arms.

## Files touched

| File | Change |
|---|---|
| `packages/hostd/src/domain/config/settings.rs` | `GuardianSettings`, merge, defaults template |
| `packages/hostd/resources/settings.toml` | `[guardian]` section |
| `packages/orchd-api/src/approval.rs` | `GuardianDenied`/`GuardianUnavailable`; `is_approval_accepted` |
| `packages/orchd/src/adapters/tools/registry.rs` | decision → error mapping |
| `packages/hostd/src/domain/guardian/mod.rs` | decision parse, prompt, context builder, breaker state, review call |
| `packages/hostd/src/domain/mod.rs` | export `guardian` module |
| `packages/hostd/src/adapters/agent_runner/orch_runner/mod.rs` | fields + constructor param + callback setter |
| `packages/hostd/src/adapters/agent_runner/orch_runner/approval_gateway.rs` | review branch + breaker updates |
| `packages/hostd/src/application/host_app.rs` | `wire_guardian_callback` |
| `packages/hostd/src/protocol/transport/jsonl_stdio.rs`, `packages/hostd/src/protocol/commands/config.rs` | wire callback at startup |
| `packages/hostd/src/protocol/orch_factory.rs` | pass `settings.guardian` |
| `docs/features/F-11-guardian.md`, `docs/agent-runtime-roadmap.md` | status updates |
| `docs/verification/V-11-guardian.md` | acceptance evidence |

## Verification

- Unit tests: strict-JSON parse (valid allow/deny, missing/wrong fields,
  trailing content); context builder bounds entries and chars; breaker
  trips at threshold and resets.
- Settings merge tests for `[guardian]`.
- Registry tests: `GuardianDenied`/`GuardianUnavailable` map to distinct
  non-retryable errors; `is_approval_accepted` is false for both.
- Hostd gateway tests with an injected review callback: allow executes
  without a user prompt and writes no grant; deny returns `GuardianDenied`
  and increments the breaker; callback error returns `GuardianUnavailable`
  and fails closed; after the threshold the next request publishes
  `ApprovalRequested` (user flow), and a user decision resets the breaker.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test -p piko-orchd-api -p piko-orchd -p piko-hostd`.
