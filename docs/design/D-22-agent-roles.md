# D-22: Agent roles (per-role permission-profile selection)

> Status: accepted
> Implements: [F-19](../features/F-19-agent-roles.md)

## Goal

Let operators attach an F-17 permission profile to each agent role. A
`[permissions.roles]` map selects the profile for every agent instance
whose `AgentSpec.role` matches: the approval gateway evaluates commands with
the role's command policy and the workspace sandbox enforces the role's
file/network policy. Unmapped roles inherit the session profile, so no
`[permissions.roles]` section changes nothing.

## Constraints and non-goals

- hostd stays authoritative: settings ownership, role→profile resolution,
  and command-policy evaluation live in hostd. orchd only carries the
  executing agent's `role` (identity from the registered `AgentSpec`) and
  materializes role file/network policies into the sandbox provider.
- The role is identity, not policy: orchd copies `AgentSpec.role` from its
  registered spec into the execution context and the approval request; it
  never accepts a role from model-controlled arguments, and hostd is the
  only place a role maps to a profile.
- Role mappings can only select defined profiles; the built-in `default`
  mapping and unknown-profile mappings both mean "inherit the session
  profile" (fail-closed, never wider).
- Non-goals: per-role feature sets (F-18), role-shaped prompts/tool sets
  (already in `AgentSpec`), dynamic mid-session switching, changing
  approval tiers or the execution whitelist.

## Proposed design

### 1. Settings: `[permissions.roles]` (`piko-hostd`)

`PermissionsSettings` gains:

```rust
pub struct PermissionsSettings {
    pub profile: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, PermissionProfileSettings>,
    /// F-19: role → profile-name selection. Merged per key across layers;
    /// an override entry replaces the same-named base entry.
    #[serde(default)]
    pub roles: HashMap<String, String>,
}
```

TOML shape:

```toml
[permissions]
profile = "default"

[permissions.roles]
researcher = "readonly"
developer = "locked"
```

- Merge (`merge_permissions`): `roles` merges per key — override wins per
  key, base-only keys survive (same pattern as `profiles`).
- `installed_settings_fixture()` and `resources/settings.toml`
  document the section.
- `host_namespace_value()` already exposes the whole `permissions` section,
  so `roles` rides along in the `host` namespace.

### 2. Resolution: `piko-hostd/src/domain/permissions/mod.rs`

`ResolvedPermissions` gains role materialization:

```rust
/// Materialized file/network rules for one role (F-19).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleSandboxPolicy {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
}

pub struct ResolvedPermissions {
    pub materialize: bool,
    pub profile: PermissionProfile,
    pub config: PermissionConfig,
    /// Role → command policy for the approval gateway. Absent roles use
    /// `config`.
    pub role_configs: HashMap<String, PermissionConfig>,
    /// Role → materialized file/network policy for the sandbox. Absent
    /// roles use the session policy.
    pub role_policies: HashMap<String, RoleSandboxPolicy>,
}
```

Resolution algorithm (pure, unit-tested), applied after the F-17 session
profile resolution:

1. Start with empty maps.
2. For each `(role, profile_name)` in `settings.roles`:
   - `profile_name == DEFAULT_PROFILE_NAME` → skip (inherit session
     profile);
   - `settings.profiles.get(profile_name)` exists → resolve
     `PermissionProfile::from(defined)`; insert
     `role_configs[role]` = its `PermissionConfig` and
     `role_policies[role]` = its file/network rules;
   - otherwise → `tracing::warn!` naming the role and the unknown profile;
     drop the mapping (inherit session profile).

No `[permissions]` section → empty maps, unchanged behavior.

### 3. Protocol (`piko-protocol`, `piko-orchd-api`)

Identity transport:

- `piko-orchd-api::ToolApprovalRequest` gains
  `agent_role: Option<String>` (`agentRole`, skip when `None`).
- `piko_orchd_api::ToolExecutionContext` gains
  `agent_role: Option<String>` (`agentRole`, skip when `None`).

Sandbox policy transport:

- `piko_protocol::config::SandboxConfig` gains
  `role_policies: HashMap<String, PermissionPolicy>` (serde default, skip
  when empty). hostd fills it with the resolved role file/network policies;
  orchd materializes them per role.

