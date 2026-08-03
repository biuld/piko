// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations are checked against the sandbox policy; process execution
// (the `bash` tool) runs through the piko-sandbox PTY runner, wrapped in the
// platform OS sandbox when `os_sandbox` is set (F-08).

use std::sync::Arc;

use async_trait::async_trait;

use piko_sandbox::exec::ShellSnapshot;
use piko_sandbox::policy::Policy;

use crate::domain::tools::definition::{ToolDef, ToolProviderSource};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

use super::workspace_handlers::{execute_workspace_tool, workspace_tools};

// ---- Provider ----

pub struct WorkspaceToolProvider {
    policy: Arc<Policy>,
    shell: ShellSnapshot,
    os_sandbox: bool,
}

impl WorkspaceToolProvider {
    /// `os_sandbox` decides whether `bash` calls run inside the platform OS
    /// sandbox (seatbelt/bwrap, F-08 slice 1); the shell snapshot is
    /// resolved once here and reused for every call.
    pub fn new(policy: Policy, os_sandbox: bool) -> Self {
        Self {
            policy: Arc::new(policy),
            shell: ShellSnapshot::capture(None),
            os_sandbox,
        }
    }

    /// Create a provider with an explicit shell path.
    pub fn with_shell(policy: Policy, shell_path: impl Into<String>, os_sandbox: bool) -> Self {
        let shell_path = shell_path.into();
        Self {
            policy: Arc::new(policy),
            shell: ShellSnapshot::capture(Some(&shell_path)),
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
        execute_workspace_tool(&self.policy, &self.shell, self.os_sandbox, &call, &context).await
    }
}
