/// Generate a unique work identifier for a task input cycle.
pub(crate) fn generate_work_id() -> String {
    format!("work_{}", uuid::Uuid::new_v4())
}

/// Returns the current Unix timestamp in milliseconds.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
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
    if !sandbox.enabled {
        tracing::info!("Sandbox disabled, using permissive policy");
        return permissive_sandbox_policy();
    }
    if let Some(ref policy_path) = sandbox.policy_path {
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
    tracing::info!("No sandbox policy found, using permissive defaults");
    permissive_sandbox_policy()
}

fn permissive_sandbox_policy() -> piko_sandbox::policy::Policy {
    piko_sandbox::policy::Policy {
        version: 1,
        read: vec![std::path::PathBuf::from(".")],
        write: vec![std::path::PathBuf::from(".")],
        deny: vec![std::path::PathBuf::from(".git")],
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
