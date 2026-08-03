# F-12: Write safety assessment (patch-safety)

> Status: implemented (slice 1; F-12/D-12/V-12)
> Priority: P1
> Source evidence: codex-rs `core/src/safety.rs` (`assess_patch_safety`),
> `core/src/elicitation.rs` (elicitation pause), `core/src/attestation.rs`
> (attestation header)

## Summary

Before a workspace write tool (`edit` / `write`) is approved, hostd
deterministically assesses whether every target path is inside the sandbox's
writable roots. A write that is fully constrained to the writable roots is
auto-approved one-shot (no prompt, no store grant) because the sandbox policy
already enforces the boundary; a write that targets a path outside all
writable roots fails closed with a deterministic `safety_rejected` error
because execution would deny it regardless of approval. Requests that cannot
be assessed (no writable roots available) and requests under an operator
opt-out fall through to the existing user/guardian approval flow unchanged.

## Problem

1. **Every workspace write prompts even when it is already safe.** `edit` and
   `write` are `OnRequest` approvals. A routine in-workspace edit interrupts
   the turn every time, even though the sandbox policy would constrain the
   write to the writable roots at execution time (the enforcement already
   exists; the prompt adds friction, not safety).
2. **Out-of-roots writes produce a dead prompt.** A write targeting a path
   outside the writable roots (e.g. `~/.ssh/config`) is denied by the sandbox
   at execution time no matter what the user answers. Prompting the user for
   a decision that cannot succeed wastes a round-trip and misleads the model
   into thinking approval could extend the write.
3. **No deterministic safety tier between "always ask" and "guardian".** The
   F-11 guardian is a model review; for purely policy-determined writes a
   deterministic, host-owned gate is cheaper, faster, and never has a model
   failure mode.

## User journeys

1. An operator runs piko in a project whose sandbox policy allows writing
   inside the workspace. The agent calls `edit` on `src/lib.rs`; hostd sees
   the target is inside the writable roots and the tool executes without a
   prompt. No approval grant is written, so a later edit is assessed again.
2. An agent calls `write` with path `/Users/me/.ssh/authorized_keys`. hostd
   rejects it deterministically: the tool fails with `safety_rejected` and a
   reason naming the out-of-roots path. The model continues with a different
   action; the user is never prompted for a write that could not execute.
3. An operator sets `[safety] auto-approve-workspace-writes = false`.
   Workspace writes behave exactly as before F-12: every write prompts (or is
   reviewed by the guardian when enabled).
4. A write tool from a provider that does not expose writable roots (for
   example a future MCP write tool) requests approval. hostd cannot assess
   it, so it falls through to the normal user flow — no auto-approval without
   enforcement.

## In scope

- `[safety]` settings: `auto-approve-workspace-writes` (default `true`),
  merged across global/project/override settings.
- Writable-root projection: the workspace tool provider resolves its sandbox
  policy's write roots against the working directory and attaches them to
  approval requests for workspace write tools.
- hostd-owned assessment before the guardian and user flows:
  - all targets inside a writable root → one-shot `Accept` (no store grant);
  - any target outside all writable roots → `SafetyRejected { reason }`
    (deterministic, non-retryable `safety_rejected` tool error);
  - no roots available, malformed request, or opt-out → unchanged user flow.
- Deterministic reason strings surfaced in the tool error (which target path
  was rejected, or why the request could not be assessed).

## Out of scope

- Per-file diff / move-destination analysis beyond the write target path
  (piko's `edit`/`write` tools are whole-file; there is no
  `ApplyPatchAction` model to decompose).
- Granting writes outside the writable roots through user approval (piko has
  no path-grant mechanism and execution denies those writes regardless).
- Changing `edit`/`write` approval tiers (`OnRequest` stays; assessment is a
  gate in front of the existing flow).
- Elicitation pause (F-12 slice 2): host-side pause state while user
  elicitations are outstanding. Deferred until a piko consumer exists
  (blocking process-output waits or MCP auth elicitation, F-13).
- Attestation (`x-oai-attestation` upstream header): rejected — see Fusion
  decisions.
- Network-approval decisioning and command allow/deny prefix rules (F-08
  exec policy / M-config permission profiles).

## Behavior and states

### Write approval lifecycle

```text
workspace write approval request (edit / write)
  ├─ store auto-accept match ───────────────> accept (unchanged F-07)
  ├─ safety opt-out (setting false) ────────> user flow (unchanged F-07)
  ├─ no writable roots / unassessable ──────> user flow (unchanged F-07)
  └─ assessment:
       ├─ all targets in writable roots ────> accept (one-shot, no grant)
       └─ any target outside roots ─────────> safety_rejected (non-retryable)
```

