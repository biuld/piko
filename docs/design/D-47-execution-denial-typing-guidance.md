# D-47: Execution denial typing and escalation guidance

> Status: implemented
> Implements: [F-34](../features/F-34-execution-denial-typing-guidance.md)
> Decisions: [ADR-005](../decisions/ADR-005-execution-authority-containment.md), [ADR-010](../decisions/ADR-010-approval-ux-mitigations.md)

## Goal

Type ordinary OS sandbox denials as `sandbox_denied` so F-23's one
approval-backed retry fires; derive retry roots from the denial **text**,
not from a second `EffectivePermissions` walk; tell the model what to do
when the retry cannot run; report approved reusable prefixes.

The sandbox remains the only containment authority. orchd does not
blacklist programs and does not re-test paths against writable roots.

## Constraints and non-goals

- hostd owns approvals, grants, and user-visible state; orchd owns the
  attempt, denial typing, and retry-argument derivation (F-23 / D-35).
- `piko-sandbox` stays a leaf. No new sandbox API.
- One-retry limit, approval lifecycle, and host grant store are unchanged.
- No network-denial detection.
- No static preflight (argv writer lists, escaping-tool lists, roots walk).
- Workdir `policy.authorize()` before spawn stays F-23 containment of the
  working directory. It is not a command-argv preflight.

## Proposed design

### Amendment to D-35

D-35 §7 said the workspace provider recognizes only backend-owned
denial/setup diagnostics and that arbitrary command stderr stays ordinary
process output. F-34 keeps that default and adds one exception:

a **sandboxed, non-zero** exit whose output contains a recognized OS
denial is typed `sandbox_denied`. Other stderr, elevated runs, and zero
exits stay ordinary.

### 1. Denial message contract

`exec_handlers/support.rs` emits `sandbox_denied` messages.
`registry/denial.rs` parses them. Shared text:

```text
sandbox denied: deny write /opt/piko-test
sandbox denied: deny read /opt/secrets.txt
sandbox denied: deny access /opt/x
sandbox denied: deny write
sandbox-exec: deny file-read-data /opt/homebrew/bin/magick
```

Classification (substring on the line, after requiring `deny`):

- `file-write` or `deny write` → write
- `file-read` or `deny read` → read
- otherwise → unknown (retry adds both root kinds)

`denied` in `sandbox denied` must not count as read. Pre-spawn
`policy.authorize()` messages that already contain `deny` stay parseable.

### 2. Runtime typing

`sandbox_observation_error(sandboxed, chunk)` is the same helper for
`exec_command` and `write_stdin`:

1. Not sandboxed, still running, or exit 0 → `None`.
2. Output contains `sandbox-exec:` and (`deny` or `operation not permitted`)
   → `sandbox_denied` with the raw output.
3. Output starts with `bwrap:` → `sandbox_unavailable` (not retryable).
4. Else any line with `read-only file system`, `operation not permitted`,
   or `permission denied` → `sandbox_denied` with a normalized
   `deny write|read|access [path]` line. The path, if any, is an absolute
   token already on that OS line (`/dev/…` ignored). No policy lookup.

`typed_os_denial` chooses the access keyword from the same line: EROFS or
a write token → `write`; a read token → `read`; otherwise `access`.

### 3. No command preflight

Do not intercept `mkdir /opt/…` or `brew install` before spawn. A
program/path blacklist or a roots walk is a shadow sandbox: incomplete,
and able to prompt for a command the backend would have allowed.

One failed spawn is cheaper than a wrong approval prompt.

### 4. Retry derivation

`registry/denial.rs::denial_retry_args`:

- write paths → `write_roots` + nearest existing ancestor
- read paths → `read_roots`
- unknown paths → both, plus ancestor on the write side
- no paths → `require_escalated`
- optional `prefix_rule` when the command is a simple program + subcommand
  (same eligibility as F-23)

`validate_write_roots` still rejects a root that would swallow the
workspace; that is a terminal second denial.

### 5. Terminal guidance and grant visibility

- No gateway: append `NO_GATEWAY_RETRY_NOTE` to the `sandbox_denied`
  message.
- Non-accepting retry decision: `approval_failure` appends a next-step
  sentence (sandboxed alternative or ask the user; no silent escalate).
- Successful retry with `prefix_rule`: `attach_approved_grant` adds
  `approved_grant: { prefix, note }` on the tool value.

### 6. Tool description

`exec_command_tool_def` states that a sandbox denial is retried once after
approval, and that commands known to need broader authority should request
it on the first call with a justification.

## Package impact

| Package | Change |
|---|---|
| `piko-orchd` | OS-denial typing in `exec_handlers/support.rs`; retry, guidance, grant in `registry/denial.rs`; tool description in `exec_handlers.rs`; tests |

Unaffected: `piko-protocol`, `piko-hostd`, `piko-llmd`, `piko-sandbox`.

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Typing is a pure scan of process output. Ambiguous or non-denial text
  leaves the ordinary nonzero result.
- A second failure after the one retry is returned to the model; there is
  no third attempt.
- Cancellation and timeout stay F-23 process-lifecycle codes; they are not
  typed as sandbox denials.

## Verification

- `exec_handlers/support.rs`: EROFS → `deny write <path>`; EPERM copies
  the OS path as `deny access`; exit 0 / unsandboxed / ordinary nonzero
  stay untyped.
- `registry_retry_tests.rs`: write denial → `write_roots` + ancestor;
  `sandbox-exec: deny file-read-data` still → `read_roots`; no-path →
  `require_escalated`; no gateway appends guidance; prefixed success
  reports `approved_grant`.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Static argv preflight (writer-program list, write-target scan) | Incomplete, drifts, and can prompt for a command the backend would have allowed. |
| Re-test extracted paths against `EffectivePermissions` roots | A second sandbox. The backend already applied those roots. |
| New sandbox typed-denial API | Sandbox stays a leaf. OS text plus existing `sandbox-exec:` lines are enough. |
| Treat every OS denial as `require_escalated` | Violates F-23 Rev B: prefer the narrowest additional permission. |

## Rollout

Already landed with F-34. No further slices.
