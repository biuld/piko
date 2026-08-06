# F-11: Guardian auto-review loop

> Status: implemented (slice 1; F-11/D-11/V-11)
> Priority: P1
> Source evidence: codex-rs `core/src/guardian/*` (compact review transcript →
> guardian review session → strict JSON allow/deny; circuit breaker),
> `core/src/session_prefix.rs` (guardian reminders), `core/src/safety.rs`

## Summary

When a tool call needs on-request approval, an operator can enable an
automated **guardian** that reviews the request against a bounded slice of
the session and decides allow/deny itself, so routine requests do not pause
the turn for a human. The guardian must answer in strict JSON within a
bounded time; a timeout, malformed output, or model failure fails the request
closed with a distinct error, and a circuit breaker stops the loop after
consecutive non-accepting outcomes so the human takes over.

## Problem

1. **Every on-request approval pauses the turn.** With F-07, any tool call
   routed through the approval gateway blocks until the user answers or the
   deadline expires. For a session where the operator has decided to trust
   the agent's judgment with a review, that is unnecessary friction: a
   repeatable, safe pattern (e.g. `cargo test`, `git status`) interrupts
   every time.
2. **No automated safety net between "never approve" and "always ask".**
   `ToolApprovalRequirement` supports `never` / `on-request` / `always`, but
   there is no policy that lets the agent propose and a *model* review the
   proposal fail-closed.
3. **Repeated denials have no remediation.** If a guardian keeps denying
   (or a broken model keeps failing), the turn can burn time and tokens on a
   request stream that will never be approved. There is no mechanism to stop
   the loop and escalate to the human.

## User journeys

1. An operator sets `[guardian] enabled = true` in project settings. An agent
   calls `cargo test`; hostd reviews the request against recent session
   context, the guardian answers `{"allow": true, "reason": "build check"}`,
   and the tool executes without a user prompt. No session/workspace grant
   is written, so a later risky call is still reviewed.
2. An agent calls `rm -rf ~/.ssh`; the guardian answers
   `{"allow": false, "reason": "destructive path outside the workspace"}`.
   The tool fails with a deterministic `guardian_denied` error whose message
   names the reason, and the model continues the turn with a different
   action.
3. The guardian model is unreachable. The request times out (or the model
   returns non-JSON). hostd fails the request closed with
   `guardian_unavailable`; the tool does not run.
4. The guardian denies three consecutive requests (the operator's
   `max-consecutive-denials` threshold). The circuit breaker trips: the next
   request is escalated to the user through the normal F-07 prompt. When the
   user answers (accept or decline), the breaker resets and the loop
   re-arms.
5. An operator disables the guardian. Approval behavior is exactly F-07:
   every on-request call prompts the user.

## In scope

- `[guardian]` settings: `enabled` (default off), `model`, `provider`,
  `timeout-secs` (default 30), `max-consecutive-denials` (default 3),
  merged across global/project/override settings.
- Automatic review of on-request tool approvals when the guardian is enabled
  and not tripped, using a bounded review transcript (recent session
  messages + the tool name and arguments) and a strict-JSON
  `{"allow": bool, "reason": string}` answer.
- One-shot allow: a guardian allow executes the call without a user prompt
  and never writes a session/workspace/permanent grant.
- Deterministic deny: a guardian deny fails the call with a non-retryable
  `guardian_denied` error including the guardian's reason.
- Fail-closed review failure: timeout, malformed output, or model error
  fails the call with a non-retryable `guardian_unavailable` error.
- Circuit breaker: `max-consecutive-denials` consecutive non-accepting
  outcomes (denies and failures) trip the loop for the session; subsequent
  requests use the F-07 user flow; any user decision resets the breaker.
- Guardian model override with default-model fallback (same pattern as the
  F-05 summarizer override).

## Out of scope

- Client-visible guardian lifecycle events or review status in the session
  snapshot (review outcomes are observable through tool errors and tracing).
- Durable circuit-breaker state across hostd restarts (the breaker is
  session-scoped runtime state, like pending approvals).
- Guardian reminders injected into prompts.
- F-12 elicitation pause, attestation, and patch-safety assessment.
- Network-approval decisioning and command allow/deny prefix rules
  (F-08 exec policy / M-config permission profiles).
- Reviewing requests that are already auto-accepted by the approval store
  (store grants short-circuit before the guardian).
- Per-request approval tiers beyond the existing
  `ToolApprovalRequirement` (`never` / `on-request` / `always`).

## Behavior and states

### Guardian lifecycle

```text
on-request approval
  ├─ store auto-accept match ───────────────> accept (unchanged F-07)
  ├─ guardian disabled or tripped ───────────> user flow (unchanged F-07)
  └─ guardian active:
       ├─ allow ────────────────────────────> accept (one-shot, no grant)
       ├─ deny ─────────────────────────────> guardian_denied (non-retryable)
       └─ timeout / malformed / model error ─> guardian_unavailable (fail closed)

circuit breaker: consecutive non-accepts (denies + failures)
  ├─ < max-consecutive-denials ─────────────> keep reviewing
  └─ >= max-consecutive-denials ────────────> tripped; escalate to user
       any user decision (accept or decline) ─> reset and re-arm
```

