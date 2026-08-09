# F-17: Permission profiles (materialized file/network/command policies)

> Status: superseded in part by F-23 (profile and role selection retained)
> Priority: P1
> Source evidence: codex-rs `core/src/config/*` (config layers, permissions,
> permission-profile catalog, managed features, agent roles, schema),
> `core/src/safety.rs`, `core/src/tools/handlers/request_permissions.rs`,
> `core/src/exec/*` (command allow/deny prefix rules)

> [F-23](F-23-command-execution-authority.md) replaces the
> executable whitelist and string-prefix command path with separated
> authorization and enforced containment.

## Summary

Permission profiles are named bundles of file, network, and command policy,
configured in the settings layers hostd already owns
(`~/.piko/settings.toml`, `.piko/settings.toml`, overrides). The active
profile is resolved at session start and materialized into (a) the sandbox
policy used for workspace and exec tools — read/write roots, deny paths,
network allow — and (b) the approval gateway's command policy — commands
matching an `allowed-commands` prefix execute without a prompt (one-shot),
commands matching a `denied-commands` prefix fail closed before any prompt
or execution.

## Problem

1. **Sandbox policy is a JSON file or a hardcoded default.** There is no
   settings-native way to express a project's safety posture — which paths
   are writable, what network access, which commands — that merges across
   the same global/project/override layers every other hostd setting uses.
   An operator must hand-author a policy JSON file (or accept the default)
   and cannot compose policy from layered settings.
2. **Command approvals are per-session and reactive.** The only way to stop
   prompting for a routine command (`cargo test`, `git status`) is to grant
   a session/workspace/permanent approval at runtime. An operator cannot
   declare "these commands never prompt" in configuration so it applies to
   every session and every user of the project.
3. **No declarative, fail-closed command deny.** Blocking a command an
   operator considers dangerous (`rm -rf ~`, `curl | sh`) currently depends
   on a human denying prompts one session at a time. Nothing in
   configuration enforces a deny deterministically before execution.

## User journeys

1. An operator adds `[permissions] profile = "locked"` to
   `.piko/settings.toml` with `write-roots = ["."]`, `allow-network = false`,
   and `denied-commands = ["rm -rf", "curl -sSL | sh"]`. Every session in
   the project gets a sandbox that denies writes outside the listed roots,
   no network, and any matching command fails closed with
   `permission_denied` before execution or prompting.
2. An operator declares `allowed-commands = ["cargo test", "git status"]`
   in the active profile. The agent calls `cargo test -- --nocapture`; the
   command matches the prefix and executes without a user prompt. No
   approval grant is written, so a different command still prompts.
3. Global settings define a shared `"dev"` profile (`write-roots = ["."]`,
   `allow-network = true`, `allowed-commands = ["npm run dev", "cargo test"]`).
   A project overrides only `profile = "dev"` and inherits the profile
   definition from the global layer.
4. A user with no `[permissions]` section runs piko. Behavior is unchanged:
   sandbox policy resolution follows the existing file/default path, and all
   on-request approvals prompt as before.
5. An operator sets `[permissions] profile = "typo"` (no such profile). hostd
   warns and falls back to the built-in `default` profile, preserving today's
   behavior rather than silently weakening to something permissive.

## In scope

- `[permissions]` settings: `profile` (active profile name, default
  `"default"`) and a `profiles` map of named profile definitions; merged
  across global/project/override like other settings sections.
- Profile fields:
  - file/network: `read-roots`, `write-roots`, `deny-paths`,
    `allow-network`;
  - command: `allowed-commands`, `denied-commands` (token-boundary prefix
    rules on the command string).
- Built-in `default` profile matching today's permissive sandbox default:
  `read-roots = ["."]`, `write-roots = ["."]`, `deny-paths = [".git",
  ".piko"]`, `allow-network = false`, no command rules.
- Materialization at session start (hostd owns the resolution):
  - file/network rules → the sandbox `Policy` used by workspace/exec tools;
  - command rules → the approval gateway.
- Sandbox policy precedence with a configured profile:
  `[sandbox] policy-path` file > permission-profile materialization >
  `.piko/sandbox.json` > permissive default. When a profile is materialized,
  the sandbox execution whitelist (`allowedCommands`) is inherited from the
  permissive default list, not from the profile's `allowed-commands`.
