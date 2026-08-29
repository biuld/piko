// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations are checked against the sandbox policy; process execution
// runs through the piko-sandbox PTY runner and is contained by the platform
// OS sandbox unless explicitly approved as escalated. The provider owns the
// long-lived `ProcessManager` and the discovered `EnvironmentProfile`
// (F-08 slice 2).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use piko_sandbox::exec::ShellSnapshot;
use piko_sandbox::exec::env::EnvironmentProfile;
use piko_sandbox::exec::process::ProcessManager;
use piko_sandbox::policy::EffectivePermissions;

use crate::domain::tools::definition::{ToolDef, ToolProviderSource};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

use super::exec_handlers::{execute_exec_command, execute_write_stdin};
use super::workspace_handlers::{execute_workspace_tool, workspace_tools};

// ---- Provider ----

pub struct WorkspaceToolProvider {
    /// Session policy (fallback for unmapped roles).
    policy: Arc<EffectivePermissions>,
    /// F-19: per-role policies keyed by agent role. A role with an entry
    /// executes workspace tools under this policy; absent roles use
    /// `policy`.
    role_policies: HashMap<String, Arc<EffectivePermissions>>,
    shell: ShellSnapshot,
    env: EnvironmentProfile,
    processes: Arc<ProcessManager>,
}

impl WorkspaceToolProvider {
    /// The shell snapshot and environment profile are resolved once here and
    /// reused for every call.
    pub fn new(policy: EffectivePermissions, processes: Arc<ProcessManager>) -> Self {
        Self {
            policy: Arc::new(policy),
            role_policies: HashMap::new(),
            shell: ShellSnapshot::capture(None),
            env: EnvironmentProfile::discover(None),
            processes,
        }
    }

    /// Create a provider with an explicit shell path.
    pub fn with_shell(
        policy: EffectivePermissions,
        shell_path: impl Into<String>,
        processes: Arc<ProcessManager>,
    ) -> Self {
        let shell_path = shell_path.into();
        Self {
            policy: Arc::new(policy),
            role_policies: HashMap::new(),
            shell: ShellSnapshot::capture(Some(&shell_path)),
            env: EnvironmentProfile::discover(Some(&shell_path)),
            processes,
        }
    }

    /// Attach F-19 per-role sandbox policies. Roles without an entry keep
    /// the session policy; a role policy is only applied to agents whose
    /// registered spec carries that role.
    pub fn with_role_policies(
        mut self,
        role_policies: HashMap<String, EffectivePermissions>,
    ) -> Self {
        self.role_policies = role_policies
            .into_iter()
            .map(|(role, policy)| (role, Arc::new(policy)))
            .collect();
        self
    }

    /// EffectivePermissions for the executing agent's role, falling back to the session
    /// policy for unmapped or unknown roles.
    fn policy_for(&self, role: Option<&str>) -> Arc<EffectivePermissions> {
        role.and_then(|role| self.role_policies.get(role).cloned())
            .unwrap_or_else(|| Arc::clone(&self.policy))
    }
}

#[async_trait]
impl ToolProvider for WorkspaceToolProvider {
    fn id(&self) -> &str {
        "workspace"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Workspace
    }

    fn writable_roots_for(
        &self,
        context: &ToolExecutionContext,
    ) -> Option<Vec<std::path::PathBuf>> {
        let cwd = std::env::current_dir().ok()?;
        Some(
            self.policy_for(context.agent_role.as_deref())
                .writable_roots(&cwd),
        )
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        workspace_tools()
    }

