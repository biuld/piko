# F-19: Agent roles (per-role permission-profile selection)

> Status: implemented (F-19/D-22/V-22)
> Priority: P1
> Source evidence: codex-rs `config/*` (config layers, permission-profile
> catalog, agent roles), digest Block M (config & permissions), F-17
> permission profiles, F-10 multi-agent

## Summary

Operators can attach a permission profile to each agent role. Agent specs
already carry a stable `role` string (`root`, `generalist`, `developer`,
`researcher`, or a user-defined role); a `[permissions.roles]` settings map
selects the F-17 permission profile that applies to every agent instance
with that role. Tools executed by a mapped-role agent evaluate commands
against the role's command policy and run inside the role's file/network
sandbox policy; unmapped roles inherit the session profile exactly as
today. With no `[permissions.roles]` section, nothing changes.

## Problem

1. **One profile applies to every agent in the session.** Today the
   resolved `[permissions] profile` is a single session-wide policy. In
   multi-agent sessions (F-10), spawned children (`coder`, `scout`,
   `researcher`, …) inherit the root agent's exact file/network/command
   powers, so there is no settings-native way to say "researcher
   subagents are read-only" or "coder subagents can never `rm -rf`" while
   the root agent keeps the session profile.
2. **The role identity is decorative.** `AgentSpec.role` already
   distinguishes agent kinds and is carried through the agent tree, but no
   enforcement surface consumes it, so it cannot express policy.
3. **Constraint is all-or-nothing today.** The only ways to restrict a
   subagent are to tighten the whole session (affecting the root agent) or
   hand-maintain separate session configs. Neither scales to role-shaped
   delegation, and both weaken the fail-closed posture by forcing a single
   least-privilege denominator.

## User journeys

1. A project defines `readonly` and `locked` profiles and maps
   `[permissions.roles] researcher = "readonly"`. A spawned researcher
   executes `bash`/`process start`/`edit`/`write` under `readonly`:
   denied commands fail closed with `permission_denied`, allowed commands
   auto-accept one-shot, file roots restrict access, and network stays off.
   The root agent (role `root`, unmapped) keeps the session profile.
2. `[permissions.roles] developer = "locked"` where `locked` declares
   `denied-commands = ["rm -rf"]`. A coder child calling `rm -rf foo` fails
   closed before any grant, guardian review, or user prompt, while the same
   command from the root agent follows the normal session flow.
3. An operator typo's a role mapping (`roles.scout = "readonly-typo"`).
   hostd warns naming the role and the unknown profile, drops the mapping,
   and the scout inherits the session profile — a typo can never widen a
   role's powers.
4. A role is explicitly mapped to the built-in `default`
   (`roles.scout = "default"`). This is a no-op: the role inherits the
   session profile. Role mappings can only ever select a defined profile;
   they cannot loosen below the session posture.
5. A user with no `[permissions]` or `[permissions.roles]` section runs
   piko. Behavior is unchanged: every agent uses the permissive session
   default, exactly as before F-19.

## In scope

- `[permissions.roles]`: a role → profile-name map inside `PermissionsSettings`,
  merged per key across global/project/override (override wins per key,
  base-only keys survive).
- Role resolution at session start (hostd owns settings): each mapping is
  validated against the defined profile catalog:
  - mapping to the built-in `default` → no per-role entry (inherit session
    profile);
  - mapping to an unknown profile → warn naming the role and the profile,
    drop the mapping (inherit session profile, fail-closed);
  - mapping to a defined profile → resolve that profile's command policy
    and file/network policy for the role.
- Command policy: the approval gateway evaluates `bash` and
  `process start` commands with the executing agent's role config when the
  role is mapped (allowed prefixes one-shot accept, denied prefixes fail
  closed with `permission_denied`); unmapped or unknown roles use the
  session config.
- File/network policy: the sandbox policy for workspace tools
  (`read`/`edit`/`write`/`bash`/`process`) is selected per executing
  agent role from the role's materialized profile; unmapped roles use the
  session policy. Approval-time `writable_roots` evidence (F-12) reflects
  the executing role's policy.
- Role identity transport: orchd reports the executing agent's `role` from
  the registered `AgentSpec` on the tool approval request and execution
  context. Profile resolution and all policy enforcement stay in hostd /
  the sandbox policy materialization (hostd authoritative); the role is
  identity metadata, never policy input from the model.
- `resources/settings.toml` documents the section.

## Out of scope

- Per-agent/per-role feature gating (F-18 per-role feature sets) — deferred
  until a consumer asks for it.
- Role-shaped prompts, tool sets, models, or thinking levels — already
  expressed by `AgentSpec` (`tool_set_ids`, `active_tool_names`,
  `instructions`, `model`, `thinking_level`); F-19 only selects permission
  profiles.
- Dynamic mid-session role or profile switching; roles resolve once per
  session start from the merged settings and registered agent specs.
- Changing tool approval tiers (`never` / `on-request` / `always`) or the
  sandbox execution whitelist from role profiles; profiles keep their F-17
  semantics (command prefix rules + file/network policy).
