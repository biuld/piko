# D-07: Approval timeout and deny semantics

> Status: implemented
> Implements: [F-07](../features/F-07-tool-approvals.md)

## Goal

Give every tool-approval request a bounded lifetime, resolve expiry
fail-closed with a distinct status, and make deny/expire terminal and
deterministic end-to-end.

## Constraints and non-goals

- hostd stays authoritative for pending approvals and their resolution
  events; orchd only observes the decision.
- The existing approval store scopes (session/workspace/permanent) and the
  auto-accept path are unchanged.
- Run cancellation semantics are unchanged (cancellation still races the
  approval request and resolves to decline).
- No guardian loop, no same-turn denial memoization, no network-approval
  decisioning (F-11 / F-08 / M-config).

## Proposed design

### 1. Settings: `[approvals] timeout-secs`

`HostSettings` gains an `approvals: Option<ApprovalSettings>` section:

```rust
pub struct ApprovalSettings {
    pub timeout_secs: Option<u64>,
}
```

- Default `120` (field-level merge like `retry`/`sandbox`).
- `installed_settings_fixture()` documents the new section.
- `orch_factory.rs` reads `settings.approvals` and passes the resolved
  `Duration` into `OrchAgentRunRunner::new_with_mcp`.

### 2. Protocol: expired is distinct from rejected

`piko-protocol` gains two variants:

- `ApprovalDecision::Expired` — a decision value an expired request resolves
  to on the wire (serialized as `expired`).
- `ApprovalStatus::Expired` — terminal snapshot/event status for expired
  requests, alongside `Pending` / `Approved` / `Rejected`.

`piko-orchd-api` mirrors the decision:

- `ToolApprovalDecision::Expired`.
- `is_approval_accepted(Expired) == false` (the current implementation only
  excludes `Decline`, so this is a correctness fix, not just an addition).

### 3. Orchd registry: deterministic error mapping

In `execute_tool`, replace the `matches!(decision, Decline)` gate with
`is_approval_accepted(&decision)` and map each non-accepting decision to a
stable error:

| Decision | Tool error |
|---|---|
| `Decline` | `declined` — "User declined approval" |
| `Expired` | `approval_expired` — "Approval request expired before a decision arrived" |
| (no gateway) | `approval_unavailable` — unchanged |
| (run cancelled) | `aborted` — unchanged |

All are `retryable: false`. The run loop treats them like any failed tool
call: the transcript records the error and the model continues with a
different action.

### 4. Hostd gateway: deadline ownership

`OrchAgentRunRunner` stores `approval_timeout: Duration`. In
`request_tool_approval`, after the pending entry is inserted and
`ApprovalRequested` published, the decision await becomes:

```text
select on:
  reply_rx     -> user decision (existing mapping + grant writes)
  deadline     -> resolve as expired
```

The deadline branch:

1. Removes the pending entry under the pending-approvals mutex (atomic with
   the reply path: exactly one resolver wins).
2. Publishes `ApprovalResolved { status: Expired }` through the observation
   router (the client path already clears pending approvals on any
   `ApprovalEvent::Resolved`).
3. Returns `ToolApprovalDecision::Expired` without touching the reply
   channel (the request is over; late user responses find no entry and are
   ignored).

The existing sender-dropped path (`rx` errors) stays a `Decline`; it means
the pending entry was removed without a decision (e.g. entry removed by the
reply handler race), never a grant.

### 5. Status → decision projection (client events)

`application/observation.rs` maps `ApprovalStatus::Expired` to
`ApprovalDecision::Decline` in the `ApprovalEvent::Resolved` projection, so
clients clear the pending prompt while the richer status remains available
to consumers that need it.

## Files touched

| File | Change |
|---|---|
| `packages/protocol/src/event.rs` | `ApprovalDecision::Expired`, `ApprovalStatus::Expired` |
| `packages/orchd-api/src/approval.rs` | `ToolApprovalDecision::Expired`; fix `is_approval_accepted` |
| `packages/orchd/src/adapters/tools/registry.rs` | decision-gate + error mapping |
| `packages/hostd/src/domain/config/settings.rs` | `ApprovalSettings`, merge, defaults |
| `packages/hostd/resources/settings.toml` | `[approvals]` section |
| `packages/hostd/src/adapters/agent_runner/orch_runner/mod.rs` | `approval_timeout` field + constructor param |
| `packages/hostd/src/adapters/agent_runner/orch_runner/approval_gateway.rs` | deadline race, expired resolution |
| `packages/hostd/src/adapters/agent_runner/orch_runner/turn_runner.rs` | status mapping stays user-decision-only |
| `packages/hostd/src/application/observation.rs` | `Expired` → decline projection |
| `packages/hostd/src/protocol/orch_factory.rs` | thread approval settings |

## Verification

- Unit tests: settings merge; `is_approval_accepted(Expired) == false`;
  registry error mapping for `Decline` / `Expired`.
- Hostd gateway test: a request with no response resolves to `Expired` after
  the (short, injected) timeout, publishes `ApprovalResolved`, and the
  pending map is empty afterwards; a late response is ignored.
- Existing turn/snapshot tests keep passing (pending approval →
  `WaitingForApproval`, then resolves once the entry is gone).
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test -p piko-protocol -p piko-orchd-api -p piko-orchd -p piko-hostd`.
