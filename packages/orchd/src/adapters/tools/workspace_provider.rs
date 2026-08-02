// ---- WorkspaceToolProvider — filesystem and process tools ----
//
// Integrates piko-sandbox library for policy-based filesystem access control.
// File operations and process execution commands are checked against the sandbox policy.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use piko_sandbox::policy::Policy;

use crate::domain::tools::definition::{ToolDef, ToolProviderSource};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

use super::workspace_handlers::{execute_workspace_tool, workspace_tools};

// ---- Provider ----

pub struct WorkspaceToolProvider {
    policy: Arc<Policy>,
    shell_path: String,
}

impl WorkspaceToolProvider {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy: Arc::new(policy),
            shell_path: "bash".into(),
        }
    }

    pub fn with_shell(policy: Policy, shell_path: impl Into<String>) -> Self {
        Self {
            policy: Arc::new(policy),
            shell_path: shell_path.into(),
        }
    }

    /// Create a provider with a permissive policy (read/write current dir).
    pub fn permissive() -> Self {
        Self::new(Policy {
            version: 1,
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from(".")],
            deny: vec![PathBuf::from(".git")],
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
        })
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
        execute_workspace_tool(&self.policy, &self.shell_path, &call, &context).await
    }
}
