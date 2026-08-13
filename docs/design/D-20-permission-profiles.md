# D-20: Permission profiles (materialized file/network/command policies)

> Status: accepted
> Implements: [F-17](../features/F-17-permission-profiles.md) (slice 1)

## Goal

Let operators declare per-project safety posture as settings, not as a
hand-authored policy JSON. A named **permission profile** resolves from the
merged settings layers at session start and materializes into two existing
enforcement surfaces:

1. the sandbox `Policy` used by workspace/exec tools (read/write roots,
   deny paths, network allow);
2. the approval gateway's command policy (allowed prefixes execute
   one-shot without a prompt; denied prefixes fail closed deterministically).

## Constraints and non-goals

- hostd stays authoritative: settings ownership, profile resolution, and
  command evaluation live in hostd; orchd only consumes a DTO (profile →
  sandbox policy) and maps the new decision to a tool error.
- The execution whitelist (`Policy::allowed_commands`) is untouched by
  profiles in this slice — it is inherited from the permissive default when
  a profile is materialized (the whitelist has a different fail mode:
  execution-time hard deny).
- Operator deny wins over prior store grants (checked before the store).
- Profile `allowed-commands` matches accept one-shot; no store grant is
  written (mirrors F-11/F-12 one-shot semantics).
- Non-goals: managed-feature gating, agent roles, `request_permissions`
  elevation, network endpoint granularity, dynamic mid-session profile
  switching, changing tool approval tiers.

## Proposed design

### 1. Settings: `[permissions]`

`HostSettings` gains `permissions: Option<PermissionsSettings>`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PermissionsSettings {
    /// Active profile name; defaults to the built-in "default".
    pub profile: Option<String>,
    /// Named profile definitions; per-name replace on merge.
    #[serde(default)]
    pub profiles: HashMap<String, PermissionProfileSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PermissionProfileSettings {
    #[serde(default)]
    pub read_roots: Vec<String>,
    #[serde(default)]
    pub write_roots: Vec<String>,
    #[serde(default)]
    pub deny_paths: Vec<String>,
    /// Network allow for sandboxed execution. Default: false.
    #[serde(default)]
    pub allow_network: bool,
    /// Command prefix rules that auto-accept on-request approvals one-shot.
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Command prefix rules that fail closed with `permission_denied`.
    #[serde(default)]
    pub denied_commands: Vec<String>,
}
```

- Merge: `profile` is a scalar override; `profiles` merges per name — an
  override entry replaces the same-named base entry, base-only entries are
  kept (mirrors how override maps behave elsewhere in piko).
- `installed_settings_fixture()` and `resources/settings.toml` gain a
  documented `[permissions]` section with a commented example profile.
- `host_namespace_value()` exposes `permissions` in the `host` namespace.

### 2. hostd domain: `domain/permissions/mod.rs`

Pure logic with unit tests:

```rust
/// Built-in profile: mirrors the permissive sandbox default.
pub const DEFAULT_PROFILE_NAME: &str = "default";

pub struct PermissionProfile {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
    pub allowed_commands: Vec<String>,
    pub denied_commands: Vec<String>,
}

/// Resolved command policy for the approval gateway.
#[derive(Debug, Clone, Default)]
pub struct PermissionConfig {
    pub allowed_command_prefixes: Vec<String>,
    pub denied_command_prefixes: Vec<String>,
}

pub struct ResolvedPermissions {
    /// True when `[permissions] profile` was explicitly configured, so the
    /// profile's file/network rules materialize into the sandbox policy.
    pub materialize: bool,
    pub profile: PermissionProfile,
    pub config: PermissionConfig,
}

pub fn resolve_permissions(settings: Option<&PermissionsSettings>) -> ResolvedPermissions;
```

- `resolve_permissions`: `settings.is_none()` → built-in `default`,
  `materialize = false`. Explicit `profile` → merged `profiles[name]`;
  unknown names warn and fall back to the built-in `default`. Only
  user-defined profiles set `materialize = true` — the built-in `default`
  is identical to configuring nothing and never materializes, so it cannot
  shadow `.piko/sandbox.json`.
- `PermissionProfile::from(settings)`: empty rule vectors inherit the
  permissive defaults per field (read `["."]`, write `["."]`, deny
  `[".git", ".piko"]`), so a profile that only declares command rules does
  not lock down file/network access unexpectedly.
- `prefix_rule_match(rule, command)`: whitespace-normalize both sides; match
  when `command == rule || command.starts_with(rule + " ")` (token
  boundary).
- `evaluate_command(tool_name, args, config) -> Option<CommandDecision>`:
  - extract `command` for `bash` and for `process` with
    `action == "start"`; missing/non-string → `None` (unchanged flow);
  - any denied prefix match → `CommandDecision::Deny { prefix }`;
  - any allowed prefix match → `CommandDecision::Allow`;
  - else `None`.
- `domain/mod.rs` exports the module.

### 3. protocol: materialized profile DTO

`piko-protocol` stays the shared DTO leaf (no dependency on `piko-sandbox`):

```rust
// packages/protocol/src/config.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPolicy {
    #[serde(default)]
    pub read_roots: Vec<String>,
    #[serde(default)]
    pub write_roots: Vec<String>,
    #[serde(default)]
    pub deny_paths: Vec<String>,
    #[serde(default)]
    pub allow_network: bool,
}

