# F-23: Command execution authority and containment

> Status: implemented (F-23/D-35/V-35)
> Priority: P0
> Source evidence: codex-rs `core/src/tools/handlers/shell_spec.rs`,
> `core/src/tools/handlers/unified_exec/`, `core/src/tools/orchestrator.rs`,
> `core/src/tools/sandboxing.rs`, `core/src/exec_policy.rs`,
> `core/src/unified_exec/`, `sandboxing/`, `execpolicy/`, and
> `shell-command/`

## Summary

piko provides one coherent command-execution capability that accepts normal
shell programs as coding models produce them, executes them under an enforced
filesystem and network boundary, and asks for approval only when authority is
missing or policy explicitly requires it. Command authorization, sandbox
containment, process lifecycle, and command results have distinct observable
semantics. A policy parser may reduce prompts for commands it understands, but
must never reject valid shell solely because it cannot fully parse it.

This feature supersedes conflicting command behavior
in F-08, F-12, and F-17. It retains F-08's PTY, process-group, timeout,
cancellation, and long-lived process behavior; F-07 remains the approval
lifecycle contract.

## Problem

The current `bash` tool promises shell execution while accepting only an
undocumented subset. It rejects common model output such as `cd`, command
substitution, heredocs, multiline quoted arguments, shell builtins, and
environment assignments before a sandbox attempt. The restriction does not
form a sound security boundary: allowed interpreters and build tools can
perform arbitrary work, command arguments are not generally path-checked, and
the platform sandbox may be disabled.

Several independent concerns are currently collapsed into one policy:

1. whether a command is authorized;
2. which filesystem and network effects are technically contained;
3. how a process is spawned, observed, cancelled, and resumed;
4. whether the command itself exited successfully;
5. whether a workspace file mutation is safe to auto-approve.

This produces avoidable tool retries, misleading errors, and configurations
whose apparent safety is stronger than their enforcement. It also makes it
difficult to explain the effective execution boundary to users and models.

## User journeys

1. The agent creates a multiline Git commit message with a heredoc. The shell
   accepts it unchanged and runs it inside the default workspace boundary; no
   syntax-policy error occurs.
2. The agent runs `cargo test` in the session workspace. The command does not
   request authority beyond the active restricted profile, so it runs without
   an approval prompt.
3. A command needs one extra writable root or network access. The agent asks
   for that constrained additional authority with a user-facing justification;
   the user can approve or decline it through the existing bounded approval
   flow. The retry remains sandboxed with only the granted addition.
4. A sandboxed command is denied by the platform boundary. piko identifies the
   denial separately from an ordinary nonzero exit and, when policy permits,
   offers one approval-backed retry with the required broader authority.
5. `grep` finds no matches and exits with code 1. The tool reports a completed
   process with `exit_code = 1`; it does not report a tool infrastructure
   failure.
6. A command remains active after the initial output window. The call returns
   a process/session id, and the agent continues through `write_stdin` without
   creating a second process abstraction.
7. The configured restricted profile requires an OS sandbox backend that is
   unavailable. piko fails closed with `sandbox_unavailable`; it never silently
   runs the command directly. An unsandboxed attempt requires an explicit
   elevation request and approval when the active policy permits escalation.

## In scope

- A model-facing `exec_command` tool accepting a shell program, explicit
  working directory, bounded initial wait, optional PTY, and output budget.
- A model-facing `write_stdin` tool for polling, interaction, and continued
  output from a live command.
- Full input support of the selected user shell, including quoting,
  redirection, pipelines, substitutions, heredocs, multiline programs,
  builtins, and environment assignments.
- The session/turn working directory is explicit in the tool contract. Models
  are instructed to use it instead of leading `cd` commands, but `cd` remains
  valid shell when needed.
- Restricted execution is the default for model-originated commands. The
  active permission profile defines readable roots, writable roots, denied
  reads, scratch roots, and network authority.
- The platform sandbox is the enforcement boundary for commands. Static
  command classification is authorization evidence only.
- A tri-state command authorization decision: allow inside the active
  sandbox, require approval, or forbid. Explicit operator forbids win over
  stored grants and model requests.
- Best-effort command classification for policy decisions. Simple command
  sequences may be decomposed. Complex or unrecognized shell remains
  executable in the sandbox but cannot gain an automatic reusable allow rule
  from inferred structure.
- Explicit per-call constrained-additional or elevated authority with a
  required user-facing justification. Constrained additions are preferred;
  approval never silently broadens the active profile.
- One sandbox-denial retry at most, after a separate approval when required;
  no retry for ordinary process exits, spawn errors, cancellation, or timeout.
- Process result states distinguish running, exited, signalled, timed out,
  cancelled, sandbox denied, policy rejected, approval rejected/expired, and
  infrastructure failure.
- Nonzero process exit codes are normal execution results.
- Workspace `read`/`edit`/`write` tools continue to use direct path
  authorization and do not route file content through a shell.
- The effective execution profile, cwd, shell, scratch roots, platform
  backend, and network posture are visible to the model through environment
  context.
