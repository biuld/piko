# D-21: Managed feature gating

> Status: accepted
> Implements: [F-18](../features/F-18-managed-features.md)

## Goal

Deliver settings-declared tool-family gating with operator-pinned
constraints. A `[features]` section in the merged settings layers resolves
once per session start into a deterministic feature set; orchd filters its
tool catalog with it and fails direct calls to disabled tools closed with a
`feature_disabled` error. No `[features]` section changes nothing.

## Constraints and non-goals

- hostd stays authoritative: settings ownership, per-key merge, and pin
  resolution live in hostd; orchd only consumes a resolved DTO (the feature
  map) and applies it to the catalog.
- Gating is availability, not approval: a disabled tool never reaches the
  approval gateway, so it never prompts. Approval tiers, sandbox
  whitelists, and permission profiles are untouched.
- The feature catalog is fixed and small (nine stable keys). No dependency
  normalization, stability tiers, or legacy aliases in this slice.
- Non-goals: agent roles (F-19), dynamic mid-session switching,
  per-agent feature sets.

## Proposed design

### 1. Settings: `[features]` (`piko-hostd`)

`HostSettings` gains `features: Option<FeaturesSettings>`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct FeaturesSettings {
    /// Explicit per-key enablement; per-key override on merge.
    #[serde(default)]
    pub enabled: HashMap<String, bool>,
    /// Operator pins; final authority over `enabled` in every layer.
    #[serde(default)]
    pub managed: HashMap<String, bool>,
}
```

TOML shape:

```toml
[features]
process = false

[features.managed]
process = false
```

Merge follows the existing pattern (`merge_permissions`-style): when both
sides have a section, `enabled` and `managed` maps merge per key with the
override layer winning per key; when only one side has a section, that side
wins. The section is included in `host_namespace_value` so `ConfigGet`
exposes it like every other host setting.

### 2. Resolution: `piko-hostd/src/domain/features/mod.rs`

Canonical keys (single source of truth, also mirrored for validation):
`workspace`, `bash`, `process`, `environment`, `context`, `todo`,
`multi-agent`, `user-interaction`, `mcp`.

```rust
pub struct ResolvedFeatures {
    /// Resolved boolean per canonical key (every key present).
    pub enabled: HashMap<String, bool>,
    pub warnings: Vec<String>,
}

pub fn resolve_features(settings: Option<&FeaturesSettings>) -> ResolvedFeatures
```

Algorithm (pure, unit-tested):

1. Start with every canonical key enabled.
2. Apply merged `enabled` map: known keys set their value; unknown keys push
   a warning and are ignored.
3. Apply merged `managed` map: known keys force their value; if the merged
   `enabled` map explicitly set the same key to the opposite value, push a
   warning naming the feature and the expected (pinned) value; unknown keys
   warn and are ignored.

No section → `ResolvedFeatures { enabled: all true, warnings: [] }`.

### 3. Protocol: `OrchdConfig.features` (`piko-protocol`)

`OrchdConfig` gains:

```rust
/// Resolved managed-feature map (F-18): canonical feature key → bool.
/// Absent keys are treated as enabled by orchd, so legacy/default configs
/// are unchanged.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub features: Option<HashMap<String, bool>>,
```

hostd always sends the full resolved map (all canonical keys); `None` keeps
every existing test/constructor unchanged (all enabled).

### 4. Catalog gating: `piko-orchd/src/adapters/tools/registry.rs`

`ToolRegistryImpl` gains a feature set installed once at bootstrap:

```rust
pub async fn set_features(&self, features: Option<HashMap<String, bool>>)
pub async fn feature_gate(&self, tool_name: &str) -> Option<String>
```

- `feature_gate(name)` returns the feature key that disables `name`, or
  `None` when the tool is not gated (unknown tool or enabled feature).
- `discover_tools` applies the feature filter to both the `ToolDef` list and
  the route map, after the `active_tool_names` filter. A tool passes when
  its feature is absent from the map or mapped to `true`.

Feature mapping helper (pure, same module or `runtime/utils.rs`):

```rust
/// Canonical feature for a tool definition; MCP tools are identified by
/// executor kind because their names are server-defined.
pub fn feature_for_tool(tool: &ToolDef) -> Option<&'static str>
/// Canonical feature for a tool *name*; used by the direct-call error path.
pub fn feature_for_tool_name(name: &str) -> Option<&'static str>
```

Name map:

| Feature | Tool names |
|---|---|
| `workspace` | `read`, `edit`, `write` |
| `bash` | `bash` |
| `process` | `process` |
| `environment` | `environment` |
| `context` | `get_context_remaining`, `new_context_window` |
| `todo` | `todo_read`, `todo_write` |
| `multi-agent` | `spawn_agent`, `spawn_agent_detached`, `send_agent_message`, `collect_agent_reports`, `close_agent`, `reopen_agent`, `followup_task`, `interrupt_agent`, `list_agents`, `wait_agent` |
| `user-interaction` | `ask_user`, `request_user_input` |
| `mcp` | any `ToolDef` with `executor.kind == "mcp"` |

Unmapped tools are ungated (today's behavior).

### 5. Direct-call error: `feature_disabled`

The batch runner's missing-route path (parallel group and sequential path)
checks `registry.feature_gate(&tc.name)` before falling back to
`not_found`:

```rust
let record = ToolExecResult {
    ok: false,
    value: None,
    error: Some(ToolExecError {
        code: "feature_disabled".into(),
        message: format!(
            "tool '{}' is disabled by feature '{}'",
            tc.name, feature
        ),
        retryable: Some(false),
    }),
};
```

`execute_tool` needs no feature check: it only receives routes that passed
discovery.

### 6. Session bootstrap wiring (`piko-hostd`)

- `build_orch_turn_runner` passes `settings.features.as_ref()` into
  `OrchAgentRunRunner::new_with_mcp` (new parameter).
- `new_with_mcp` resolves features, sets
  `config.features = Some(resolved.enabled)`, and skips
  `initialize_mcp_tools` when the resolved `mcp` feature is off (no MCP
  server processes). The catalog filter remains the defense-in-depth gate.

### 7. Defaults template

`packages/hostd/resources/settings.toml` documents `[features]` and
`[features.managed]` with the canonical keys and defaults.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `OrchdConfig.features: Option<HashMap<String, bool>>` |
| `piko-hostd` | `FeaturesSettings` + per-key merge + host namespace; `domain/features` resolver; `orch_factory`/`new_with_mcp` wiring; MCP connection skip |
| `piko-orchd` | `set_features`/`feature_gate` on the registry; catalog filter; `feature_disabled` direct-call error; feature mapping helpers |
| `piko-sandbox` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- **Unknown keys**: warn at resolution, ignored; they can neither disable
  nor weaken anything.
- **Pin conflicts**: warn, pin wins; deterministic and fail-closed.
- **Cancellation**: feature resolution is synchronous at bootstrap, before
  any catalog build; the existing cancellation race owns tool outcomes
  unchanged.
- **Direct calls**: a disabled tool yields a non-retryable
  `feature_disabled` error recorded like any failed tool call; no route
  exists, so no provider executes and no approval is requested.

## Verification

- Unit tests (hostd): settings merge per key; resolution with no section /
  explicit disable / pin-wins-with-warning / unknown-key warnings.
- Unit tests (orchd): catalog filter (disabled tools absent from tools and
  routes), MCP executor-kind gating, `active_tool_names` intersection,
  `feature_gate` direct-call error, default config unchanged.
- Defaults template documentation check.
- Differential: F-18 acceptance criteria against codex-rs
  `managed_features` pin semantics (warn + pin wins adaptation).
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
