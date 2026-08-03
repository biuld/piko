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
) -> piko_sandbox::policy::Policy {
    // Permission profiles materialize regardless of `enabled`: they refine
    // the authorization policy (file roots/deny/network) that applies even
    // when OS-level sandboxing is off. `[sandbox] policy-path` and
    // `.piko/sandbox.json` are OS-sandbox policy sources and only apply
    // when the sandbox is enabled.
    if !sandbox.enabled && sandbox.policy_profile.is_none() {
        tracing::info!("Sandbox disabled, using permissive policy");
        return permissive_sandbox_policy();
    }
    if sandbox.enabled
        && let Some(ref policy_path) = sandbox.policy_path
    {
        let path = std::path::Path::new(policy_path);
        if path.exists() {
            match piko_sandbox::policy::Policy::load(path) {
                Ok(p) => {
                    tracing::info!("Loaded sandbox policy from {}", path.display());
                    return p;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load sandbox policy from {}: {}, using permissive",
                        path.display(),
                        e
                    );
                }
            }
        } else {
            tracing::warn!(
                "Sandbox policy path configured but file not found: {}, using permissive",
                path.display()
            );
        }
    }
    // F-17 permission profiles: the host materializes the resolved profile's
    // file/network policy here. Empty rule lists inherit the permissive
    // defaults per field (partial profiles do not lock down access), and the
    // execution whitelist always comes from the permissive default list.
    if let Some(ref profile) = sandbox.policy_profile {
        let permissive = permissive_sandbox_policy();
        tracing::info!("Materializing sandbox policy from permission profile");
        return piko_sandbox::policy::Policy {
            version: 1,
            read: if profile.read_roots.is_empty() {
                permissive.read
            } else {
                profile
                    .read_roots
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            write: if profile.write_roots.is_empty() {
                permissive.write
            } else {
                profile
                    .write_roots
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            deny: if profile.deny_paths.is_empty() {
                permissive.deny
            } else {
                profile
                    .deny_paths
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect()
            },
            allowed_commands: permissive.allowed_commands,
            allow_network: profile.allow_network,
        };
    }
    if sandbox.enabled {
        let default_path = std::path::Path::new(".piko/sandbox.json");
        if default_path.exists() {
            match piko_sandbox::policy::Policy::load(default_path) {
                Ok(p) => {
                    tracing::info!("Loaded sandbox policy from default location");
                    return p;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load sandbox policy from default location: {}, using permissive",
                        e
                    );
                }
            }
        }
    }
    tracing::info!("No sandbox policy found, using permissive defaults");
    permissive_sandbox_policy()
}

fn permissive_sandbox_policy() -> piko_sandbox::policy::Policy {
    piko_sandbox::policy::Policy {
        version: 1,
        read: vec![std::path::PathBuf::from(".")],
        write: vec![std::path::PathBuf::from(".")],
        deny: vec![
            std::path::PathBuf::from(".git"),
            // hostd's own workspace state (approvals, project settings):
            // tools must never be able to self-grant approvals or rewrite
            // their own configuration through `edit`/`write`.
            std::path::PathBuf::from(".piko"),
        ],
        allowed_commands: vec![
            "ls".into(),
            "cat".into(),
            "head".into(),
            "tail".into(),
            "find".into(),
            "grep".into(),
            "rg".into(),
            "git".into(),
            "echo".into(),
            "mkdir".into(),
            "cp".into(),
            "mv".into(),
            "rm".into(),
            "wc".into(),
            "sort".into(),
            "uniq".into(),
            "sed".into(),
            "awk".into(),
            "diff".into(),
            "npm".into(),
            "npx".into(),
            "node".into(),
            "bun".into(),
            "cargo".into(),
            "python3".into(),
            "python".into(),
            "go".into(),
            "make".into(),
            "rustc".into(),
            "tsc".into(),
            "biome".into(),
            "prettier".into(),
        ],
        allow_network: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::config::{PermissionPolicy, SandboxConfig};

    fn permissive() -> piko_sandbox::policy::Policy {
        permissive_sandbox_policy()
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
            deny_paths: deny_paths.iter().map(|s| (*s).to_string()).collect(),
            allow_network,
        }
    }

    #[test]
    fn disabled_sandbox_without_profile_is_permissive() {
        let sandbox = SandboxConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(load_sandbox_policy(&sandbox), permissive());
    }

    #[test]
    fn profile_materializes_even_when_sandbox_disabled() {
        let sandbox = SandboxConfig {
            enabled: false,
            policy_profile: Some(profile(&["/work"], &["/work"], &["/work/secret"], true)),
            ..Default::default()
        };
        let policy = load_sandbox_policy(&sandbox);
        assert_eq!(policy.read, vec![std::path::PathBuf::from("/work")]);
        assert_eq!(policy.write, vec![std::path::PathBuf::from("/work")]);
        assert_eq!(policy.deny, vec![std::path::PathBuf::from("/work/secret")]);
        assert!(policy.allow_network);
        // The execution whitelist is inherited from the permissive default.
        assert_eq!(policy.allowed_commands, permissive().allowed_commands);
    }

    #[test]
    fn partial_profile_inherits_permissive_file_defaults() {
        let sandbox = SandboxConfig {
            enabled: true,
            policy_profile: Some(profile(&[], &[], &[], false)),
            ..Default::default()
        };
        let policy = load_sandbox_policy(&sandbox);
        assert_eq!(policy.read, permissive().read);
        assert_eq!(policy.write, permissive().write);
        assert_eq!(policy.deny, permissive().deny);
    }

    #[test]
    fn policy_path_file_wins_over_profile() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("policy.json");
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "read": ["/data"],
                "write": ["/data"],
                "allowedCommands": ["cat"],
                "allowNetwork": false
            }"#,
        )
        .unwrap();
        let sandbox = SandboxConfig {
            enabled: true,
            policy_path: Some(path.display().to_string()),
            policy_profile: Some(profile(&["/work"], &["/work"], &[], true)),
            ..Default::default()
        };
        let policy = load_sandbox_policy(&sandbox);
        assert_eq!(policy.read, vec![std::path::PathBuf::from("/data")]);
        assert_eq!(policy.allowed_commands, vec!["cat"]);
        assert!(!policy.allow_network);
    }

    #[test]
    fn enabled_sandbox_with_no_policy_sources_is_permissive() {
        let sandbox = SandboxConfig {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(load_sandbox_policy(&sandbox), permissive());
    }
}