### 4. orchd: role-aware execution (`piko-orchd`)

#### Execution identity and context

- `runtime/execution/mod.rs::ExecutionIdentity` gains
  `agent_role: Option<String>`.
- Construction at `prepare` (`mod.rs`) resolves the role from the
  registered spec: `self.services.agent_spec(&agent_id).await.map(|s| s.role)`.
  Unknown/unregistered agents → `None` (inherit session profile). The
  legacy test helper (`scope.rs`, fixtures) and the registry path copy the
  field through.
- `tool_exec_context` (tool_batch) copies `identity.agent_role` into
  `ToolExecutionContext`.

#### Provider interface

- `ToolProvider::writable_roots(&self)` becomes
  `writable_roots_for(&self, context: &ToolExecutionContext)` with the same
  default (`None`). Only `WorkspaceToolProvider` overrides it.

#### `WorkspaceToolProvider`

```rust
pub struct WorkspaceToolProvider {
    policy: Arc<Policy>,                       // session policy (fallback)
    role_policies: HashMap<String, Arc<Policy>>, // F-19 per-role policies
    shell: ShellSnapshot,
    env: EnvironmentProfile,
    processes: Arc<ProcessManager>,
    os_sandbox: bool,
}

impl WorkspaceToolProvider {
    fn policy_for(&self, role: Option<&str>) -> Arc<Policy> {
        role.and_then(|r| self.role_policies.get(r).cloned())
            .unwrap_or_else(|| Arc::clone(&self.policy))
    }
}
```

- A `with_role_policies(policies: HashMap<String, Policy>)` builder keeps
  the existing constructors untouched.
- `execute` resolves `let policy = self.policy_for(context.agent_role.as_deref());`
  and passes it to the shell/process/workspace handlers (the `environment`
  tool stays role-agnostic).
- `writable_roots_for(context)` projects
  `self.policy_for(role).writable_roots(&cwd)`.

#### Role policy materialization (`runtime/utils.rs`)

`load_role_sandbox_policies(sandbox: &SandboxConfig) -> HashMap<String, Policy>`
materializes each `role_policies` entry with the same rules as the F-17
session profile path: empty rule lists inherit the permissive defaults per
field, and the execution whitelist always comes from the permissive
default. A role without an entry keeps the session policy (no permissive
override possible). `bootstrap` builds these and passes them to the
workspace provider.

#### Approval request

`registry.rs` builds `ToolApprovalRequest` with
`agent_role: context.agent_role.clone()` and
`writable_roots: provider.writable_roots_for(&context)`.

### 5. hostd wiring (`orch_runner/mod.rs`, `approval_gateway.rs`)

- `OrchAgentRunRunner` stores `role_permission_configs: HashMap<String,
  PermissionConfig>` alongside the existing `permission_config`.
- `new_with_mcp` fills `sandbox.role_policies` from
  `resolved_permissions.role_policies` (mapping `RoleSandboxPolicy` →
  `piko_protocol::config::PermissionPolicy`).
- `approval_gateway.rs::request_tool_approval` selects the config:

```rust
let config = request
    .agent_role
    .as_deref()
    .and_then(|role| self.role_permission_configs.get(role))
    .unwrap_or(&self.permission_config);
match crate::domain::permissions::evaluate_command(
    &request.tool_name,
    &request.tool_args,
    config,
) { ... }
```

### 6. Tests

- hostd `domain/permissions/mod.rs`: role resolution (defined profile,
  `default` no-op, unknown-profile warn+drop, no section); `evaluate_command`
  against a role config.
- hostd `domain/config/settings.rs`: `roles` merges per key; template
  documents `[permissions.roles]`.
- hostd `orch_runner` tests: a role-mapped denied command fails closed with
  `permission_denied` while the same command from an unmapped role keeps the
  session flow; allowed prefixes one-shot accept for the mapped role.
- orchd `runtime/utils.rs`: role policy materialization (per-field
  inheritance, missing role → session policy).
- orchd `workspace_provider.rs`/handlers: `policy_for` role selection and
  `writable_roots_for` per role.
- orchd `registry_tests.rs` / tool_batch fixtures: the approval request and
  execution context carry `agent_role`.

## Open questions

None blocking. Per-role feature gating and per-role approval tiers remain
deferred (F-19 out of scope).
