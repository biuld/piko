# F-07: Tool approval timeout and deny semantics

> Status: implemented
> Priority: P0
> Source evidence: codex-rs `core/src/tools/approvals.rs`,
> `core/src/session/mod.rs` (`request_command_approval`),
> `core/src/state/turn.rs` (pending-approval lifecycle),
> `codex-protocol` `ReviewDecision`

## Summary

When a tool call requires human approval, the agent pauses the call and asks
the user. The request must resolve in bounded time: if the user does not
answer before a configurable deadline, hostd fails the request closed and the
tool call fails with a deterministic, non-retryable error. A user decline and
an expiry are distinct outcomes that are surfaced separately to the client,
and neither ever writes a grant into the approval store.

## Problem

Today a pending approval can wait forever: the orchestrator's gateway awaits a
decision with no deadline, so a client that never answers (disconnected,
ignored, crashed) leaves the turn stuck in `WaitingForApproval` and the
pending entry in the session snapshot forever. Denial and non-answer are also
indistinguishable: a timeout currently has no protocol representation, so
clients cannot tell "the user said no" from "the request expired", and the
model sees a generic "User declined approval" error for both. That is a
fail-open ambiguity on a security-sensitive path.

## User journeys

1. An agent wants to run `npm install`. hostd asks the user; the user is
   away and never answers. After the configured timeout (default 120s), the
   request resolves as **expired**, the turn returns from
   `WaitingForApproval`, the tool call fails with an `approval_expired`
   error, and the model continues the turn with a different action.
2. An agent wants to run `rm -rf build`. The user clicks **Decline**. The
   request resolves as **rejected**, the tool call fails with a
   `declined` error, and no grant of any scope is recorded.
3. A user opens an approval dialog and clicks **Accept** just after the
   request already expired. The late response is ignored: the approval was
   already resolved, no grant is written, and no second resolution event is
   emitted.
4. An operator sets `[approvals] timeout-secs = 30` in the project settings.
   Every approval in that project expires after 30 seconds.

## In scope

- A configurable approval deadline (`[approvals] timeout-secs`, default 120),
  merged across global/project/override settings.
- Fail-closed expiry: after the deadline, the pending request is removed,
  an `ApprovalResolved { status: Expired }` event is published, the gateway
  returns an expired decision, and the tool call fails with a
  non-retryable `approval_expired` error.
- Distinct protocol representations for user decline (`Rejected`) and expiry
  (`Expired`), including the decision and status types shared with clients.
- Terminal deny semantics: denied and expired requests never populate the
  session/workspace/permanent approval stores; a response arriving after
  resolution is ignored; a denied or expired call is not retried
  automatically by the runtime.
- Preserved behavior: scope grants (`AcceptSession`/`AcceptWorkspace`/
  `AcceptPermanent`) still auto-accept matching future requests without
  re-prompting; the run-level cancellation race still resolves to a decline.

## Out of scope

- F-11 guardian auto-review loop and circuit breaker (repeated-request
  handling lives there).
- F-12 elicitation pause, attestation, and patch-safety assessment.
- Network-approval decisioning and command allow/deny prefix rules
  (F-08 exec policy / M-config permission profiles).
- Same-turn denial memoization (no codex-rs precedent; the guardian circuit
  breaker is the piko home for it).
- Approval tier configuration beyond the existing
  `ToolApprovalRequirement` (`never` / `on-request` / `always`).

## Behavior and states

### Approval lifecycle

```text
requested (pending) --user accept--> approved (granted per scope or once)
requested (pending) --user decline--> rejected (terminal)
requested (pending) --deadline--> expired (terminal)
requested (pending) --run cancelled--> declined (terminal, existing path)
```

- `pending`: entry is in hostd's pending map, `ApprovalRequested` was
  published, the turn shows `WaitingForApproval`.
- `approved`: the client answered with an accept decision; the entry is
  removed and the tool executes. Scope decisions write grants.
- `rejected`: the client answered `Decline`; the entry is removed, no grant
  is written, the tool fails with `declined`.