- Approval gateway command policy:
  - `denied-commands` prefix match → `PermissionDenied { reason }`,
    non-retryable `permission_denied` tool error, checked before store
    grants, guardian, and the user flow (operator deny wins over prior
    grants);
  - `allowed-commands` prefix match → one-shot `Accept` (no store grant), so
    a later non-matching command is assessed again;
  - non-matching commands keep the existing F-07 user flow unchanged;
  - non-command tools (`edit`, `write`, `read`, `environment`, …) are
    unaffected.
- Command extraction: `bash` tool `command` argument; `process` tool with
  `action = "start"` reads its `command` argument (same identity as the
  F-08 approval fingerprint).
- `resources/settings.default.toml` documents the section.

## Out of scope

- Managed-feature gating (tool enablement by feature flag / policy) — later
  M-config slice.
- Agent-role layers (per-role profile selection) — later M-config slice.
- `request_permissions`-style tool elevation (no piko consumer;
  out-of-policy commands fail closed or prompt through the F-07 flow).
- Network endpoint granularity (host/port allow lists): the sandbox exposes
  a boolean network allow today; profiles mirror that.
- Changing tool approval tiers (`never` / `on-request` / `always`) or the
  sandbox execution whitelist from profiles (the whitelist stays
  policy-file/default driven in this slice).
- Dynamic profile switching mid-session; profiles are resolved per session
  start from the merged settings at that time.

## Behavior and states

### Profile resolution

```text
[permissions] sections: global → project → override (field-level merge)
  profile: scalar override (override wins)
  profiles: per-name replace (override entry replaces same-named base)
  active profile = merged profiles[merged profile] or built-in "default"
    user-defined profile ───────────────────────> materialize
    built-in "default" ─────────────────────────> no materialization
    unknown name ───────────────────────────────> warn + built-in "default"
```

- No `[permissions]` section: active profile is the built-in `default`, but
  no profile materialization happens — sandbox policy resolution is
  unchanged (file path → default file → permissive).
- The built-in `default` profile (selected explicitly or by default) never
  materializes: it is identical to configuring nothing, so it cannot shadow
  `.piko/sandbox.json` or any other existing policy source. Only
  user-defined profiles materialize.

### Materialization

- A selected user-defined profile's file/network fields produce the sandbox
  `Policy` when no `[sandbox] policy-path` file exists: `read-roots` →
  `read`, `write-roots` → `write`, `deny-paths` → `deny`,
  `allow-network` → `allowNetwork`, `allowedCommands` inherited from the
  permissive default whitelist. Empty rule lists inherit the permissive
  defaults per field, so a partial profile does not lock down access
  unexpectedly.
- Command rules are always applied when the profile (built-in or custom)
  defines them; the built-in default defines none, so default behavior is
  unchanged.

### Command rule evaluation (approval gateway)

```text
command tool call needing approval (bash / process start)
  ├─ denied-commands prefix match ────────> permission_denied (fail closed,
  │                                          before store grants / prompts)
  ├─ store auto-accept match ─────────────> accept (unchanged F-07)
  ├─ allowed-commands prefix match ───────> accept (one-shot, no grant)
  ├─ F-12 safety gate ────────────────────> (unchanged)
  ├─ F-11 guardian ───────────────────────> (unchanged)
  └─ user flow (F-07) ────────────────────> (unchanged)
```

- Prefix rule matching: both the rule and the command are whitespace-
  normalized; a rule matches when the command starts with the rule followed
  by a space or end-of-string (token boundary). `cargo test` matches
  `cargo test -- --nocapture` but not `cargo testrun`; `git` matches
  `git status` but not `gitlab-ci`.
- `permission_denied`: terminal, non-retryable tool error carrying the
  offending command prefix and reason; the run loop records it like any
  failed tool call.
- One-shot allow: the tool executes for this call only; the approval store
  is untouched.
- Unparseable command arguments (missing/non-string `command`) are not
  matched and keep the existing flow.

### Races

- Deny vs. store grant: the deny check runs before the store auto-accept
  check, so an operator deny wins over a previously granted approval.
- Profile resolution vs. cancellation: resolution is synchronous at session
  start and completes before any tool approval; the existing
  registry-level cancellation race still owns the outcome for wait paths.

## Acceptance criteria

- [x] `[permissions]` settings merge field-by-field across
      global/project/override: `profile` scalar override, `profiles` map
      per-name replace, base-only profiles preserved (fixture: settings
      merge unit tests, defaults template check).
