# F-34: Execution denial typing and escalation guidance

> Status: implemented (F-34/D-47)
> Priority: P1
> Source evidence: piko product direction; F-23 Rev B

## Summary

The sandbox is the only authority for what a command may do. When a
sandboxed process exits non-zero with an ordinary OS denial (`Read-only
file system`, `Operation not permitted`, `Permission denied`), piko types
that result as `sandbox_denied` so F-23's one approval-backed retry can
fire. Retry permissions follow the access named in the denial text (or
both, when the text does not say). Terminal paths tell the model what to
do next; an approved reusable prefix is reported back on the tool result.

piko does **not** guess write intent from a program blacklist, and it does
**not** walk policy roots to decide whether a path would have been denied.
That would be a second sandbox.

## Problem

F-23 retries once when a tool error is coded `sandbox_denied`, but that
code used to appear only from a pre-spawn policy check or an output line
that literally names the platform sandbox frontend. Real denials arrive as
normal process text, especially under Linux bubblewrap, so the retry never
ran and the model spent turns discovering it needed more authority.

1. **OS denials were untyped.** `mkdir: /opt/x: Operation not permitted`
   and `Read-only file system` looked like ordinary nonzero exits.
2. **Retry roots were always reads.** A write denial retried with a read
   grant failed again.
3. **Terminal paths had no next step.** No approval gateway, a decline, or
   a policy reject left a bare `sandbox_denied` or `declined`.
4. **A static preflight would reimplement the sandbox.** Inferring "this
   `mkdir` / `brew` will fail" from the command line, or re-testing a path
   against the live permission roots, duplicates containment and can prompt
   for commands the real backend would have allowed.

## User journeys

1. The agent runs `mkdir -p /opt/piko-test` under the default sandbox. The
   sandbox denies the write. piko types the OS line as `sandbox_denied`
   (`deny write /opt/piko-test`). The one approval-backed retry adds
   `/opt/piko-test` and its nearest existing ancestor (`/opt`) as write
   roots. One failed spawn, one approval, one success.
2. The agent runs `touch ~/.bashrc` under bubblewrap. The write fails with
   `Read-only file system` or `Operation not permitted`. Same typing and
   single write-root retry.
3. The agent runs `cat /opt/secrets.txt`. It runs sandboxed. If the
   platform denies the read, the OS line is typed and the retry adds a
   read root when the text says read, or both roots when it does not.
4. A headless or queued session has no approval gateway. The denial stays
   `sandbox_denied` and names the alternative: request additional
   permissions or explicit elevation with a justification, or ask the user.
5. After an approved retry that proposed a reusable prefix (`brew
   install`), the successful tool result includes `approved_grant` so later
   matching commands reuse the grant.

## In scope

- Typing sandboxed, non-zero OS denials (read-only filesystem, permission
  not permitted / denied), including output seen while polling a still-
  running command.
- Copying an absolute path out of that OS line into the typed message.
  piko does not decide whether the path is inside or outside policy roots.
- Retry derivation from the typed message: read / write / unknown access;
  no path → explicit elevation.
- Guidance on terminal denial and decline paths, and `approved_grant` on a
  successful prefixed retry.
- A short `exec_command` description that a sandbox denial is retried once
  after approval.

## Out of scope

- Static preflight (command blacklists, argv write-target scanning, a
  second roots walk).
- Network-denial detection (F-23).
- Typing denials on non-sandboxed (elevated) execution.
- Changing the one-retry limit, approval lifecycle, or host-owned grants
  (F-23 / F-07).
- A model-facing `request_permissions` tool.

## Behavior and states

### Runtime denial recognition

A **sandboxed** execution that exits **non-zero** is `sandbox_denied` when
combined output contains:

- `Read-only file system` (write); or
- `Operation not permitted` or `Permission denied`.

If that line already names an absolute path (not a `/dev/…` device), the
typed message copies it:

- read-only filesystem, or the line already says write → `deny write <path>`
- the line already says read → `deny read <path>`
- otherwise → `deny access <path>`

No path → `deny write` or `deny access` with no path; the retry falls back
to explicit elevation. Other nonzero exits stay ordinary process results.
Elevated runs are never typed this way. Zero exits are never denials.

Existing platform-frontend denial lines (`sandbox-exec: deny …`) stay typed
as they are. A bubblewrap setup failure stays `sandbox_unavailable`, not a
retryable denial.

