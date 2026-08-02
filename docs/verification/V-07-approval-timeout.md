# V-07: Approval timeout and deny-semantics acceptance evidence

> Date: 2026-08-02
> Fixture: `packages/hostd/tests/approval_timeout.rs` (end-to-end through
> `HostServer` + real `OrchAgentRunRunner` with a scripted tool-calling
> gateway), `packages/orchd/src/adapters/tools/registry_tests.rs`,
> `packages/hostd/tests/settings.rs`
> Environment: macOS, `cargo test -p piko-hostd -p piko-orchd`

## Reproduction

```bash
cargo test -p piko-hostd --test approval_timeout
cargo test -p piko-orchd --lib adapters::tools::registry_tests
cargo test -p piko-hostd --test settings
```

The end-to-end fixture drives one turn in which the scripted gateway emits a
`bash` tool call (`pwd`, deliberately not covered by any pre-existing grant).
The runner is constructed with `[approvals] timeout-secs = 1`. No client
answers the approval; the turn is allowed to run to completion.

## Result

All new tests pass (1 end-to-end, 5 registry mapping, 3 approval settings),
and the full `piko-hostd` / `piko-orchd` / `piko-protocol` /
`piko-orchd-api` suites stay green.

Observed end-to-end event sequence (abridged):

```text
Approval(Requested { tool_name: "bash", tool_args: {"command": "pwd"} })
… ~1s elapses with no response …
Approval(Resolved { decision: Decline })            # Expired → Decline projection
TranscriptCommitted(ToolResult {
  details: { "code": "approval_expired",
             "message": "Approval request expired before a decision arrived",
             "retryable": false },
  is_error: true })
TurnLifecycle(Completed)                             # turn not stuck in WaitingForApproval
```

After the turn: a late `ApprovalRespond { decision: Accept }` returns
`Ok(Empty)` without publishing a second `ApprovalEvent::Resolved`, and a
`StateSnapshot` lists zero pending approvals (the expired entry was removed).

Registry mapping evidence: `Expired` → `approval_expired` (non-retryable),
`Decline` → `declined` (non-retryable), `Accept` → tool executes, no gateway
→ `approval_unavailable`, and `is_approval_accepted(Expired) == false`.

Settings evidence: default `timeout-secs = 120`; override wins; project
setting overrides global; a project without the key preserves the global
value; `[approvals]` appears in the `host` config namespace.

## Invariants

- Every pending approval resolves within its deadline: user decision or
  fail-closed expiry; no path waits indefinitely.
- Expiry and user decline are distinct protocol outcomes
  (`ApprovalStatus::Expired` vs `Rejected`, `approval_expired` vs `declined`
  tool errors).
- Denied/expired requests never write grants; a response after resolution is
  a no-op with no second resolution event.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
