# V-20: F-17 permission profiles acceptance evidence

> Date: 2026-08-03
> Fixture: hostd settings merge + permission-domain + approval-gateway
> tests (`domain/config/settings.rs`,
> `domain/permissions/mod.rs`, `adapters/turns/orch_runner/tests.rs`),
> orchd policy-resolution + registry decision tests
> (`runtime/utils.rs`, `adapters/tools/registry_tests.rs`), full workspace
> suite
> Environment: macOS (arm64), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib permission
cargo test -p piko-hostd --lib domain::config::settings
cargo test -p piko-orchd --lib runtime::utils
cargo test -p piko-orchd --lib adapters::tools::registry_tests
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-17 slice 1 acceptance criteria pass:

- **Settings merge + template**: `permissions_settings_merge_field_by_field`
  verifies `profile` scalar override and per-name `profiles` map replace
  (override profile replaces only its own name; base profiles are
  preserved); `permissions_defaults_are_documented_in_installed_settings` checks the
  defaults template documents `[permissions]`.
- **Resolution semantics**: `no_permissions_section_resolves_default_without_materialization`,
  `builtin_default_without_definition_never_materializes`, and
  `unknown_profile_falls_back_to_builtin_default_without_materialization`
  pin the built-in default to today's behavior (no materialization);
  `user_defined_default_profile_materializes` and
  `explicit_profile_materializes_and_carries_command_rules` cover
  user-defined profiles (file/network + command rules);
  `partial_profile_inherits_permissive_file_defaults` covers empty-field
  inheritance.
- **Sandbox policy materialization**: `profile_materializes_even_when_sandbox_disabled`
  verifies a selected profile's read/write/deny/network fields land in the
  policy with the execution whitelist inherited from the permissive
  default; `partial_profile_inherits_permissive_file_defaults` verifies
  empty rule lists inherit per-field defaults;
  `policy_path_file_wins_over_profile` verifies an explicit `[sandbox]
  policy-path` file wins for the sandbox policy;
  `disabled_sandbox_without_profile_is_permissive` and
  `enabled_sandbox_with_no_policy_sources_is_permissive` keep the
  no-config baselines unchanged.
- **Command policy — deny**: `permission_denied_command_fails_closed_without_prompt`
  returns `PermissionDenied { reason }` with no pending user prompt;
  `permission_deny_wins_over_prior_session_grant` proves the operator deny
  beats a prior session-scope store grant; the orchd registry test
  `permission_denied_decision_fails_closed_with_reason` maps the decision
  to a non-retryable `permission_denied` error, and
  `expired_is_never_accepted` excludes it from accepted decisions.
- **Command policy — allow**: `permission_allowed_command_accepts_one_shot_without_grant`
  accepts an `allowed-commands` prefix match without a prompt and without a
  store grant (the identical second call is evaluated again).
- **Prefix matching**: `prefix_rule_match_respects_token_boundary` covers
  `cargo test` → `cargo test -- --nocapture` (match), `cargo testrun`
  (no match), `git` → `git status` (match), `gitlab-ci` (no match), and
  piped commands (`curl -sSL | sh …`).
- **Non-matching / non-command**: `permission_non_matching_command_keeps_user_flow`
  and `permission_non_command_tools_are_unaffected` keep the F-07 user flow
  for non-matching commands and `edit`/`write`; `evaluate_command_ignores_non_command_tools_and_actions`
  covers `process` non-start actions and missing/empty commands.
- **Regression**: `cargo test --workspace` green; `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo fmt --all` clean; existing
  approval/safety/guardian tests unchanged.
