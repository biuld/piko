# V-22: F-19 agent roles acceptance evidence

> Date: 2026-08-04
> Fixture: hostd settings merge + permission-domain tests
> (`domain/config/settings.rs`, `domain/permissions/mod.rs`), hostd approval
> gateway role tests (`adapters/turns/orch_runner/tests.rs`), orchd role
> policy materialization + provider + registry tests (`runtime/utils.rs`,
> `adapters/tools/workspace_provider.rs`, `adapters/tools/registry_tests.rs`),
> full workspace suite
> Environment: macOS (arm64), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib -- permissions
cargo test -p piko-hostd --lib -- domain::config::settings
cargo test -p piko-hostd --lib -- orch_runner::tests
cargo test -p piko-orchd --lib -- utils::tests
cargo test -p piko-orchd --lib -- workspace_provider
cargo test -p piko-orchd --lib -- registry
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-19 acceptance criteria pass:

- **Settings merge + template**: `permissions_settings_merge_field_by_field`
  verifies `[permissions.roles]` merges per key across layers (override
  replaces the same-named base entry, base-only keys survive, new keys are
  added); `permissions_defaults_are_documented_in_template` checks the
  defaults template documents `[permissions.roles]`.
- **Role resolution semantics**: `role_mapped_to_defined_profile_resolves_command_and_sandbox_policy`
  proves a defined-profile mapping resolves the role's `PermissionConfig`
  (allowed/denied prefixes) and its materialized file/network rules while
  the session profile stays untouched;
  `role_mapped_to_builtin_default_inherits_session_profile` covers the
  `default` no-op; `role_mapped_to_unknown_profile_is_dropped` proves a typo
  warns and drops the mapping (role inherits the session profile);
  `role_mappings_resolve_even_when_session_profile_is_default` pins that
  role layers resolve independently of the session profile choice;
  `no_roles_section_resolves_no_role_policies` keeps no-section behavior
  unchanged.
- **Approval gateway**: `permission_role_denied_command_fails_closed_for_mapped_role`
  shows a mapped role's `denied-commands` fail closed with
  `permission_denied` before any prompt while the same command from an
  unmapped role (or a request with no role) keeps the session user flow;
  `permission_role_allowed_command_accepts_one_shot_for_mapped_role` shows
  a mapped role's `allowed-commands` accept one-shot without a grant and
  that a role mapped to a different profile is not affected by another
  role's allow rules.
- **Sandbox role policies**: `role_policies_materialize_with_permissive_inheritance`
  and `roles_without_entries_keep_the_session_policy` cover orchd
  materialization (per-field permissive inheritance, whitelist inherited,
  missing roles keep the session policy);
  `policy_for_selects_role_policy_with_session_fallback` proves the
  workspace provider picks the role policy with session fallback for
  unmapped/unknown roles;
  `writable_roots_for_reflects_role_policy` proves F-12 safety evidence is
  projected from the executing role's policy.
- **Identity transport**: `approval_request_carries_executing_agent_role`
  proves the registry forwards the executing agent's role from the
  execution context to the `ToolApprovalRequest`; the execution runtime
  resolves `agent_role` from the registered `AgentSpec` at identity
  construction (hostd remains the only place a role maps to a profile).
- **Regression**: `cargo test --workspace` green across all packages; the
  only in-sandbox failures are `piko-llmd tests/gateway_retry` (they bind a
  local TCP listener, which the managed sandbox denies) — verified green
  unsandboxed (`cargo test -p piko-llmd --test gateway_retry`, 7/7 pass) and
  unrelated to F-19. `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo fmt --all` clean.

## Invariants

- No `[permissions]` / no `[permissions.roles]` changes behavior: every
  agent uses the session profile exactly as before F-19.
- Roles can never loosen below the session posture: a mapping to the
  built-in `default` or to an unknown profile is a no-op (inherit session
  profile), and role policies are only ever materialized from defined
  profiles.
- The role is identity, not policy: orchd copies `AgentSpec.role` from its
  registered spec; hostd owns profile resolution and command evaluation.
- Role policies apply per executing agent: workspace tools and approval
  `writable_roots` evidence both resolve through the executing role's
  policy, with the session policy as the fallback for unmapped roles.
- Per-role command policy runs before store grants, guardian review, and
  user prompts (deny fails closed, allow accepts one-shot), matching F-17
  precedence.
