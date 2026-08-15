# D-35: Command execution authority and containment

> Status: implemented
> Implements: [F-23](../features/F-23-command-execution-authority.md)
> Decisions: [ADR-005](../decisions/ADR-005-execution-authority-containment.md)

## Goal

Replace piko's static restricted-shell path with a single execution pipeline
that accepts full shell programs, obtains authority from hostd, applies an
enforced filesystem/network profile in piko-sandbox, and manages short and
long-running processes through one orchd-owned runtime.

The design keeps F-08's proven PTY/process-group machinery and F-07's durable
approval lifecycle. It changes policy boundaries and model-facing semantics,
not the hostd/orchd split.

## Constraints and non-goals

- hostd remains authoritative for settings, role/profile selection,
  approvals, grants, guardian decisions, and user-visible approval state.
- orchd remains authoritative for model tools, attempt sequencing, sandbox
  denial handling, and live processes.
- piko-sandbox remains a leaf crate and cannot depend on hostd, orchd, or
  protocol.
- Shared wire types in `piko-protocol` carry facts and decisions, not domain
  services.
- Existing sessions and transcripts remain append-only. Historical old tool
  names are not rewritten and do not imply callable compatibility aliases.
- The first implementation targets macOS and Linux. Windows remains explicit
  unsupported/fail-closed for restricted execution.
- No remote executor, managed network proxy, or shell-to-file-edit
  interception is introduced.
- No attempt is made to prove arbitrary shell safe through parsing.

## Proposed design

### 1. Domain split

```text
model tool call
    |
    v
orchd: normalize ExecutionIntent
    |
    v
hostd ApprovalGateway: authorize AttemptAuthority
    |  policy/grants/guardian/user; durable events
    v
orchd: build ExecutionAttempt
    |
    v
piko-sandbox: enforce EffectivePermissions + spawn
    |
    v
orchd ProcessManager: yield/poll/stdin/cancel/reap
    |
    v
typed ExecutionResult -> transcript/client projection
```

The boundaries use three different types. They must not be aliases for one
large `Policy`:

```rust
struct ExecutionIntent {
    shell_program: String,
    shell: ResolvedShell,
    workdir: PathBuf,
    tty: bool,
    requested_authority: RequestedAuthority,
    justification: Option<String>,
    proposed_prefix_rule: Option<Vec<String>>,
}

enum AuthorizationDecision {
    AllowSandboxed { permissions: EffectivePermissions },
    AllowElevated { grant: GrantedAuthority },
    Reject { reason: AuthorizationRejection },
}

struct ExecutionAttempt {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, String)>,
    permissions: AttemptPermissions,
}
```