- `expired`: no answer arrived before the deadline; the entry is removed, no
  grant is written, the tool fails with `approval_expired`.

### Resolution races

- User response vs. deadline: exactly one wins. Whichever path removes the
  pending entry first owns the resolution; the other path observes no entry
  and is a no-op.
- User response vs. run cancellation: the existing run-level cancellation
  race is unchanged and resolves to a decline.
- Duplicate or late responses: `respond_approval` returns `false` and the
  response is ignored.

## Acceptance criteria

- [ ] With no response, a tool-approval request resolves to `Expired` after
      the configured timeout: the pending entry is removed, an
      `ApprovalResolved { status: Expired }` event is published, and the
      gateway returns the expired decision (fixture: hostd gateway timeout
      test with a mock gateway input).
- [ ] An expired approval fails the tool call with a deterministic,
      non-retryable `approval_expired` error whose message names the timeout
      (fixture: orchd registry decision-mapping test).
- [ ] A user decline fails the tool call with `declined` and resolves the
      approval as `Rejected`; neither a decline nor an expiry writes a grant
      to any scope of the approval store (fixture: existing approval-store
      tests + gateway resolution tests).
- [ ] A response arriving after the deadline is ignored (no double
      resolution, no grant write) (fixture: late-response race test).
- [ ] `[approvals] timeout-secs` overrides the default and merges correctly
      across global/project/override settings (fixture: settings merge unit
      tests).
- [ ] Differential validation against codex-rs: `Denied` (continue the turn,
      no execution) and `TimedOut` (fail closed) map to piko's `Rejected` and
      `Expired`; codex-rs `Abort` (stop the turn on a synthetic denial) is
      rejected for this slice because piko's run cancellation already
      produces terminal aborts (fixture: `docs/verification/V-07`).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does the deadline live? | hostd gateway (the owner of pending approvals) | hostd is authoritative for user-visible state; orchd just observes the decision. |
| What is the default timeout? | 120 seconds | Long enough for a human, short enough that a dead client cannot wedge a turn. |
| Distinct status vs. reuse of rejected? | New `Expired` status/decision | Clients and models must distinguish "user said no" from "nobody answered". |
| Timeout during an in-flight tool call | Expire the request, return a non-retryable tool error | Fail closed; the model can retry with a different action in the same turn. |
| Late response handling | Ignore (entry already removed) | A resolved approval is terminal; re-resolving would double-grant or double-publish. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `ReviewDecision::Denied { rejection }` — user denied; agent continues with a different action | kept | Maps to `ApprovalDecision::Decline` / `ApprovalStatus::Rejected` and the `declined` tool error. |
| `ReviewDecision::TimedOut` — automatic review expired, fail closed | kept (adapted) | Maps to piko's `Expired` approval status/decision and the `approval_expired` tool error; in codex-rs it is guardian-only, in piko it is the host-level request deadline. |
| `ReviewDecision::Abort` — pending approval cleared ⇒ turn stops rather than continuing on a synthetic denial | rejected (for this slice) | piko's turn cancellation already ends the run with terminal aborts; the deadline is not a turn-level stop. |
| Pending approval keyed by call id, overwrite warns | kept | piko already keys pending entries by `toolEntityId`; no change. |
| Grants only written on accept (session/permanent scopes) | kept | piko's `ApprovalStore` already only writes on accept decisions; expiry/decline paths are excluded. |

## Open questions

1. Should an expired approval surface a countdown to clients, or is the
   terminal event enough? Deferred: terminal event + snapshot are sufficient
   for the current clients.

## Reference evidence

- codex-rs `core/src/tools/approvals.rs` — central approval policy stage,
  `ReviewDecision` routing, fail-closed review outcomes.
- codex-rs `core/src/session/mod.rs` — `request_command_approval` pending
  approval registration and reply awaiting.
- codex-rs `core/src/state/turn.rs` — pending-approval map lifecycle and
  `clear_pending_waiters`.
- codex-rs `codex-protocol/src/protocol.rs` — `ReviewDecision` variants
  (`Approved`, `Denied`, `TimedOut`, `Abort`).
