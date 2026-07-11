// ---- Utils — helper functions for Supervisor ----

use piko_protocol::agents::HostTaskContext;
use piko_protocol::config::SandboxConfig;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

pub(crate) fn generate_task_id() -> String {
    format!(
        "task_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>()
    )
}

pub(crate) fn generate_work_id() -> String {
    crate::runtime::utils::generate_work_id()
}

pub(crate) fn load_sandbox_policy(sandbox: &SandboxConfig) -> piko_sandbox::policy::Policy {
    if !sandbox.enabled {
        tracing::info!("Sandbox disabled, using permissive policy");
        return permissive_policy();
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
    permissive_policy()
}

fn permissive_policy() -> piko_sandbox::policy::Policy {
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

pub(crate) fn ensure_run_context(opts: Option<OrchRunOptions>) -> OrchRunOptions {
    let mut opts = opts.unwrap_or(OrchRunOptions {
        command: OrchRunCommandOptions {
            target_agent_id: None,
        },
        history: None,
        host_context: None,
        source_turn_id: None,
        work_id: None,
    });
    if opts.host_context.is_none() {
        let id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>();
        opts.host_context = Some(HostTaskContext::new(format!("run_local_{id}")));
        opts.source_turn_id
            .get_or_insert_with(|| format!("turn_local_{id}"));
        opts.work_id.get_or_insert_with(generate_work_id);
    }
    opts
}
