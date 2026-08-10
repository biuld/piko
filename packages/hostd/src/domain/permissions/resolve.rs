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
                allow_escalation: profile.allow_escalation,
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
                    allow_escalation: profile.allow_escalation,
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
                    scratch_roots: profile.scratch_roots.clone(),
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

fn bundled_prompt_rule(command: &str) -> bool {
    let command = normalize_command(command);
    [
        "rm -rf",
        "rm -fr",
        "git reset --hard",
        "git clean -f",
        "sudo ",
        "chmod -R",
        "chown -R",
        "| sh",
        "| bash",
    ]
    .iter()
    .any(|pattern| command == pattern.trim() || command.contains(pattern))
}

/// Extract the shell program from an `exec_command` call.
fn tool_command(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "exec_command" => args
            .get("cmd")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        _ => None,
    }
}

pub fn validate_command_authority(tool_name: &str, args: &serde_json::Value) -> Result<(), String> {
    if tool_name != "exec_command" {
        return Ok(());
    }
    let authority = args
        .get("sandbox_permissions")
        .and_then(|value| value.as_str())
        .unwrap_or("use_default");
    if !matches!(
        authority,
        "use_default" | "with_additional_permissions" | "require_escalated"
    ) {
        return Err("invalid sandbox_permissions value".into());
    }
    if authority != "use_default"
        && args
            .get("justification")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("extra authority requires a non-empty justification".into());
    }
    if authority == "with_additional_permissions"
        && !args
            .get("additional_permissions")
            .is_some_and(serde_json::Value::is_object)
    {
        return Err("with_additional_permissions requires additional_permissions".into());
    }
    if let Some(rule) = args.get("prefix_rule") {
        // F-23 Rev B: approval-backed denial retries may carry a reusable
        // narrow prefix under constrained additional permissions as well as
        // under explicit elevation. Prefix rules never apply to default
        // sandbox calls.
        if !matches!(
            authority,
            "require_escalated" | "with_additional_permissions"
        ) {
            return Err(
                "prefix_rule is valid only for require_escalated or with_additional_permissions"
                    .into(),
            );
        }
        let tokens = rule
            .as_array()
            .ok_or_else(|| "prefix_rule must be an array of tokens".to_string())?;
        if tokens.len() < 2 {
            return Err("prefix_rule must contain a narrow program and subcommand".into());
        }
        let tokens: Vec<&str> = tokens
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|token| {
                        !token.is_empty()
                            && !token.chars().any(char::is_whitespace)
                            && !token.chars().any(|ch| ";|&<>`$()".contains(ch))
                    })
                    .ok_or_else(|| "prefix_rule entries must be simple argv tokens".to_string())
            })
            .collect::<Result<_, _>>()?;
        let command = args
            .get("cmd")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if command.chars().any(|ch| ";|&<>`$\n\r".contains(ch)) {
            return Err("prefix_rule cannot authorize a shell expression".into());
        }
        let prefix = tokens.join(" ");
        if !prefix_rule_match(&prefix, command) {
            return Err("prefix_rule does not match cmd".into());
        }
        if matches!(
            tokens[0],
            "sh" | "bash"
                | "zsh"
                | "fish"
                | "sudo"
                | "env"
                | "python"
                | "python3"
                | "node"
                | "ruby"
                | "perl"
                | "rm"
                | "curl"
                | "wget"
        ) {
            return Err("prefix_rule starts with a non-reusable command".into());
        }
    }
    Ok(())
}

/// Evaluate a tool call against the resolved command policy.
///
/// Returns `None` when the call must continue through guardian/user approval.
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
    let explicitly_allowed = config
        .allowed_command_prefixes
        .iter()
        .any(|rule| prefix_rule_match(rule, &command));
    let authority = args
        .get("sandbox_permissions")
        .and_then(|value| value.as_str())
        .unwrap_or("use_default");
    if authority == "require_escalated" && !config.allow_escalation {
        return Some(CommandDecision::Deny {
            prefix: "elevated execution".into(),
        });
    }
    if authority != "use_default" {
        return None;
    }
    if explicitly_allowed {
        return Some(CommandDecision::Allow);
    }
    if bundled_prompt_rule(&command) {
        return None;
    }
    // The default sandbox is the normal execution authority. It does not
    // require a user prompt merely because the shell program is complex or
    // absent from a static allowlist. Extra or elevated authority continues
    // through the approval flow.
    Some(CommandDecision::Allow)
}