// SandboxConfig gains:
#[serde(skip_serializing_if = "Option::is_none")]
pub policy_profile: Option<PermissionPolicy>,
```

### 4. `piko-orchd-api`: new decision

`ToolApprovalDecision` gains:

```rust
/// The permission-profile command policy rejected the request (operator
/// denied the command prefix). Fails closed, non-retryable.
PermissionDenied { reason: String },
```

`is_approval_accepted` excludes it (fail closed).

### 5. `piko-orchd`: policy resolution + decision mapping

`load_sandbox_policy(sandbox: &SandboxConfig)` precedence becomes:

```text
!sandbox.enabled && no policy_profile ──────────> permissive
sandbox.enabled && policy_path file exists ─────> file
policy_profile present ─────────────────────────> materialized:
    Policy { version: 1,
             read: profile.read_roots,
             write: profile.write_roots,
             deny: profile.deny_paths,
             allowed_commands: permissive_default().allowed_commands,
             allow_network: profile.allow_network }
sandbox.enabled && .piko/sandbox.json exists ───> file
otherwise ──────────────────────────────────────> permissive
```

Empty rule lists in the profile inherit the permissive defaults per field,
and the whitelist inheritance happens in orchd, where the permissive list
already lives — no duplication in hostd. Profiles apply even when
`[sandbox] enabled = false` (they refine the authorization policy used by
tool handlers); `[sandbox] policy-path` and `.piko/sandbox.json` are
OS-sandbox policy sources and only apply when enabled.

`registry.rs` decision mapping adds:

| Decision | Tool error |
|---|---|
| `PermissionDenied { reason }` | `permission_denied` — "Command denied by permission policy: {reason}" (non-retryable) |

### 6. hostd runner + gateway

`OrchAgentRunRunner::new_with_mcp` gains
`permissions_settings: Option<&PermissionsSettings>`:

- resolves `ResolvedPermissions`;
- stores `permission_config: PermissionConfig` on the runner;
- when `resolved.materialize`, sets `SandboxConfig.policy_profile` from the
  profile's file/network fields (shell path and enabled preserved).

`request_tool_approval` gains the F-17 gate at the top, before the store
check:

```rust
// F-17 permission profiles: operator command policy is the strongest gate.
// Denied prefixes fail closed before grants/prompts; allowed prefixes
// auto-accept one-shot (no store grant).
match crate::domain::permissions::evaluate_command(
    &request.tool_name,
    &request.tool_args,
    &self.permission_config,
) {
    Some(CommandDecision::Deny { prefix }) => {
        return ToolApprovalDecision::PermissionDenied {
            reason: format!("command prefix '{}' is denied by permission policy", prefix),
        };
    }
    Some(CommandDecision::Allow) => {
        return ToolApprovalDecision::Accept; // one-shot, no grant
    }
    None => {}
}
```

Existing order (store → F-12 safety → F-11 guardian → user) is unchanged
for everything else.

### 7. Tests

- hostd `settings.rs`: merge field-by-field + template check.
- hostd `domain/permissions/mod.rs`: default resolution, unknown-name
  fallback, materialize flag, prefix matching boundaries, command
  extraction (bash / process start / non-command).
- hostd gateway tests: deny-before-store, one-shot allow without grant,
  non-matching falls through to user flow, non-command tools unaffected.
- orchd `utils.rs`: policy precedence (profile materialization vs.
  policy_path vs. default file), whitelist inheritance.
- orchd `registry_tests.rs`: `PermissionDenied` → `permission_denied`,
  non-retryable; `is_approval_accepted` excludes it.

## Migration and compatibility

- No `[permissions]` section: no behavior change (existing tests stay
  green).
- Existing `[sandbox] policy-path` users: file still wins for the sandbox
  policy; command rules only apply when profiles are configured.
- No settings-file format change: new optional section only.