- Roles loosening below the session profile: the built-in `default`
  mapping is a no-op, so a locked session profile cannot be widened per
  role.

## Behavior and states

### Settings

```toml
[permissions]
profile = "default"

[permissions.roles]
researcher = "readonly"
developer = "locked"
```

`roles` merges per key across layers: an override entry replaces the
same-named base entry; base-only entries are kept (same pattern as
`profiles`).

### Resolution

```text
session profile        = resolve `[permissions] profile` (F-17, unchanged)
role_profiles[role]    = merged `roles[role]`, validated:
                           "default"           → no entry (inherit session)
                           defined profile     → entry (role policy)
                           unknown profile     → warn + drop (inherit session)
unmapped role          → session profile (unchanged behavior)
```

`role_profiles` is resolved once per session start from the merged settings
and the registered agent catalog's role strings; unknown role names that
appear only in `roles` produce entries only when the profile is defined
(a role mapping is applied to any agent whose spec carries that role,
whether built-in or user-defined).

### Enforcement

- Approval gateway: `config_for_role(role)` = `role_configs[role]` when
  mapped, else the session `PermissionConfig`. `evaluate_command` runs
  against that config; non-command tools are unaffected.
- Sandbox: `policy_for_role(role)` = materialized role policy when mapped,
  else the session policy. `writable_roots_for(context)` projects the role
  policy's writable roots for F-12 safety assessment.
- Unmapped role, unknown role (spec role with no mapping), or missing role
  on the request: session policy and config, exactly as today.

### Failure modes

- Unknown profile in a role mapping: warn + drop mapping; role inherits the
  session profile. Nothing widens.
- Unknown role string on a request: inherits the session profile.
- No `[permissions]` section: no role mappings can exist; all agents use
  the permissive session default.

## Acceptance criteria

- [x] `permissions_settings_merge_field_by_field`-style coverage proves
      `[permissions.roles]` merges per key across layers (override wins per
      key; base-only keys survive).
- [x] A role mapped to a defined profile resolves that profile's
      `PermissionConfig`; `evaluate_command` with that config denies
      `denied-commands` (fail closed, `permission_denied`) and one-shot
      accepts `allowed-commands` for `bash` and `process start`.
- [x] Unmapped roles and unknown roles evaluate against the session config
      (existing F-17 behavior unchanged).
- [x] A mapping to an unknown profile logs a warning naming the role and
      the profile and is dropped (role inherits the session profile).
- [x] A mapping to the built-in `default` is a no-op (role inherits the
      session profile); roles can never loosen below the session profile.
- [x] Workspace tools executed by a mapped-role agent run under the role's
      materialized file/network policy (read/write roots, deny paths,
      network allow); unmapped roles keep the session policy.
- [x] Approval-time `writable_roots` reflects the executing role's policy
      (read-only role → no auto-approve for out-of-roots writes).
- [x] The approval request carries the executing agent's `role` from the
      registered `AgentSpec` (identity, not model input); hostd resolves
      the profile.
- [x] No `[permissions]` / no `[permissions.roles]` changes behavior;
      `resources/settings.toml` documents the section.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What is a role? | The existing `AgentSpec.role` string | Roles already identify agent kinds across the tree; F-19 gives them policy meaning without a new identity system |
| Role mapping to built-in `default` | No-op: inherit the session profile | Preserves "`default` never materializes"; a role layer must not loosen below the session posture (fail-closed) |
| Unknown profile in a mapping | Warn + drop mapping | A typo must fail closed toward the session profile, never widen a role |
| Where does the role come from at approval time? | orchd reports the spec role on the request; hostd resolves the profile | hostd stays authoritative for settings and policy; the role is identity, not model-controllable policy |
| Role policy vs `[sandbox] policy-path` | Role profiles materialize on top of the resolved session policy; a mapped role's profile wins for that role | An explicit per-role override is the strongest statement for that role; the session policy stays the fallback |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Agent-role config layers (roles select policy) | kept (adapted) | roles are config-layer selectors: `[permissions.roles]` maps `AgentSpec.role` → F-17 profile |
| Role layer over session config | kept (adapted) | unmapped roles inherit the session profile; mapped roles override with a defined profile; `default` mapping is a no-op |
| Role-shaped prompt/tool/model layers | rejected (deferred) | `AgentSpec` already carries tool sets, model, instructions, thinking level; no new consumer in this slice |
| Per-role managed-feature sets | rejected (deferred) | F-18 gating stays session-wide until a consumer asks for per-role gating |

## Open questions

1. Whether a later slice should let roles select per-role feature sets
   (F-18) or approval tiers; deferred until a consumer exists.

## Reference evidence

- codex-rs `core/src/config/*` (config layers, permission-profile catalog,
  agent roles), `core/src/config/environment_selection.rs`.
- piko F-17 (`docs/features/F-17-permission-profiles.md`), F-10
  (`docs/features/F-10-multi-agent.md`), F-18
  (`docs/features/F-18-managed-features.md`).
- Digest Block M (Config & Permissions) and roadmap M3 M-config row.