- `safety gate active`: `[safety] auto-approve-workspace-writes` is true and
  the request carries writable roots and a target path.
- `one-shot accept`: the tool executes for this call only; the approval store
  is untouched, so future writes are assessed again.
- `safety_rejected`: terminal, non-retryable tool error with the offending
  path in the message; the run loop records it like any failed tool call.
- Requests with no resolvable target path (missing `path`, non-string path)
  are treated as unassessable and fall through to the user flow, preserving
  the existing F-07 behavior for malformed-but-promptable requests.

### Races

- Assessment vs. cancellation: assessment is synchronous and completes before
  the guardian/user flows; the existing registry-level cancellation race
  still owns the outcome for the wait paths.
- Assessment vs. store grant: the store auto-accept check runs first, so a
  previously granted write is accepted without re-assessment (unchanged F-07
  semantics).

## Acceptance criteria

- [ ] `[safety]` settings merge correctly across global/project/override with
      `auto-approve-workspace-writes` defaulting to `true` (fixture: settings
      merge unit tests, defaults template check).
- [ ] With defaults, an `edit`/`write` request whose path resolves inside a
      writable root is accepted one-shot: no user prompt is published, no
      session/workspace/permanent grant is written, and an identical second
      request is assessed again (fixture: hostd gateway test).
- [ ] A request targeting a path outside every writable root returns
      `SafetyRejected { reason }`; the orchd registry maps it to a
      non-retryable `safety_rejected` error carrying the reason (fixture:
      hostd gateway test + orchd registry decision test).
- [ ] When the request carries no writable roots, the request falls through
      to the user flow: a pending approval entry is created and a user
      decision resolves it (fixture: hostd gateway test).
- [ ] When `auto-approve-workspace-writes = false`, in-roots writes fall
      through to the user flow exactly as before F-12 (fixture: hostd gateway
      test).
- [ ] Non-write tools (`bash`, `process`, `read`) are unaffected: no safety
      assessment is applied and their approval behavior is unchanged
      (fixture: hostd gateway test + existing registry tests).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Default for auto-approving constrained writes | `true` | Matches codex-rs default behavior (constrained writes auto-approve under an enforced sandbox); the enforcement is piko's own policy layer, so the prompt is friction without safety. Operators can opt out. |
| Where does the assessment run? | hostd approval gateway | hostd is authoritative for approvals (F-07/F-11 precedent); orchd supplies the writable-root evidence. |
| Out-of-roots writes: ask or reject? | Deterministic reject | Execution denies them regardless of approval; a prompt would be a dead decision. |
| Assessment order vs. guardian | Safety first | Deterministic policy wins over model review; the guardian never sees a request the policy can decide. |
| One-shot semantics | No store grant | Mirrors F-11 one-shot allows; every write is re-assessed so grants never bypass the policy gate. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `assess_patch_safety` over `ApplyPatchAction` with permission-profile matrix | **kept (adapted)** | Distilled to writable-root containment for `edit`/`write` targets. piko has no `ApplyPatchAction`/`FileChange` model and no path-grant approval, so the AutoApprove-with-sandbox / Reject-outside-roots cases land directly; the AskUser fallback maps to piko's existing user flow when roots are unavailable. |
| `ElicitationService` pausing tool-result delivery while user elicitations are outstanding | **kept (adapted), deferred to slice 2** | The mechanism is sound (hostd-owned registration counting + watch channel), but piko has no consumer yet: tool calls already await their user response, there is no blocking output-collection loop, and MCP auth elicitation is not landed (F-13). Landed when a consumer exists; recorded in the roadmap. |
| `AttestationProvider` generating `x-oai-attestation` for upstream requests | **rejected** | The header is an OpenAI host-integration mechanism; piko is provider-agnostic through `piko-llmd` and has no app-server surface that could generate or consume it. Shipping an unused hook violates the "no piko consumer" rejection rule (ADR-002). |

## Open questions

1. Should a future path-grant approval flow (M-config permission profiles)
   allow users to approve out-of-roots writes by amending the writable roots?
   If so, `safety_rejected` becomes an AskUser path with a grant proposal.
   Until M-config lands, deterministic rejection is correct.

## Reference evidence

- codex-rs `core/src/safety.rs` — `assess_patch_safety`, writable-path
  normalization, rejection reasons.
- codex-rs `core/src/elicitation.rs` — registration-counted pause service.
- codex-rs `core/src/attestation.rs` — `AttestationProvider`,
  `x-oai-attestation` header.
