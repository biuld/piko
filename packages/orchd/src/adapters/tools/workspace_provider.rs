// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations are checked against the sandbox policy; process execution
// (the `bash` tool) runs through the piko-sandbox PTY runner, wrapped in the
// platform OS sandbox when `os_sandbox` is set (F-08). The provider owns the
// long-lived `ProcessManager` and the discovered `EnvironmentProfile`
// (F-08 slice 2).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use piko_sandbox::exec::ShellSnapshot;
use piko_sandbox::exec::env::EnvironmentProfile;
use piko_sandbox::exec::process::ProcessManager;
use piko_sandbox::policy::Policy;

use crate::domain::tools::definition::{ToolDef, ToolProviderSource};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

use super::process_handlers::execute_process_tool;
use super::shell_handlers::execute_shell_tool;
use super::workspace_handlers::{execute_workspace_tool, workspace_tools};

// ---- Provider ----

pub struct WorkspaceToolProvider {
    /// Session policy (fallback for unmapped roles).
    policy: Arc<Policy>,
    /// F-19: per-role policies keyed by agent role. A role with an entry
    /// executes workspace tools under this policy; absent roles use
    /// `policy`.
    role_policies: HashMap<String, Arc<Policy>>,
    shell: ShellSnapshot,
    env: EnvironmentProfile,
    processes: Arc<ProcessManager>,
    os_sandbox: bool,
}

impl WorkspaceToolProvider {
    /// `os_sandbox` decides whether process execution runs inside the
    /// platform OS sandbox (seatbelt/bwrap, F-08); the shell snapshot and
    /// environment profile are resolved once here and reused for every call.
    pub fn new(policy: Policy, os_sandbox: bool, processes: Arc<ProcessManager>) -> Self {
        Self {
            policy: Arc::new(policy),
            role_policies: HashMap::new(),
            shell: ShellSnapshot::capture(None),
            env: EnvironmentProfile::discover(None),
            processes,
            os_sandbox,
        }
    }

    /// Create a provider with an explicit shell path.
    pub fn with_shell(
        policy: Policy,
        shell_path: impl Into<String>,
        os_sandbox: bool,
        processes: Arc<ProcessManager>,
    ) -> Self {
        let shell_path = shell_path.into();
        Self {
            policy: Arc::new(policy),
            role_policies: HashMap::new(),
            shell: ShellSnapshot::capture(Some(&shell_path)),
            env: EnvironmentProfile::discover(Some(&shell_path)),
            processes,
            os_sandbox,
        }
    }

    /// Attach F-19 per-role sandbox policies. Roles without an entry keep
    /// the session policy; a role policy is only applied to agents whose
    /// registered spec carries that role.
    pub fn with_role_policies(mut self, role_policies: HashMap<String, Policy>) -> Self {
        self.role_policies = role_policies
            .into_iter()
            .map(|(role, policy)| (role, Arc::new(policy)))
            .collect();
        self
    }

    /// Policy for the executing agent's role, falling back to the session
    /// policy for unmapped or unknown roles.
    fn policy_for(&self, role: Option<&str>) -> Arc<Policy> {
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
            "bash" => {
                execute_shell_tool(&policy, &self.shell, self.os_sandbox, &call, &context).await
            }
            "process" => {
                execute_process_tool(
                    &self.processes,
                    &policy,
                    &self.shell,
                    self.os_sandbox,
                    &call,
                    &context,
                )
                .await
            }
            "environment" => ToolExecResult {
                ok: true,
                value: Some(serde_json::json!({
                    "shell": self.env.shell,
                    "os": self.env.os,
                    "arch": self.env.arch,
                    "cwd": self.env.cwd,
                    "path": self.env.path,
                    "tools": self.env.tools,
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
    use piko_sandbox::policy::Policy;

    use crate::ports::tool_provider::{ToolExecutionContext, ToolProvider};

    use super::WorkspaceToolProvider;

    fn policy(read: &[&str], write: &[&str], deny: &[&str], network: bool) -> Policy {
        Policy {
            version: 1,
            read: read.iter().map(PathBuf::from).collect(),
            write: write.iter().map(PathBuf::from).collect(),
            deny: deny.iter().map(PathBuf::from).collect(),
            allowed_commands: Vec::new(),
            allow_network: network,
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
        let provider = WorkspaceToolProvider::new(session, false, Arc::new(ProcessManager::new()))
            .with_role_policies(HashMap::from([("researcher".to_string(), role)]));

        assert_eq!(
            provider.policy_for(Some("researcher")).read,
            vec![PathBuf::from("/docs")]
        );
        assert_eq!(
            provider.policy_for(Some("developer")).read,
            vec![PathBuf::from(".")]
        );
        assert_eq!(provider.policy_for(None).read, vec![PathBuf::from(".")]);
    }

    #[test]
    fn writable_roots_for_reflects_role_policy() {
        let cwd = std::env::current_dir().unwrap();
        let session = policy(&["."], &["."], &[], false);
        let role = policy(&["/docs"], &["/work"], &[], false);
        let provider = WorkspaceToolProvider::new(session, false, Arc::new(ProcessManager::new()))
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
