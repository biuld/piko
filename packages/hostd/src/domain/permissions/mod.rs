//! F-17 permission profiles: profile resolution and command-policy
//! evaluation.
//!
//! Profiles bundle file/network policy (materialized into the sandbox
//! policy by the orchd wiring) and command policy (materialized into the
//! approval gateway: allowed prefixes auto-accept one-shot, denied prefixes
//! fail closed). The logic here is pure and unit-tested; all settings come
//! from the already-merged `[permissions]` section.

use std::collections::HashMap;

use crate::domain::config::{PermissionProfileSettings, PermissionsSettings};

/// Built-in profile name; mirrors the permissive sandbox default.
pub const DEFAULT_PROFILE_NAME: &str = "default";

/// A resolved permission profile: file/network rules for the sandbox policy
/// and command rules for the approval gateway.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionProfile {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
    /// Command prefixes that auto-accept on-request approvals one-shot.
    pub allowed_commands: Vec<String>,
    /// Command prefixes that fail closed with `permission_denied`.
    pub denied_commands: Vec<String>,
}

impl Default for PermissionProfile {
    fn default() -> Self {
        Self {
            read_roots: vec![".".into()],
            write_roots: vec![".".into()],
            deny_paths: vec![".git".into(), ".piko".into()],
            allow_network: false,
            allowed_commands: Vec::new(),
            denied_commands: Vec::new(),
        }
    }
}

impl From<&PermissionProfileSettings> for PermissionProfile {
    fn from(settings: &PermissionProfileSettings) -> Self {
        // Empty vectors inherit the permissive defaults per field, so a
        // profile that only declares command rules does not lock down file
        // or network access unexpectedly.
        let base = PermissionProfile::default();
        Self {
            read_roots: if settings.read_roots.is_empty() {
                base.read_roots
            } else {
                settings.read_roots.clone()
            },
            write_roots: if settings.write_roots.is_empty() {
                base.write_roots
            } else {
                settings.write_roots.clone()
            },
            deny_paths: if settings.deny_paths.is_empty() {
                base.deny_paths
            } else {
                settings.deny_paths.clone()
            },
            allow_network: settings.allow_network,
            allowed_commands: settings.allowed_commands.clone(),
            denied_commands: settings.denied_commands.clone(),
        }
    }
}

/// Resolved command policy handed to the approval gateway.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PermissionConfig {
    pub allowed_command_prefixes: Vec<String>,
    pub denied_command_prefixes: Vec<String>,
}

/// Materialized file/network rules for one agent role (F-19). Mirrors the
/// sandbox-facing fields of a permission profile; empty vectors inherit the
/// permissive defaults per field when materialized.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleSandboxPolicy {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub deny_paths: Vec<String>,
    pub allow_network: bool,
}

/// Fully resolved permissions for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPermissions {
    /// Whether a `[permissions]` section exists, so the profile's
    /// file/network rules materialize into the sandbox policy. Without a
    /// section, sandbox policy resolution is unchanged.
    pub materialize: bool,
    pub profile: PermissionProfile,
    pub config: PermissionConfig,
    /// F-19: role → command policy for the approval gateway. Absent roles
    /// use `config` (the session profile).
    pub role_configs: HashMap<String, PermissionConfig>,
    /// F-19: role → materialized file/network policy for the sandbox.
    /// Absent roles use the session policy.
    pub role_policies: HashMap<String, RoleSandboxPolicy>,
}