### Retry permission derivation

Retry arguments are derived from the typed message (and existing
platform-frontend / pre-spawn policy text). They do not consult the live
policy.

- read denial → additional read roots for the named paths
- write denial → additional write roots for each path and its nearest
  existing ancestor (so `mkdir /opt/x` can write `/opt`)
- access denial, or a deny line with a path but no read/write keyword →
  both root lists, plus the ancestor on the write side
- no path → explicit elevation (`require_escalated`)

Justification text matches that access. A later check that a write root
would swallow the workspace is a terminal second denial, not a second
retry.

### Terminal denial guidance

- No approval gateway: append that the automatic retry is unavailable and
  name additional permissions, explicit elevation, or ask the user.
- Declined, expired, guardian-, safety-, or permission-rejected retries
  append: prefer a sandboxed alternative or ask the user; do not escalate
  without consent.

### Approved grant visibility

When the retry carried a reusable prefix and succeeds, the tool value
includes `approved_grant: { prefix, note }` stating that commands under
that prefix reuse the grant for the session.

### Model-facing guidance

`exec_command` states that a sandbox denial is retried once after
approval, and that commands known to need broader authority should request
it on the first call with a justification.

## Acceptance criteria

- [x] Sandboxed nonzero output containing `Read-only file system` is
      `sandbox_denied`; a named path becomes a write-root retry (plus
      nearest existing ancestor).
- [x] Sandboxed nonzero `Operation not permitted` / `Permission denied` is
      `sandbox_denied`; any path is taken from the OS line, not re-checked
      against policy roots.
- [x] Elevated commands and zero exits are never typed as sandbox denials
      from output.
- [x] There is no command-blacklist or roots-walk preflight.
- [x] Write / read / unknown / no-path retries match the derivation above.
- [x] No gateway: the error names the missing retry and the alternatives.
- [x] An approved prefixed retry reports `approved_grant`.
- [x] Polling a still-running command uses the same typing as the initial
      execution.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Who decides a path is denied? | The sandbox, by running the command | A second roots walk is a shadow policy. |
| Static preflight? | No | Blacklists drift and can false-positive. One failed spawn is cheaper than a wrong prompt. |
| Which OS text is a typed denial? | Read-only filesystem, permission not permitted / denied, sandboxed nonzero only | Strong signals; other exits stay ordinary. |
| Path for retry? | Copied from the OS or sandbox line | Parsing residue is not re-authorization. |
| Unknown access? | Both read and write roots | Do not invent a writer-program list. |
| Write retry | Path + nearest existing ancestor | Create needs the parent; grant stays narrow. |
| Terminal guidance | On denial/decline messages | Model gets a next step. |
| Grant visibility | `approved_grant` on the tool result | Stops repeat futile requests. |

## Fusion decisions (codex-rs)

F-34 is piko product direction, not a 1:1 port. F-23 remains the authority
model; this feature only types ordinary OS denials so that model already
fires.

| Behavior | Decision | piko landing / rationale |
|---|---|---|
| Backend-owned denial diagnostics only; command stderr stays ordinary (D-35) | **kept (adapted)** | Platform-frontend `sandbox-exec:` lines stay typed. Sandboxed nonzero OS denials (`EROFS` / `EPERM` / `EACCES`) are also typed. Other stderr stays ordinary. |
| One approval-backed retry, narrow additional permissions first | **kept** | F-23 Rev B / ADR-010. |
| Static argv / writer-program preflight | **rejected** | A blacklist is a second policy and can prompt for a command the backend would have allowed. |
| Re-test extracted paths against live permission roots | **rejected** | The sandbox already applied those roots. |
| New sandbox typed-denial API | **rejected** | Sandbox stays a leaf; OS text is enough. |
| Network-denial detection | **rejected** | Remains F-23 out of scope. |

## Open questions

None for this slice.

## Reference evidence

- [F-23](F-23-command-execution-authority.md) — one retry, approval
  lifecycle, host-owned grants.
- [D-35](../design/D-35-command-execution-authority.md) — attempt
  orchestration; §7 backend-diagnostics sentence is amended by D-47.
- [ADR-005](../decisions/ADR-005-execution-authority-containment.md),
  [ADR-010](../decisions/ADR-010-approval-ux-mitigations.md).
- Implementation design: [D-47](../design/D-47-execution-denial-typing-guidance.md).
