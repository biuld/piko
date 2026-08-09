use std::collections::HashMap;

/// Returns the current Unix timestamp in milliseconds.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Bound a JSON argument body recorded in spans to a sane size.
pub fn truncate_json(value: &serde_json::Value, max: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.len() <= max {
        text.to_string()
    } else {
        let mut result = text.chars().take(max).collect::<String>();
        result.push_str("...");
        result
    }
}

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}

/// Produce a stable runtime tool call message ID.
pub fn runtime_tool_call_message_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{parent_message_id}:tool_call:{tool_call_index}")
}

/// Generate a stable runtime tool entity ID.
pub(crate) fn runtime_tool_entity_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{}:tool:{}", parent_message_id, tool_call_index)
}

/// Load sandbox policy for workspace tools (Execution bootstrap).
pub(crate) fn load_sandbox_policy(
    sandbox: &piko_protocol::config::SandboxConfig,
) -> piko_sandbox::policy::EffectivePermissions {
    // F-17 permission profiles: the host materializes the resolved profile's
    // file/network policy here. Empty rule lists inherit the permissive
    // defaults per field (partial profiles do not lock down access).
    if let Some(ref profile) = sandbox.policy_profile {
        let permissive = default_sandbox_policy();
        tracing::info!("Materializing sandbox policy from permission profile");
        return piko_sandbox::policy::EffectivePermissions {
            version: 1,
            read_roots: if profile.read_roots.is_empty() {
                permissive.read_roots
            } else {
                profile
                    .read_roots
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            write_roots: if profile.write_roots.is_empty() {
                permissive.write_roots
            } else {
                profile
                    .write_roots
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            scratch_roots: if profile.scratch_roots.is_empty() {
                permissive.scratch_roots
            } else {
                profile
                    .scratch_roots
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            denied_read_roots: if profile.deny_paths.is_empty() {
                permissive.denied_read_roots
            } else {
                profile
                    .deny_paths
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            denied_write_roots: permissive.denied_write_roots,
            network: profile.allow_network.into(),
        };
    }
    tracing::info!("Using built-in restricted workspace policy");
    default_sandbox_policy()
}

/// Materialize F-19 per-role sandbox policies from `role_policies`.
///
/// Each role entry follows the same rules as the session profile path:
/// empty rule lists inherit the permissive defaults per field, and the
/// execution whitelist always comes from the permissive default. A role
/// without an entry keeps the session policy — role layers can never widen
/// it (hostd already validates role mappings against defined profiles).
pub(crate) fn load_role_sandbox_policies(
    sandbox: &piko_protocol::config::SandboxConfig,
) -> HashMap<String, piko_sandbox::policy::EffectivePermissions> {
    let permissive = default_sandbox_policy();
    sandbox
        .role_policies
        .iter()
        .map(|(role, profile)| {
            let policy = piko_sandbox::policy::EffectivePermissions {
                version: 1,
                read_roots: if profile.read_roots.is_empty() {
                    permissive.read_roots.clone()
                } else {
                    profile
                        .read_roots
                        .iter()
                        .map(std::path::PathBuf::from)
                        .collect()
                },
                write_roots: if profile.write_roots.is_empty() {
                    permissive.write_roots.clone()
                } else {
                    profile
                        .write_roots
                        .iter()
                        .map(std::path::PathBuf::from)
                        .collect()
                },
                scratch_roots: if profile.scratch_roots.is_empty() {
                    permissive.scratch_roots.clone()
                } else {
                    profile
                        .scratch_roots
                        .iter()
                        .map(std::path::PathBuf::from)
                        .collect()
                },
                denied_read_roots: if profile.deny_paths.is_empty() {
                    permissive.denied_read_roots.clone()
                } else {
                    profile
                        .deny_paths
                        .iter()
                        .map(std::path::PathBuf::from)
                        .collect()
                },
                denied_write_roots: permissive.denied_write_roots.clone(),
                network: profile.allow_network.into(),
            };
            (role.clone(), policy)
        })
        .collect()
}

fn default_sandbox_policy() -> piko_sandbox::policy::EffectivePermissions {
    let mut scratch = vec![std::env::temp_dir()];
    for candidate in ["/tmp", "/private/tmp"] {
        let path = std::path::PathBuf::from(candidate);
        if path.exists() && !scratch.contains(&path) {
            scratch.push(path);
        }
    }
    piko_sandbox::policy::EffectivePermissions {
        version: 1,
        read_roots: vec![std::path::PathBuf::from(".")],
        write_roots: vec![std::path::PathBuf::from(".")],
        scratch_roots: scratch,
        // Host-owned state can contain approvals and configuration and is
        // outside the agent's problem domain.
        denied_read_roots: vec![std::path::PathBuf::from(".piko")],
        // Repository and agent-control metadata are useful context but are
        // immutable under default authority.
        denied_write_roots: vec![
            std::path::PathBuf::from(".git"),
            std::path::PathBuf::from(".codex"),
            std::path::PathBuf::from(".agents"),
        ],
        network: false.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::config::{PermissionPolicy, SandboxConfig};

    fn permissive() -> piko_sandbox::policy::EffectivePermissions {
        default_sandbox_policy()
    }

    fn profile(
        read_roots: &[&str],
        write_roots: &[&str],
        deny_paths: &[&str],
        allow_network: bool,
    ) -> PermissionPolicy {
        PermissionPolicy {
            read_roots: read_roots.iter().map(|s| (*s).to_string()).collect(),
            write_roots: write_roots.iter().map(|s| (*s).to_string()).collect(),
            scratch_roots: vec![],
            deny_paths: deny_paths.iter().map(|s| (*s).to_string()).collect(),
            allow_network,
        }
    }

    #[test]
    fn default_sandbox_uses_builtin_policy() {
        let sandbox = SandboxConfig::default();
        assert_eq!(load_sandbox_policy(&sandbox), permissive());
    }

    #[test]
    fn profile_materializes_file_and_network_policy() {
        let sandbox = SandboxConfig {
            policy_profile: Some(profile(&["/work"], &["/work"], &["/work/secret"], true)),
            ..Default::default()
        };
        let policy = load_sandbox_policy(&sandbox);
        assert_eq!(policy.read_roots, vec![std::path::PathBuf::from("/work")]);
        assert_eq!(policy.write_roots, vec![std::path::PathBuf::from("/work")]);
        assert_eq!(
            policy.denied_read_roots,
            vec![std::path::PathBuf::from("/work/secret")]
        );
        assert!(policy.network.is_enabled());
    }

    #[test]
    fn partial_profile_inherits_permissive_file_defaults() {
        let sandbox = SandboxConfig {
            policy_profile: Some(profile(&[], &[], &[], false)),
            ..Default::default()
        };
        let policy = load_sandbox_policy(&sandbox);
        assert_eq!(policy.read_roots, permissive().read_roots);
        assert_eq!(policy.write_roots, permissive().write_roots);
        assert_eq!(policy.denied_read_roots, permissive().denied_read_roots);
    }

    #[test]
    fn role_policies_materialize_with_permissive_inheritance() {
        let mut roles = std::collections::HashMap::new();
        roles.insert(
            "researcher".to_string(),
            profile(&["/docs"], &[], &["/docs/private"], false),
        );
        let sandbox = SandboxConfig {
            policy_profile: Some(profile(&["/work"], &["/work"], &[], true)),
            role_policies: roles,
            ..Default::default()
        };
        let policies = load_role_sandbox_policies(&sandbox);
        assert_eq!(policies.len(), 1);
        let researcher = policies.get("researcher").expect("role policy present");
        assert_eq!(
            researcher.read_roots,
            vec![std::path::PathBuf::from("/docs")]
        );
        // Empty write roots inherit the permissive default; deny paths and
        // network come from the role profile; the whitelist is inherited.
        assert_eq!(researcher.write_roots, permissive().write_roots);
        assert_eq!(
            researcher.denied_read_roots,
            vec![std::path::PathBuf::from("/docs/private")]
        );
        assert!(!researcher.network.is_enabled());
    }

    #[test]
    fn roles_without_entries_keep_the_session_policy() {
        let sandbox = SandboxConfig {
            policy_profile: Some(profile(&["/work"], &["/work"], &[], true)),
            ..Default::default()
        };
        let policies = load_role_sandbox_policies(&sandbox);
        assert!(policies.is_empty());
    }

    #[test]
    fn default_sandbox_with_no_profile_is_restricted_workspace() {
        let sandbox = SandboxConfig::default();
        assert_eq!(load_sandbox_policy(&sandbox), permissive());
    }
}