- Durable approval requests and resolutions remain host-owned and visible to
  clients; transient process supervision remains agent-runtime-owned.
- Immediate replacement of the model-facing `bash` and `process` tools; no
  legacy-name routing or catalog compatibility is retained.

## Out of scope

- Reproducing the codex-rs crate graph, central session coupling, remote
  executor architecture, or protocol types.
- A general parser that proves the safety of arbitrary shell.
- Treating command allowlists as filesystem or network containment.
- Automatic unsandboxed fallback when a sandbox backend is missing or denies
  an operation.
- Windows sandbox support in the first implementation slices.
- Host/port-specific network approvals and a managed network proxy. These
  require a separate product journey beyond the current boolean network
  boundary.
- A standalone `request_permissions` tool. Command elevation is expressed on
  the command call until another tool family has a concrete consumer.
- Persisting live processes across hostd/orchd restart.

## Behavior and states

### Authority model

Every command has four independent inputs:

- **program intent**: shell source, selected shell, cwd, PTY, and environment;
- **ambient authority**: the active role/session permission profile;
- **requested authority**: default sandbox, constrained additions, or explicit
  elevation;
- **authorization policy**: operator rules, stored grants, guardian, and user
  decision.

Authorization produces one of:

```text
allow_sandboxed(profile)
allow_elevated(approved scope)
prompt(reason, requested authority)
forbid(reason)
```

An approval is authority to make a particular attempt; it is not itself the
mechanism that constrains that attempt.

### Command flow

```text
validate tool arguments
  -> resolve shell + cwd + active permission profile
  -> classify command for authorization (best effort)
  -> host-owned authorization decision
       forbid ------------------------------------> policy_rejected
       prompt -> reject/expire -------------------> approval terminal
       allow/prompt -> accept
  -> select enforced attempt
  -> spawn and observe
       normal exit (including nonzero) -----------> exited result
       still active after yield ------------------> running result + session id
       timeout/cancel/signal ---------------------> matching process result
       sandbox denial ----------------------------> typed denial
          -> policy permits retry + approval -----> one elevated retry
          -> otherwise ---------------------------> sandbox_denied
       backend unavailable -----------------------> sandbox_unavailable
```

### Default behavior

- The built-in profile is restricted workspace execution: project roots are
  readable and writable, `.git`/`.codex`/`.agents` are read-only, `.piko` is
  denied, network is restricted, and declared scratch roots are writable.
- Commands inside that authority do not prompt merely because they are shell
  commands.
- Known-dangerous or operator-prompt rules may still require approval before a
  sandboxed attempt.
- Selecting unrestricted execution is an explicit configuration or approved
  per-call elevation, never the meaning of “sandbox disabled because no
  backend was found.”

### Command classification

- Classification recognizes simple argv and shell sequences for operator
  allow/prompt/forbid rules and reusable approval suggestions.
- If only a safe prefix of a complex program can be understood, it may inform
  a prompt but must not authorize the remaining program.
- Parse failure never yields `unsupported_shell_syntax`.
- Broad interpreters, shells, package-script runners, destructive utilities,
  and commands without a stable narrow prefix are ineligible for automatic
  reusable-prefix suggestions.
- A small bundled dangerous-command classifier may require a prompt but can
  never forbid by itself; only explicit operator rules forbid.
- A configured forbid is an authorization rule, not claimed containment. The
  sandbox still applies even when a command is allowed.

### Process results

A successful tool invocation means piko successfully evaluated and attempted
the command. It may contain a nonzero exit code. Tool errors are reserved for
events outside normal program semantics: invalid arguments, policy/approval
rejection, sandbox setup/denial after retry resolution, spawn failure, unknown
session id, or internal failure.

The result includes bounded output, truncation metadata, elapsed time, and
exactly one of an exit code or a live session id where applicable. Polling is
incremental and does not repeat already-consumed output unless explicitly
requested by a future API.

### Tool replacement

- The model catalog advertises only `exec_command` and `write_stdin`.
- Calls to `bash` or `process` fail as unknown tools; no hidden alias exists.
- Historical transcript records remain immutable. They are context evidence,
  not callable tool definitions for the resumed turn.
- Client-owned process list/stop controls move directly to the new process
  manager identifiers.

## Acceptance criteria

- [x] Multiline shell, heredocs, command substitution, environment assignment,
      pipelines, and shell builtins reach the selected shell without a static
      syntax rejection (differential fixtures from codex-rs shell tools).
- [x] `workdir` selects the command directory without requiring `cd`; a valid
      explicit `cd` program also executes.
- [x] Under the default restricted profile, an ordinary workspace command
      executes without a user prompt and cannot write outside the effective
      writable and scratch roots.
- [x] When restricted execution is required and no backend is available, the
      command returns `sandbox_unavailable` and is not launched directly.
- [x] Static command classification never substitutes for containment:
      interpreters and build tools remain confined by the same filesystem and
      network policy as every other command.
- [x] An unclassified complex shell program is not rejected; it runs inside
      the sandbox unless an independent policy requires prompt/forbid.
