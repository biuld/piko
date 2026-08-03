// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations are checked against the sandbox policy; process execution
// (the `bash` tool) runs through the piko-sandbox PTY runner, wrapped in the
// platform OS sandbox when `os_sandbox` is set (F-08). The provider owns the
// long-lived `ProcessManager` and the discovered `EnvironmentProfile`
// (F-08 slice 2).

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
    policy: Arc<Policy>,
    shell: ShellSnapshot,
    env: EnvironmentProfile,
    processes: Arc<ProcessManager>,
    os_sandbox: bool,
}

impl WorkspaceToolProvider {
    /// `os_sandbox` decides whether process execution runs inside the
    /// platform OS sandbox (seatbelt/bwrap, F-08); the shell snapshot and
    /// environment profile are resolved once here and reused for every call.
    pub fn new(policy: Policy, os_sandbox: bool) -> Self {
        Self {
            policy: Arc::new(policy),
            shell: ShellSnapshot::capture(None),
            env: EnvironmentProfile::discover(None),
            processes: Arc::new(ProcessManager::new()),
            os_sandbox,
        }
    }

    /// Create a provider with an explicit shell path.
    pub fn with_shell(policy: Policy, shell_path: impl Into<String>, os_sandbox: bool) -> Self {
        let shell_path = shell_path.into();
        Self {
            policy: Arc::new(policy),
            shell: ShellSnapshot::capture(Some(&shell_path)),
            env: EnvironmentProfile::discover(Some(&shell_path)),
            processes: Arc::new(ProcessManager::new()),
            os_sandbox,
        }
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

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        workspace_tools()
    }

    async fn execute(
        &self,
        call: crate::domain::tools::call::ToolCall,
        context: ToolExecutionContext,
    ) -> ToolExecResult {
        match call.name.as_str() {
            "bash" => {
                execute_shell_tool(&self.policy, &self.shell, self.os_sandbox, &call, &context)
                    .await
            }
            "process" => {
                execute_process_tool(
                    &self.processes,
                    &self.policy,
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
            _ => execute_workspace_tool(&self.policy, &call, &context).await,
        }
    }
}
