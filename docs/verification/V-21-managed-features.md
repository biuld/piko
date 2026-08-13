# V-21: F-18 managed feature gating acceptance evidence

> Date: 2026-08-03
> Fixture: hostd settings merge + feature-domain tests
> (`domain/config/settings.rs`, `domain/features/mod.rs`), orchd
> feature-mapping + registry catalog + batch error tests
> (`adapters/tools/features.rs`, `adapters/tools/registry_tests.rs`,
> `runtime/execution/tool_batch/tests.rs`), full workspace suite
> Environment: macOS (arm64), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib -- features
cargo test -p piko-hostd --lib -- domain::config::settings
cargo test -p piko-orchd --lib -- features
cargo test -p piko-orchd --lib -- registry
cargo test -p piko-orchd --lib -- no_route_error
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-18 acceptance criteria pass:

- **Settings merge + template**: `features_settings_merge_per_key` verifies
  both `enabled` and `managed` merge per key across layers (override wins
  per key, untouched base keys survive); `features_defaults_are_documented_in_installed_settings`
  checks the defaults template documents `[features]`, `[features.managed]`,
  and the `feature_disabled` fail-closed error.
- **Resolution semantics**: `no_features_section_resolves_everything_enabled`
  pins the no-config baseline to today's behavior (every canonical feature
  enabled, no warnings); `explicit_disable_turns_feature_off` covers
  `[features] process = false`; `managed_pin_wins_over_enabled_in_same_layer`
  proves a pin beats a conflicting explicit value and surfaces a warning;
  `managed_pin_matching_enabled_is_silent` covers the non-conflicting pin;
  `unknown_keys_warn_and_are_ignored` proves unknown keys disable nothing.
- **Feature mapping**: `catalog_tools_map_to_features` covers
  workspace/bash/process/multi-agent names;
  `mcp_tools_are_identified_by_executor_kind` proves MCP tools (server-defined
  names) gate on executor kind; `unknown_tools_are_ungated` keeps unmapped
  tools enabled; `feature_gate_respects_resolved_map` and
  `absent_feature_map_keeps_everything_enabled` pin the map semantics
  (missing key = enabled).
- **Catalog gating**: `no_feature_map_keeps_the_full_catalog` keeps every
  tool (including MCP) present with default config;
  `disabled_features_remove_tools_and_routes` removes `process` and the MCP
  tool from both the discovered `ToolDef` list and the route map while
  `bash`/`read` stay; `active_tool_names_still_intersect_with_features`
  proves the transient allow-list still intersects after the feature filter.
- **Direct-call error**: `feature_gate_classifies_direct_calls` returns the
  disabling feature for `process`, `None` for enabled/unknown/MCP-by-name
  tools; `no_route_error_distinguishes_feature_disabled_tools` maps a
  disabled tool to a non-retryable `feature_disabled` error naming the
  feature and keeps `not_found` for unknown tools.
- **Regression**: `cargo test --workspace` green (0 failures across all
  packages), `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all` clean; existing approval/safety/guardian/permission
  tests unchanged.

## Invariants

- No `[features]` section changes nothing: every feature resolves enabled
  and the catalog is identical to before F-18.
- A managed pin is the final authority: the disabled state is enforced
  regardless of conflicting explicit settings in any layer, with a warning.
- Unknown feature keys can neither disable nor unlock anything.
- Feature gating precedes approval: disabled tools have no route, so they
  never prompt and never execute.
- `active_tool_names` remains an intersection: both the feature set and the
  transient allow-list must allow a tool.