/// Resolve the active permission profile from merged settings.
///
/// - No `[permissions]` section: built-in `default`, no materialization.
/// - Built-in `default` (selected explicitly or by default): same as no
///   section — never materializes, so it cannot shadow `.piko/sandbox.json`.
/// - Unknown profile name: warn and fall back to the built-in `default`.
/// - A user-defined profile: materializes file/network policy into the
///   sandbox policy and command rules into the approval gateway.
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::config::{PermissionProfileSettings, PermissionsSettings};

    fn settings_with(
        profile: Option<&str>,
        profiles: &[(&str, PermissionProfileSettings)],
    ) -> PermissionsSettings {
        PermissionsSettings {
            profile: profile.map(str::to_string),
            profiles: profiles
                .iter()
                .map(|(name, profile)| ((*name).to_string(), profile.clone()))
                .collect(),
            roles: HashMap::new(),
        }
    }

    fn settings_with_roles(
        profile: Option<&str>,
        profiles: &[(&str, PermissionProfileSettings)],
        roles: &[(&str, &str)],
    ) -> PermissionsSettings {
        PermissionsSettings {
            profile: profile.map(str::to_string),
            profiles: profiles
                .iter()
                .map(|(name, profile)| ((*name).to_string(), profile.clone()))
                .collect(),
            roles: roles
                .iter()
                .map(|(role, profile)| ((*role).to_string(), (*profile).to_string()))
                .collect(),
        }
    }

    fn command_profile() -> PermissionProfileSettings {
        PermissionProfileSettings {
            allowed_commands: vec!["cargo test".into(), "git status".into()],
            denied_commands: vec!["rm -rf".into(), "curl -sSL | sh".into()],
            ..Default::default()
        }
    }

    #[test]
    fn no_permissions_section_resolves_default_without_materialization() {
        let resolved = resolve_permissions(None);
        assert!(!resolved.materialize);
        assert_eq!(resolved.profile, PermissionProfile::default());
        assert_eq!(resolved.config, PermissionConfig::default());
    }

    #[test]
    fn explicit_profile_materializes_and_carries_command_rules() {
        let settings = settings_with(Some("locked"), &[("locked", command_profile())]);
        let resolved = resolve_permissions(Some(&settings));
        assert!(resolved.materialize);
        assert_eq!(
            resolved.profile.allowed_commands,
            vec!["cargo test", "git status"]
        );
        assert_eq!(
            resolved.profile.denied_commands,
            vec!["rm -rf", "curl -sSL | sh"]
        );
        assert_eq!(
            resolved.config.allowed_command_prefixes,
            vec!["cargo test", "git status"]
        );
        assert_eq!(
            resolved.config.denied_command_prefixes,
            vec!["rm -rf", "curl -sSL | sh"]
        );
    }

    #[test]
    fn partial_profile_inherits_permissive_file_defaults() {
        let settings = settings_with(
            Some("commands-only"),
            &[(
                "commands-only",
                PermissionProfileSettings {
                    denied_commands: vec!["rm -rf".into()],
                    ..Default::default()
                },
            )],
        );
        let resolved = resolve_permissions(Some(&settings));
        assert_eq!(
            resolved.profile,
            PermissionProfile {
                denied_commands: vec!["rm -rf".into()],
                ..PermissionProfile::default()
            }
        );
    }

    #[test]
    fn unknown_profile_falls_back_to_builtin_default_without_materialization() {
        let settings = settings_with(Some("typo"), &[]);
        let resolved = resolve_permissions(Some(&settings));
        assert!(!resolved.materialize);
        assert_eq!(resolved.profile, PermissionProfile::default());
    }

    #[test]
    fn builtin_default_without_definition_never_materializes() {
        let settings = settings_with(Some("default"), &[]);
        let resolved = resolve_permissions(Some(&settings));
        assert!(!resolved.materialize);
        assert_eq!(resolved.profile, PermissionProfile::default());
    }

    #[test]
    fn user_defined_default_profile_materializes() {
        let settings = settings_with(
            Some("default"),
            &[(
                "default",
                PermissionProfileSettings {
                    allow_network: true,
                    ..Default::default()
                },
            )],
        );
        let resolved = resolve_permissions(Some(&settings));
        assert!(resolved.materialize);
        assert!(resolved.profile.allow_network);
    }

    #[test]
    fn role_mapped_to_defined_profile_resolves_command_and_sandbox_policy() {
        let settings = settings_with_roles(
            Some("locked"),
            &[
                ("locked", command_profile()),
                (
                    "readonly",
                    PermissionProfileSettings {
                        write_roots: vec!["/work".into()],
                        read_roots: vec![".".into()],
                        deny_paths: vec![".git".into(), ".piko".into()],
                        allow_network: false,
                        allowed_commands: vec!["git status".into()],
                        denied_commands: vec!["rm".into()],
                    },
                ),
            ],
            &[("researcher", "readonly")],
        );
        let resolved = resolve_permissions(Some(&settings));
        let role_config = resolved
            .role_configs
            .get("researcher")
            .expect("role config present");
        assert_eq!(role_config.allowed_command_prefixes, vec!["git status"]);
        assert_eq!(role_config.denied_command_prefixes, vec!["rm"]);
        let role_policy = resolved
            .role_policies
            .get("researcher")
            .expect("role policy present");
        assert_eq!(role_policy.read_roots, vec!["."]);
        assert_eq!(role_policy.write_roots, vec!["/work"]);
        assert!(!role_policy.allow_network);
        // The session profile is untouched by role layers.
        assert_eq!(
            resolved.config.denied_command_prefixes,
            vec!["rm -rf", "curl -sSL | sh"]
        );
    }

    #[test]
    fn role_mapped_to_builtin_default_inherits_session_profile() {
        let settings = settings_with_roles(
            Some("locked"),
            &[("locked", command_profile())],
            &[("generalist", "default")],
        );
        let resolved = resolve_permissions(Some(&settings));
        assert!(resolved.role_configs.is_empty());
        assert!(resolved.role_policies.is_empty());
    }

    #[test]
    fn role_mapped_to_unknown_profile_is_dropped() {
        let settings = settings_with_roles(
            Some("locked"),
            &[("locked", command_profile())],
            &[("scout", "readonly-typo")],
        );
        let resolved = resolve_permissions(Some(&settings));
        assert!(resolved.role_configs.is_empty());
        assert!(resolved.role_policies.is_empty());
    }

    #[test]
    fn role_mappings_resolve_even_when_session_profile_is_default() {
        // The session profile is the built-in default (permissive), but the
        // role layer still tightens mapped roles.
        let settings = settings_with_roles(
            None,
            &[(
                "readonly",
                PermissionProfileSettings {
                    denied_commands: vec!["rm -rf".into()],
                    ..Default::default()
                },
            )],
            &[("researcher", "readonly")],
        );
        let resolved = resolve_permissions(Some(&settings));
        assert!(!resolved.materialize);
        assert!(resolved.role_configs.contains_key("researcher"));
        assert!(resolved.role_policies.contains_key("researcher"));
        assert_eq!(resolved.config, PermissionConfig::default());
    }

    #[test]
    fn no_roles_section_resolves_no_role_policies() {
        let settings = settings_with(Some("locked"), &[("locked", command_profile())]);
        let resolved = resolve_permissions(Some(&settings));
        assert!(resolved.role_configs.is_empty());
        assert!(resolved.role_policies.is_empty());
    }

    #[test]
    fn prefix_rule_match_respects_token_boundary() {
        assert!(prefix_rule_match("cargo test", "cargo test -- --nocapture"));
        assert!(prefix_rule_match("cargo test", "cargo test"));
        assert!(!prefix_rule_match("cargo test", "cargo testrun"));
        assert!(prefix_rule_match("git", "git status"));
        assert!(!prefix_rule_match("git", "gitlab-ci"));
        assert!(prefix_rule_match(
            "curl -sSL | sh",
            "curl -sSL | sh -s -- --yes"
        ));
        assert!(!prefix_rule_match("", "ls"));
        assert!(!prefix_rule_match("ls", ""));
    }

    #[test]
    fn evaluate_command_denies_matching_bash_command() {
        let config = PermissionConfig {
            denied_command_prefixes: vec!["rm -rf".into()],
            ..Default::default()
        };
        let decision = evaluate_command("bash", &json!({ "command": "rm -rf /tmp/x" }), &config);
        assert_eq!(
            decision,
            Some(CommandDecision::Deny {
                prefix: "rm -rf".into()
            })
        );
    }

    #[test]
    fn evaluate_command_allows_matching_process_start() {
        let config = PermissionConfig {
            allowed_command_prefixes: vec!["cargo test".into()],
            ..Default::default()
        };
        let decision = evaluate_command(
            "process",
            &json!({ "action": "start", "command": "cargo test -- --nocapture" }),
            &config,
        );
        assert_eq!(decision, Some(CommandDecision::Allow));
    }

    #[test]
    fn evaluate_command_deny_wins_over_allow() {
        let config = PermissionConfig {
            allowed_command_prefixes: vec!["cargo".into()],
            denied_command_prefixes: vec!["cargo test".into()],
        };
        let decision = evaluate_command("bash", &json!({ "command": "cargo test" }), &config);
        assert_eq!(
            decision,
            Some(CommandDecision::Deny {
                prefix: "cargo test".into()
            })
        );
    }

    #[test]
    fn evaluate_command_ignores_non_command_tools_and_actions() {
        let config = PermissionConfig {
            allowed_command_prefixes: vec!["cargo".into()],
            denied_command_prefixes: vec!["rm".into()],
        };
        assert_eq!(
            evaluate_command("edit", &json!({ "path": "a.rs" }), &config),
            None
        );
        assert_eq!(
            evaluate_command(
                "process",
                &json!({ "action": "write_stdin", "input": "cargo test" }),
                &config
            ),
            None
        );
        assert_eq!(
            evaluate_command("bash", &json!({ "command": "" }), &config),
            None
        );
        assert_eq!(evaluate_command("bash", &json!({}), &config), None);
    }

    #[test]
    fn evaluate_command_non_matching_keeps_existing_flow() {
        let config = PermissionConfig {
            allowed_command_prefixes: vec!["cargo test".into()],
            denied_command_prefixes: vec!["rm -rf".into()],
        };
        assert_eq!(
            evaluate_command("bash", &json!({ "command": "ls -la" }), &config),
            None
        );
    }
}