    async fn execute(
        &self,
        call: crate::domain::tools::call::ToolCall,
        context: ToolExecutionContext,
    ) -> ToolExecResult {
        let policy = self.policy_for(context.agent_role.as_deref());
        match call.name.as_str() {
            "exec_command" => {
                execute_exec_command(&self.processes, &policy, &self.shell, &call, &context).await
            }
            "write_stdin" => execute_write_stdin(&self.processes, &call).await,
            "environment" => ToolExecResult {
                ok: true,
                value: Some(serde_json::json!({
                    "shell": self.env.shell,
                    "os": self.env.os,
                    "arch": self.env.arch,
                    "cwd": self.env.cwd,
                    "path": self.env.path,
                    "tools": self.env.tools,
                    "execution": {
                        "readRoots": policy.readable_roots(&self.shell.cwd),
                        "writeRoots": policy.writable_roots(&self.shell.cwd),
                        "scratchRoots": policy.scratch_roots(&self.shell.cwd),
                        "network": if policy.network.is_enabled() { "enabled" } else { "restricted" },
                        "containment": "required",
                        "backend": piko_sandbox::platform::backend_name(),
                        "backendAvailable": piko_sandbox::platform::backend_available(),
                    },
                })),
                error: None,
            },
            _ => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                execute_workspace_tool(&cwd, &policy, &call, &context).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use piko_sandbox::exec::process::ProcessManager;
    use piko_sandbox::policy::EffectivePermissions;

    use crate::ports::tool_provider::{ToolExecutionContext, ToolProvider};

    use super::WorkspaceToolProvider;

    fn policy(
        read_roots: &[&str],
        write_roots: &[&str],
        denied_read_roots: &[&str],
        network: bool,
    ) -> EffectivePermissions {
        EffectivePermissions {
            version: 1,
            read_roots: read_roots.iter().map(PathBuf::from).collect(),
            write_roots: write_roots.iter().map(PathBuf::from).collect(),
            scratch_roots: vec![],
            denied_read_roots: denied_read_roots.iter().map(PathBuf::from).collect(),
            denied_write_roots: Vec::new(),
            network: network.into(),
        }
    }

    fn context(role: Option<&str>) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session".into(),
            agent_instance_id: "agent".into(),
            execution_id: "exec".into(),
            cancellation: None,
            agent_id: "main".into(),
            agent_role: role.map(str::to_string),
            agent_kind: piko_protocol::AgentKind::Supervisor,
            tool_set_ids: vec![],
            turn_index: None,
            event_seq: None,
            next_event_seq: None,
            parent_message_id: None,
            content_index: None,
            tool_call_index: None,
            tool_entity_id: None,
            host_context: None,
            source_turn_id: None,
            context_remaining: None,
        }
    }

    #[test]
    fn policy_for_selects_role_policy_with_session_fallback() {
        let session = policy(&["."], &["."], &[".git"], false);
        let role = policy(&["/docs"], &[], &["/docs/private"], false);
        let provider = WorkspaceToolProvider::new(session, Arc::new(ProcessManager::new()))
            .with_role_policies(HashMap::from([("researcher".to_string(), role)]));

        assert_eq!(
            provider.policy_for(Some("researcher")).read_roots,
            vec![PathBuf::from("/docs")]
        );
        assert_eq!(
            provider.policy_for(Some("developer")).read_roots,
            vec![PathBuf::from(".")]
        );
        assert_eq!(
            provider.policy_for(None).read_roots,
            vec![PathBuf::from(".")]
        );
    }

    #[test]
    fn writable_roots_for_reflects_role_policy() {
        let cwd = std::env::current_dir().unwrap();
        let session = policy(&["."], &["."], &[], false);
        let role = policy(&["/docs"], &["/work"], &[], false);
        let provider = WorkspaceToolProvider::new(session, Arc::new(ProcessManager::new()))
            .with_role_policies(HashMap::from([("researcher".to_string(), role)]));

        let role_roots = provider
            .writable_roots_for(&context(Some("researcher")))
            .expect("roots projected");
        assert_eq!(role_roots, vec![PathBuf::from("/work")]);

        let session_roots = provider
            .writable_roots_for(&context(None))
            .expect("roots projected");
        assert!(session_roots.contains(&cwd));
    }
}