- [x] No `[permissions]` section resolves to the built-in `default` profile;
      an unknown active profile name warns and falls back to `default`
      (fixture: hostd domain resolver unit tests).
- [x] With `[permissions] profile = "locked"` and no `[sandbox] policy-path`,
      the session sandbox policy carries the profile's read/write/deny/
      network fields and the inherited execution whitelist (fixture: orchd
      policy-resolution test).
- [x] The built-in `default` profile (absent section, explicit `profile =
      "default"` without a definition, or unknown-name fallback) never
      materializes: sandbox policy resolution is unchanged (fixture: hostd
      resolver tests + orchd policy-resolution test).
- [x] With a `[sandbox] policy-path` file present, the file wins for the
      sandbox policy while profile command rules still apply to approvals
      (fixture: orchd policy-resolution test).
- [x] A `bash`/`process start` call matching a `denied-commands` prefix
      returns `PermissionDenied { reason }`; the orchd registry maps it to a
      non-retryable `permission_denied` error, and no user prompt is
      published (fixture: hostd gateway test + orchd registry decision
      test).
- [x] A store-granted command that matches a `denied-commands` prefix still
      fails closed (deny wins over grants; fixture: hostd gateway test).
- [x] A `bash`/`process start` call matching an `allowed-commands` prefix
      is accepted one-shot: no user prompt, no store grant, and an identical
      second call is evaluated again (fixture: hostd gateway test).
- [x] Token-boundary prefix matching: `cargo test` matches
      `cargo test --release`, not `cargo testrun`; `git` matches `git status`,
      not `gitlab-ci` (fixture: hostd domain unit tests).
- [x] Non-matching commands and non-command tools keep the existing approval
      flow unchanged (fixture: hostd gateway test + existing registry
      tests).
- [x] With no `[permissions]` section, sandbox policy resolution and
      approval behavior are unchanged (fixture: existing tests green).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where do profiles live? | `[permissions]` in hostd settings layers | hostd owns settings; merge machinery already exists; keeps one source of truth |
| What does a profile materialize into? | Sandbox policy (file/network) + approval gateway (command) | Matches "materialized file/network/command policies" without a new execution layer |
| Do profile `allowed-commands` also change the sandbox execution whitelist? | No (slice 1) | The whitelist is execution enforcement with a different fail mode; conflating them would make routine commands fail at execution |
| Deny vs. prior grant ordering | Deny wins, checked before store grants | Fail-closed operator policy must not be overridable by a session grant |
| Unknown profile name | Warn + built-in `default` | Default equals today's behavior; failing closed on a typo would block startup for no safety gain |
| `[sandbox] policy-path` vs. profile | File wins | The file is the most explicit operator statement; profiles are the settings-native alternative |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Layered config (CLI > session > project > user > bundled) | kept (adapted) | piko already has global/project/override settings; profiles ride the same merge machinery. CLI/session layers stay out of scope |
| Permission-profile catalog | kept (adapted) | `[permissions].profiles` map with named bundles, selected by `profile` |
| Materialized file/network/command policies | kept (adapted) | file/network → sandbox `Policy`; command → approval gateway prefix rules |
| Command allow/deny prefix rules | kept (adapted) | token-boundary prefix matching on `bash`/`process start` commands |
| Managed-feature gating | out of this slice | landed separately in F-18/D-21/V-21 |
| Agent-role layers | out of this slice | permission-profile selection landed separately in F-19/D-22/V-22 |
| `request_permissions` tool elevation | rejected | no piko consumer; out-of-policy commands fail closed or prompt via F-07 |
| Network endpoint granularity | rejected | sandbox exposes a network boolean; profiles mirror the enforcement surface |

## Open questions

1. Whether a later slice should let profiles replace the sandbox execution
   whitelist (`allowedCommands`) for "locked-down" postures; deferred until a
   consumer asks for execution-level command restriction.

## Reference evidence

- codex-rs `core/src/config/*`: layered config, permission-profile catalog,
  managed features, agent roles, schema.
- codex-rs `core/src/exec/*`: command allow/deny prefix rules.
- piko `packages/hostd/src/domain/config/settings.rs` (merge machinery),
  `packages/hostd/src/adapters/turns/orch_runner/approval_gateway.rs`
  (gateway order), `packages/orchd/src/runtime/utils.rs`
  (`load_sandbox_policy`), `packages/sandbox/src/policy.rs` (Policy fields).