- [x] Operator forbid rules resolve before stored grants, guardian review, and
      user approval, and cannot be overridden by elevation.
- [x] An explicit elevation request without a justification fails argument
      validation; constrained additional permissions are preferred over
      elevation, and an approved request gets only the described attempt and
      scope.
- [x] A sandbox denial is distinguishable from exit code 1 and may cause at
      most one approval-backed retry; an ordinary nonzero exit is never
      retried automatically.
- [x] Exit codes 1 and 127 are returned as completed process results, not as
      `command_failed` tool errors.
- [x] A still-running process returns a session id and can be polled, written,
      cancelled, and reaped without duplicate process creation.
- [x] Approval request/resolution remains durable and reconstructable through
      hostd session state while process output remains runtime-streamed.
- [x] The environment surface reports effective shell, cwd, sandbox backend,
      filesystem/network posture, and scratch roots in terms the model can act
      on.
- [x] `bash` and `process` are absent from discovery and have no executable
      routing path; the old static validator is unreachable.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Shell subset or real shell? | Real selected shell | Matches model output and the tool promise; containment belongs below parsing. |
| What is the security boundary? | Enforced platform filesystem/network sandbox | Command-name checks cannot constrain interpreters or arguments. |
| Default command posture | Restricted workspace execution without routine prompts | Safe autonomy is the primary coding-agent journey. |
| What if the sandbox backend is unavailable? | Fail closed for restricted execution | “Unavailable” must not silently mean full host authority. |
| Role of command parsing | Authorization optimization only | Parse uncertainty reduces auto-approval; it does not invalidate shell. |
| Nonzero exit | Normal process result | Exit status belongs to the child program, not tool infrastructure. |
| Escalation | Explicit request or typed denial followed by approval; one retry | Makes authority changes visible and prevents retry loops. |
| Additional authority | Prefer sandboxed per-call additions; reserve unsandboxed elevation for effects that cannot be represented | Least authority preserves containment. |
| Dangerous-command heuristics | Bundled prompt-only heuristic; operator rules alone may forbid | Reduces accidents without pretending heuristics are enforcement. |
| Process tool shape | `exec_command` + `write_stdin` | One lifecycle covers short, long, interactive, and polled commands. |
| Workspace file mutations | Keep dedicated path-aware tools | They provide stronger intent and deterministic path enforcement than shell. |
| Durable owner | hostd owns policy/approval state; orchd owns attempts/processes | Preserves piko's host/orchestrator split. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `exec_command` with `workdir`, PTY/yield, output budget, and `write_stdin` | kept (adapted) | Replace the split `bash`/model-facing `process` surface while retaining piko's process manager. |
| Full shell source passed to the selected shell | kept | Removes the undocumented shell subset and matches model behavior. |
| Shell parsing feeds exec-policy but complex parsing does not reject execution | kept | Authorization may be conservative; sandbox enforcement remains independent. |
| Central approval -> sandbox selection -> attempt -> optional escalated retry flow | kept (adapted) | orchd owns transient orchestration; hostd remains the approval authority. |
| Explicit additional permissions / `require_escalated`, justification, and reusable prefix proposal | kept (adapted) | Prefer constrained sandbox additions; use piko approval scopes and host-owned stores; broad prefix suggestions are prohibited. |
| Permission profile separates filesystem and network dimensions | kept (adapted) | Replace the overloaded piko `Policy` projection with an explicit effective profile. |
| Sandbox denial distinct from process exit | kept | Required for correct retry and result semantics. |
| Nonzero process exit returned in tool output | kept | Avoid false tool failures such as `grep` with no match. |
| Apply-patch interception inside shell execution | rejected | piko already has dedicated `edit`/`write` tools; shell interception would couple command execution to mutation modeling. |
| Managed network proxy and deferred domain approval | deferred | No current piko consumer; boolean network containment remains. |
| Remote execution environments and foreign path conventions | deferred | piko currently executes in one local session environment. |
| codex-rs central session/tool orchestrator architecture | rejected | piko keeps hostd authority and orchd runtime ownership. |

## Reference evidence

- codex-rs `core/src/tools/handlers/shell_spec.rs` — model-facing full-shell
  schema, explicit workdir, unified exec and approval parameters.
- codex-rs `core/src/tools/handlers/unified_exec/` — argument resolution,
  environment selection, shell lowering, and handler/result adaptation.
- codex-rs `core/src/tools/orchestrator.rs` — approval, sandbox attempt, typed
  denial, and approval-backed retry flow.
- codex-rs `core/src/tools/sandboxing.rs` — separate approval and sandbox
  runtime contracts.
- codex-rs `core/src/exec_policy.rs`, `execpolicy/`, `shell-command/` —
  best-effort classification, rule decisions, dangerous/safe heuristics, and
  restrictions on reusable prefix suggestions.
- codex-rs `core/src/unified_exec/` — one process manager for initial yield,
  long-lived sessions, incremental output, and exit state.
- piko F-07, F-08, F-12, and F-17 — existing approval, execution, write
  safety, and permission-profile behavior superseded only where this PRD
  explicitly conflicts.
