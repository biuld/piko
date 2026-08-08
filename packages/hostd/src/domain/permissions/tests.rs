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