`ExecutionIntent` is approval input. `ExecutionAttempt` is already lowered to
`shell -c <program>` (or the selected shell's equivalent) before it enters
piko-sandbox. piko-sandbox never parses shell source or asks for approval.

### 2. Model-facing tools

Replace `bash` and the model-facing parts of `process` with:

```text
exec_command {
  cmd: string,
  workdir?: string,
  tty?: bool,
  yield_time_ms?: integer,
  timeout_ms?: integer,
  max_output_tokens?: integer,
  sandbox_permissions?: "use_default" | "with_additional_permissions" |
                        "require_escalated",
  additional_permissions?: {
    read_roots?: string[],
    write_roots?: string[],
    network?: "restricted" | "enabled"
  },
  justification?: string,
  prefix_rule?: string[]
}

write_stdin {
  session_id: string,
  chars?: string,
  terminate?: bool,
  yield_time_ms?: integer,
  max_output_tokens?: integer
}
```

The `exec_command` description states:

- `cmd` is a program for the selected user shell; normal shell syntax is
  supported;
- `workdir` defaults to the turn cwd and should be preferred over leading
  `cd`;
- a command that outlives the yield window returns `session_id`;
- `yield_time_ms` defaults to 30_000 (Rev B); a still-running command returns
  a `running` result whose text instructs the model to poll the returned
  `session_id` with `write_stdin`;
- `with_additional_permissions` requests narrow extra sandbox authority and
  requires a user-facing `justification`;
- `require_escalated` is only for an attempt that cannot be represented by
  sandbox permissions and also requires a user-facing `justification`;
- a `prefix_rule` is optional, narrow, and valid only with explicit elevation.

`tty` defaults to false so ordinary commands get stable pipe semantics. It is
true only for interactive/TTY-sensitive work. `yield_time_ms` controls when a
live session is returned; it is not the command timeout. `timeout_ms` is an
optional whole-process deadline and retains F-08's process-group termination
semantics. `write_stdin.terminate` requests the same bounded SIGTERM -> SIGKILL
shutdown without relying on terminal control characters.

`bash` and `process` are removed from discovery and routing in the same slice.
There are no adapters or hidden aliases. Historical calls remain in durable
transcripts, while every resumed turn receives only the current tool catalog.
Client process list/stop commands are rewired directly to the unified process
manager identifiers.

### 3. Permission model

Replace sandbox-facing use of `Policy` with an explicit permission model:

```rust
struct EffectivePermissions {
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    denied_read_roots: Vec<PathBuf>,
    denied_write_roots: Vec<PathBuf>,
    scratch_roots: Vec<PathBuf>,
    network: NetworkPermissions,
}

enum NetworkPermissions {
    Restricted,
    Enabled,
}
```

Restricted attempts carry `Some(EffectivePermissions)` into piko-sandbox.
Only a separately approved elevated attempt carries `None` and executes
directly.

Roots are normalized/canonicalized once relative to the session cwd and
materialized with project roots before orchd starts a run. Missing targets are
resolved through their nearest existing ancestor, retaining the current
symlink and TOCTOU protections for direct file tools.

The built-in profile becomes restricted workspace execution:

- session project roots readable and writable;
- `.piko` and host-owned state denied;
- repository `.git` and agent-control metadata (`.codex`, `.agents`) are
  readable but read-only by default, including for shell commands;
- platform scratch roots are explicit and reported to the model;
- platform defaults add read-only access to the standard system toolchain
  (OS binaries, frameworks/dylibs, Homebrew and `/usr/local` bin/lib/Cellar,
  Xcode CommandLineTools) and read-only access to user home configuration;
- platform temp locations are scratch roots (writable, non-persistent);
- network restricted.

Read-only Git operations therefore work in the default sandbox. Commands that
mutate Git metadata, including commit, require a constrained `.git` write
addition when the backend can represent it, or explicit elevation otherwise.
Direct `edit`/`write` tools never gain `.git` access from a command approval.

The toolchain and home-config defaults exist so a platform denial — and its
approval-backed retry — is the exception rather than the rule: ordinary read
commands run inside the default sandbox without prompting. `$HOME` remains
read-only and is never a write root.

`AttemptPermissions` is the effective restricted permissions, that profile
plus approved per-call additions, or an explicit elevated mode. Additional
permissions are intersected with what the active profile permits requesting;
the user may grant a subset without changing the session's ambient profile.

### 4. Execution readiness and platform sandbox

At session bootstrap, orchd asks piko-sandbox for a capability report:

```rust
struct SandboxCapabilities {
    backend: Option<SandboxBackend>,
    filesystem_enforced: bool,
    network_enforced: bool,
    supports_denied_reads: bool,
    scratch_roots: Vec<PathBuf>,
    diagnostic: Option<String>,
}
```

The report is included in the environment fragment. Capability detection does
not weaken policy:

- restricted permissions + capable backend -> sandboxed attempt;
- restricted permissions + missing/broken backend ->
  `sandbox_unavailable`, no spawn;
- unrestricted profile -> direct attempt only when configuration explicitly
  selects it;
- `require_escalated` -> hostd approval, then direct/elevated attempt only if
  the profile permits escalation.

The current `os_sandbox: bool` is removed. Boolean disabled state is too
ambiguous to distinguish an operator's unrestricted choice from unavailable
enforcement.

### 5. Command authorization

hostd adds a pure `domain/exec_policy` evaluator. Its input is a normalized
command authorization request containing raw shell source, lowered shell argv,
cwd, agent role, requested authority, and an optional proposed prefix.

Decision order:

```text
operator forbid rule
  -> explicit operator prompt rule
  -> compatible stored grant
  -> explicit operator allow rule
  -> dangerous-command heuristic
  -> requested elevation
  -> active restricted profile default allow_sandboxed
  -> unrestricted-profile approval policy
```

The classifier follows codex-rs behavior without importing its architecture:

- decompose only shell programs confidently recognized as simple sequences;
- evaluate every decomposed command, with the strongest decision winning;
- for complex programs, optionally recover a conservative leading command for
  prompt context, but never use it to auto-allow the remainder;
- inability to classify means no inferred reusable allow and no syntax error;
- configured rules remain token-array prefix rules, not whitespace string
  prefixes;
- resolve executable identity for matching where practical;
- never propose broad reusable prefixes for shells, interpreters, script
  runners, `git` without a subcommand, destructive utilities, privilege
  escalation, or a complex parse.

The existing F-17 `allowed-commands` and `denied-commands` settings migrate to
structured allow/forbid prefix rules. A compatibility loader accepts strings,
tokenizes simple rules, and warns on ambiguous rules. A future settings schema
may add explicit `prompt` rules.

Command rules decide authorization only. The old `allowed_commands` executable
whitelist is deleted from piko-sandbox and permission-profile materialization.

### 6. Approval and escalation

Extend the orchd-api approval request with:

```rust
enum ApprovalPurpose {
    InitialCommand,
    SandboxEscalation { denial: SandboxDenial },
}

enum RequestedAuthority {
    UseDefault,
    WithAdditionalPermissions(AdditionalPermissions),
    RequireEscalated,
}
```

hostd continues to serialize all user-visible approval requests through its
prompt gate, publish pending/resolved events, apply the configured timeout,
and own scope grants. Approval fingerprints include:

- normalized command identity;
- cwd, with `workdir` lexically normalized against the session cwd (`.`,
  `./`, and the absolute cwd produce one grant);
- agent role;
- requested authority;
- relevant permission-profile identity;
- PTY mode where it changes risk.

An approval for a sandboxed command does not authorize an unsandboxed retry.
If the sandbox returns a typed denial, orchd first derives the narrowest
representable additional permission. When policy permits, it requests that
authority through `SandboxEscalation`; only an unrepresentable denial may
propose full elevation. After acceptance it runs one retry. There is no
recursive escalation. The implementation must honor this ordering —
`with_additional_permissions` before `require_escalated` — rather than
short-cutting to full elevation (Rev B).

Grant matching may key on a proposed narrow prefix in addition to the full
command identity (Rev B): an approved denial retry attaches a reusable
prefix when the command has a stable narrow argv prefix, and a repeat command
whose normalized command starts with that approved prefix reuses the grant.
Prefix proposals follow the same eligibility restrictions as operator prefix
rules (no shells, interpreters, script runners, destructive utilities, or
broad interpreters).

Guardian review remains an approval reviewer, not a sandbox or policy
evaluator. Operator forbids and backend-unavailable failures occur before the
guardian. Stored grants never override operator forbids or expand beyond their
fingerprinted authority.

### 7. Attempt orchestration

orchd introduces an `ExecutionOrchestrator` independent of the generic tool
registry:

```text
authorize(intent)
  -> build default attempt
  -> sandbox.spawn(attempt)
  -> observe/yield
  -> if typed SandboxDenied:
       derive narrow additional authority or evaluate elevation eligibility
       propose reusable narrow prefix when eligible
       request escalation approval
       build one broader attempt
       spawn once
  -> return ExecutionResult
```

The generic tool registry remains responsible for schema validation, tool-call
events, cancellation propagation, result commit, and the single retry budget.
It does not parse shell policy. The workspace provider recognizes only
backend-owned denial/setup diagnostics; arbitrary command stderr remains
ordinary process output. F-34 / D-47 amend this: a sandboxed non-zero exit
whose output contains a recognized OS denial (`Read-only file system`,
`Operation not permitted`, `Permission denied`) is also typed
`sandbox_denied`. Elevated runs, zero exits, and other stderr stay ordinary.

### 8. Unified process manager

Refactor the existing one-shot runner and `ProcessManager` behind one API:

```rust
async fn start(attempt, yield_window, output_budget, cancellation)
    -> ExecutionObservation;
async fn interact(session_id, chars, yield_window, output_budget)
    -> ExecutionObservation;
async fn terminate(session_id) -> ExecutionObservation;
```

An observation contains:

```rust
struct ExecutionObservation {
    state: ExecutionState,
    output: String,
    exit_code: Option<i32>,
    signal: Option<i32>,
    session_id: Option<String>,
    elapsed: Duration,
    truncated: bool,
    original_token_count: Option<usize>,
}
```

`ExecutionState` is `Running`, `Exited`, `Signalled`, `TimedOut`, or
`Cancelled`. Exit code 1 is `Exited`, not a `ToolExecError`. Sandbox denial,
authorization rejection, approval outcomes, unknown session, and spawn/setup
failure remain typed outer errors because no normal child result exists.

Output uses a bounded head/tail or chunked buffer rather than a single 50 KiB
tail. Each poll returns only new output and a chunk id/sequence so clients and
transcripts can avoid duplication. Completed processes are reaped after their
terminal observation has been delivered; abandoned live processes are
cancelled when the owning agent/session ends.

### 9. Workspace file tools

`read`, `edit`, and `write` keep deterministic path authorization. Their
approval evidence uses the same `EffectivePermissions` root projection,
but execution remains direct and path-aware:

- in-root writes may auto-approve one-shot because the handler enforces the
  exact target and re-verifies before mutation;
- out-of-root writes are rejected unless a future explicit additional-root
  flow is implemented;
- scratch writes are allowed only in reported scratch roots;
- `.piko` and other host-owned state remain denied;
- no shell command classification is involved.

This keeps F-12's valuable direct-write guarantee while removing the false
assumption that the same check constrains arbitrary shell commands.

### 10. Error taxonomy

Stable error codes:

| Layer | Codes / states |
|---|---|
| Argument | `invalid_args`, `invalid_workdir`, `missing_justification` |
| Authorization | `permission_denied`, `approval_declined`, `approval_expired` |
| Containment | `sandbox_unavailable`, `sandbox_denied`, `sandbox_setup_failed` |
| Process setup | `spawn_failed`, `unknown_session` |
| Process result | `running`, `exited`, `signalled`, `timed_out`, `cancelled` |
| Internal | `internal_error` |

Errors include structured recovery hints where deterministic. For example,
`sandbox_denied` identifies the denied authority and whether explicit
elevation is eligible; `invalid_workdir` reports the resolved base; and
`missing_justification` tells the model which field is required.

### 11. Settings transition

The settings model separates permission posture from shell selection:

```toml
[permissions]
profile = "workspace"

[permissions.profiles.workspace]
read-roots = ["."]
write-roots = ["."]
scratch-roots = ["/tmp"]
allow-network = false
allow-escalation = true

[execution]
shell = "/bin/zsh" # optional; environment discovery is the default
```

The backend is selected by platform (`seatbelt` on macOS, `bwrap` on Linux)
and enforcement is mandatory for restricted attempts. There is no sandbox
disable switch, policy-JSON loader, or executable whitelist. Direct execution
exists only as a separately approved elevated attempt.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Add execution environment facts and typed approval/result DTOs needed across host/client boundaries; keep live-process internals private. |
| `piko-hostd` | Own `exec_policy`, effective profile resolution, elevation decisions, approval fingerprints/events, settings migration, and environment facts exposed to clients/model prompts. |
| `piko-orchd` | Replace `bash`/`process` with `exec_command`/`write_stdin`, add `ExecutionOrchestrator`, full-shell lowering, typed sandbox-denial retry, and unified result mapping. |
| `piko-sandbox` | Replace shell validation/command whitelist with structured attempt enforcement, capability discovery, platform backend selection, typed denial/setup errors, and unified process spawn primitives. |
| `piko-tui` | Render initial/escalation approval purpose, effective requested authority, execution state, and live-session controls without owning policy decisions. |

## Reusable infrastructure

No `island-rs` change required.

The existing piko PTY/process-group runner, cancellation token propagation,
approval gateway, permission settings merge, direct file authorization, and
timeline tool-call projection are reused behind new domain boundaries.

## Failure and cancellation

- Invalid arguments fail before authorization and never create approval state.
- Operator forbid is terminal and cannot be escalated.
- Approval decline/expiry is terminal for that attempt and writes no grant.
- Sandbox backend setup failure never falls back to direct execution.
- A typed sandbox denial can trigger at most one separately authorized retry.
- Nonzero child exit is returned without retry.
- Turn cancellation cancels pending approval or the active process group,
  records a bounded terminal observation, and reaps the process.
- Session shutdown cancels all owned live processes after a bounded grace.
- A client disconnect does not change host-owned approval deadlines or orchd
  process ownership.
- Output truncation never stops draining the child pipe/PTY and therefore
  cannot deadlock a chatty process.

## Verification

- Unit tests for shell lowering, policy classification, structured prefix
  matching, banned prefix suggestions, permission materialization, error
  mapping, and settings migration.
- piko-sandbox tests for backend capability detection, filesystem/network
  enforcement, scratch roots, symlink escape, typed denial, backend failure,
  process groups, timeout, cancellation, and output drain after truncation.
- orchd integration tests for full-shell constructs, workdir, initial yield,
  write/poll, nonzero exits, one escalation retry, no retry loops, and legacy
  adapters.
- hostd integration tests for decision ordering, durable approval purpose,
  fingerprints/scopes, guardian placement, denial/expiry, and role profiles.
- TUI/client projection tests for running/exited/error states and escalation
  prompts.
- Differential fixtures against codex-rs for full-shell acceptance,
  workdir-first guidance, exec-policy complex parse fallback, sandboxed-first
  execution, explicit elevation, nonzero exit output, and unified process
  continuation.
- Adversarial tests showing `python`, `node`, build scripts, alternate binary
  paths, and shell substitutions remain contained without relying on command
  names.

## Alternatives considered

### Keep the validator and document its shell subset

Rejected. It would reduce model retries only after substantial prompt tuning,
would still diverge from ordinary shell, and would not turn command-name
checks into containment.

### Replace shell with only structured program + argv

Rejected as the sole model-facing tool. It is easier to authorize but poorly
matches coding-model output for pipelines, redirects, environment setup, and
multiline scripts. piko-sandbox still receives structured argv after orchd
lowers the full shell program.

### Allow full shell only when the platform sandbox is enabled

Rejected as an implicit mode switch. Tool syntax must not change with backend
availability. Restricted execution fails closed; explicitly unrestricted
execution remains a separate authority choice.

### Ask for approval for every shell command

Rejected. Approval is not containment and routine prompts train users to
approve mechanically. Commands confined to the active restricted profile
should run autonomously unless policy identifies a specific risk.

### Port codex-rs tool/runtime architecture directly

Rejected. Its behavioral separation is useful, but piko must keep hostd
authoritative for user-visible state and orchd responsible for agent runtime.

## Implementation slices (landed)

1. **Contracts and observability**: introduce typed permissions,
   authorization purpose, sandbox capabilities, execution observations, and
   error taxonomy without changing the advertised tools.
2. **Sandbox boundary**: make platform enforcement capability explicit,
   remove silent direct fallback for restricted profiles, and add typed denial
   results. Keep the old validator temporarily ahead of execution.
3. **Full-shell execution**: replace `bash`/`process` with
   `exec_command`/`write_stdin`, unified process manager, workdir, output
   budgets, and normal nonzero-exit results; remove static syntax rejection.
4. **Authorization policy**: land hostd `exec_policy`, structured prefix
   rules, conservative complex parsing, dangerous-command prompts, and banned
   reusable-prefix suggestions. Delete the sandbox executable whitelist.
5. **Additional authority and elevation**: add constrained per-call
   permissions, explicit elevation, typed denial -> hostd approval -> single
   retry, scope-safe fingerprints, and TUI purpose display.
6. **Settings and catalog cutover**: replace `[sandbox]` with `[execution]`,
   expose capability facts, advertise only new tools, and delete all old-name
   routing and policy JSON loading.
7. **Closeout**: update F-08/F-12/F-17 and their designs/verification to point
   to the landed F-23 behavior, remove legacy code/config, and record V-35
   differential and adversarial evidence.
