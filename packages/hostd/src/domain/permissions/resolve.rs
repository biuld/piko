use super::*;

pub fn resolve_permissions(settings: Option<&PermissionsSettings>) -> ResolvedPermissions {
    let Some(settings) = settings else {
        return ResolvedPermissions {
            materialize: false,
            profile: PermissionProfile::default(),
            config: PermissionConfig::default(),
            role_configs: HashMap::new(),
            role_policies: HashMap::new(),
        };
    };

    // F-19: role mappings resolve independently of the session profile
    // selection, whenever a `[permissions]` section exists.
    let role_configs = resolve_role_configs(settings);
    let role_policies = resolve_role_policies(settings);

    let name = settings.profile.as_deref().unwrap_or(DEFAULT_PROFILE_NAME);
    match settings.profiles.get(name) {
        Some(defined) => {
            let profile = PermissionProfile::from(defined);
            let config = PermissionConfig {
                allowed_command_prefixes: profile.allowed_commands.clone(),
                denied_command_prefixes: profile.denied_commands.clone(),
            };
            ResolvedPermissions {
                materialize: true,
                profile,
                config,
                role_configs,
                role_policies,
            }
        }
        None if name != DEFAULT_PROFILE_NAME => {
            tracing::warn!(
                profile = name,
                "unknown permission profile; falling back to built-in 'default'"
            );
            ResolvedPermissions {
                materialize: false,
                profile: PermissionProfile::default(),
                config: PermissionConfig::default(),
                role_configs,
                role_policies,
            }
        }
        _ => ResolvedPermissions {
            materialize: false,
            profile: PermissionProfile::default(),
            config: PermissionConfig::default(),
            role_configs,
            role_policies,
        },
    }
}

/// Resolve role → command policy for the approval gateway (F-19).
///
/// Each `[permissions.roles]` entry is validated against the defined
/// profile catalog: a mapping to the built-in `default` or to an unknown
/// profile is dropped (the role inherits the session profile), and a
/// mapping to a defined profile resolves that profile's command rules.
fn resolve_role_configs(settings: &PermissionsSettings) -> HashMap<String, PermissionConfig> {
    let mut configs = HashMap::new();
    for (role, profile_name) in &settings.roles {
        if let Some(profile) = resolve_role_profile(settings, role, profile_name) {
            configs.insert(
                role.clone(),
                PermissionConfig {
                    allowed_command_prefixes: profile.allowed_commands.clone(),
                    denied_command_prefixes: profile.denied_commands.clone(),
                },
            );
        }
    }
    configs
}

/// Resolve role → materialized file/network policy for the sandbox (F-19).
///
/// Same validation as [`resolve_role_configs`]; a mapped role's defined
/// profile supplies the file/network rules materialized into the sandbox.
fn resolve_role_policies(settings: &PermissionsSettings) -> HashMap<String, RoleSandboxPolicy> {
    let mut policies = HashMap::new();
    for (role, profile_name) in &settings.roles {
        if let Some(profile) = resolve_role_profile(settings, role, profile_name) {
            policies.insert(
                role.clone(),
                RoleSandboxPolicy {
                    read_roots: profile.read_roots.clone(),
                    write_roots: profile.write_roots.clone(),
                    deny_paths: profile.deny_paths.clone(),
                    allow_network: profile.allow_network,
                },
            );
        }
    }
    policies
}

/// Resolve one role mapping to a `PermissionProfile`, or `None` when the
/// mapping must be dropped (built-in `default` or unknown profile). Unknown
/// profile names warn so an operator typo is visible while failing closed
/// toward the session profile.
fn resolve_role_profile(
    settings: &PermissionsSettings,
    role: &str,
    profile_name: &str,
) -> Option<PermissionProfile> {
    if profile_name == DEFAULT_PROFILE_NAME {
        return None;
    }
    match settings.profiles.get(profile_name) {
        Some(defined) => Some(PermissionProfile::from(defined)),
        None => {
            tracing::warn!(
                role = role,
                profile = profile_name,
                "unknown permission profile in [permissions.roles]; role inherits the session profile"
            );
            None
        }
    }
}

/// Outcome of evaluating a command against the resolved command policy.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandDecision {
    /// Matches an `allowed-commands` prefix: accept one-shot.
    Allow,
    /// Matches a `denied-commands` prefix: fail closed.
    Deny { prefix: String },
}

/// Token-boundary prefix match: `rule` matches `command` when the
/// whitespace-normalized command equals the rule or starts with the rule
/// followed by a space.
pub fn prefix_rule_match(rule: &str, command: &str) -> bool {
    let rule = normalize_command(rule);
    let command = normalize_command(command);
    if rule.is_empty() || command.is_empty() {
        return false;
    }
    command == rule || command.starts_with(&format!("{rule} "))
}

fn normalize_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the shell command from a tool call, if the tool is a command
/// tool (`bash`, or `process` with `action == "start"`).
fn tool_command(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "bash" => args
            .get("command")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        "process" => {
            let action = args
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if action == "start" {
                args.get("command")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Evaluate a tool call against the resolved command policy.
///
/// Returns `None` for non-command tools, missing/non-string commands, empty
/// commands, and commands matching neither prefix list — the existing flow
/// is unchanged in all those cases.
pub fn evaluate_command(
    tool_name: &str,
    args: &serde_json::Value,
    config: &PermissionConfig,
) -> Option<CommandDecision> {
    let command = tool_command(tool_name, args)?;
    if command.trim().is_empty() {
        return None;
    }
    for rule in &config.denied_command_prefixes {
        if prefix_rule_match(rule, &command) {
            return Some(CommandDecision::Deny {
                prefix: rule.clone(),
            });
        }
    }
    for rule in &config.allowed_command_prefixes {
        if prefix_rule_match(rule, &command) {
            return Some(CommandDecision::Allow);
        }
    }
    None
}