- `guardian active`: the session has guardian enabled and the breaker is not
  tripped.
- `one-shot allow`: the tool executes for this call only; the approval store
  is untouched, so future calls are reviewed again.
- `guardian_denied` / `guardian_unavailable`: terminal, non-retryable tool
  errors; the run loop records them like any failed tool call and the model
  continues with a different action.
- Breaker reset: any user decision on a later escalated request (accept or
  decline) resets the counter to zero and clears the tripped flag.

### Races

- Review vs. run cancellation: the registry races the whole approval request
  against cancellation (existing F-07 path); a dropped review records no
  breaker state.
- Review vs. review timeout: the bounded timeout owns the outcome; a late
  model response is ignored.

## Acceptance criteria

- [x] `[guardian]` settings merge correctly across global/project/override
      with the documented defaults (fixture: settings merge unit tests).
- [x] With the guardian enabled and a model answering
      `{"allow": true, ...}`, an on-request tool call executes without a user
      prompt and no grant is written to any approval-store scope (fixture:
      hostd gateway review test).
- [x] A guardian deny fails the call with a non-retryable `guardian_denied`
      error whose message includes the guardian reason, and increments the
      session denial counter (fixture: hostd gateway + registry tests).
- [x] Timeout, malformed JSON, or model error fails the call closed with a
      non-retryable `guardian_unavailable` error (fixture: hostd gateway
      review-failure test + parse unit tests).
- [x] After `max-consecutive-denials` consecutive non-accepting outcomes,
      the next request reaches the user (an `ApprovalRequested` event is
      published), and a user decision resets the breaker (fixture: hostd
      gateway circuit-breaker test).
- [x] Guardian decisions are excluded from `is_approval_accepted` and map to
      distinct errors in the orchd registry (fixture: registry decision
      mapping tests).
- [x] Differential validation against codex-rs: the guardian auto-review
      loop is kept (adapted to a host-owned model call instead of a spawned
      review session); strict-JSON allow/deny is kept; fail-closed
      timeout/malformed is kept; the circuit breaker is kept (adapted:
      consecutive non-accepts escalate to the user, and any user decision
      re-arms) (fixture: `docs/verification/V-11`).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does the review run? | hostd, as a bounded model call over the durable session transcript | hostd is authoritative for approvals and already owns host-level LLM calls (compaction summarizer); no spawned agent session is needed for slice 1. |
| What does an allow write? | Nothing — one-shot accept, no store grant | A grant would silently approve future, possibly riskier, calls; per-request review preserves the fail-closed posture. |
| Timeout/malformed semantics | Fail closed with `guardian_unavailable`, tool does not run | Cross-cutting invariant: approvals deny by default; a broken reviewer must not auto-approve. |
| Circuit-breaker condition | Consecutive non-accepts (denies + failures) ≥ `max-consecutive-denials` | A stuck or malfunctioning reviewer should surface to the human; counting failures prevents a flaky model from denying the whole session. |
| Breaker recovery | Any user decision on an escalated request resets the breaker | A human decision breaks the loop and re-arms the guardian for the next request. |
| Guardian model | `[guardian] model`/`provider` override with default-model fallback | Same proven pattern as the F-05 summarizer override; absent override uses the session default. |
| Distinct tool errors | `guardian_denied` and `guardian_unavailable` | The model must distinguish "reviewed and rejected (reason)" from "reviewer unavailable" to choose the next action. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Guardian auto-review loop converting on-request approvals into auto-approvals | kept (adapted) | piko runs the review as a host-owned bounded model call (compaction-summarizer pattern) instead of a spawned review agent session; the review transcript is a bounded projection of the durable session tree. |
| Strict JSON allow/deny review output | kept | `{"allow": bool, "reason": string}` parsed fail-closed; malformed output is `guardian_unavailable`. |
| Guardian review fails closed on timeout/malformed output | kept | The review has its own bounded timeout (`[guardian] timeout-secs`); expiry never auto-approves. |
| Circuit breaker around repeated review outcomes | kept (adapted) | piko trips after `max-consecutive-denials` consecutive non-accepts and escalates to the user; any user decision resets. |
| Guardian reminders injected into the prompt | rejected (for this slice) | No piko consumer yet; the review transcript already carries the needed context. Tracked as a follow-on under F-03. |
| Review as a spawned agent session with its own compaction | rejected (for this slice) | A single bounded model call is sufficient for slice 1 and reuses the hostd summarizer pattern; agent-session review is a possible follow-on. |

## Open questions

1. Should the guardian review use the session's active tool-approved
   "request reason" (the model's own justification) when one exists? Slice 1
   only passes tool name + arguments; a richer request reason is a follow-on.

## Reference evidence

- codex-rs `core/src/guardian/*` — compact review transcript, guardian
  review session, strict JSON allow/deny, circuit breaker.
- codex-rs `core/src/session_prefix.rs` — guardian reminders.
- F-07 (`docs/features/F-07-tool-approvals.md`) — the approval gateway this
  feature extends; its out-of-scope section names the guardian loop and
  circuit breaker as the home for repeated-request handling.
