use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};
use piko_protocol::ToolCall;
use piko_protocol::tools::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolExposure, ToolMetadata, ToolProviderSource,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::domain::config::McpServerConfig;

use super::types::*;

/// Typed failure for `McpProvider::read_resource` so the caller can map
/// distinct non-retryable codes (F-13).
#[derive(Debug)]
pub(crate) enum ResourceReadError {
    NotFound(String),
    BlobUnsupported(String),
    Transport(String),
}

impl std::fmt::Display for ResourceReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceReadError::NotFound(message)
            | ResourceReadError::BlobUnsupported(message)
            | ResourceReadError::Transport(message) => write!(f, "{message}"),
        }
    }
}

/// A ToolProvider backed by an MCP server process connected via stdio.
///
/// `Clone` shares the same child process / stdio handles so the provider can be
/// registered on both the classic Runtime and the Execution runtime.
#[derive(Clone)]
pub struct McpProvider {
    id: String,
    name: String,
    pub(super) tools: Vec<ToolDef>,
    /// F-13: cached `resources/list` catalog (first page).
    pub(super) resources: Vec<McpResource>,
    /// F-13: cached `resources/templates/list` catalog (first page).
    pub(super) templates: Vec<McpResourceTemplate>,
    /// Child process handle. We use Arc<Mutex<...>> to satisfy Send + Sync.
    child: Arc<Mutex<Option<Child>>>,
    /// Next JSON-RPC request ID.
    next_id: Arc<Mutex<u64>>,
    /// Stdin writer for the MCP process.
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    /// Stdout reader for the MCP process.
    stdout: Arc<Mutex<Option<BufReader<tokio::process::ChildStdout>>>>,
}

impl std::fmt::Debug for McpProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpProvider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("tool_count", &self.tools.len())
            .finish()
    }
}

mod methods;
mod tool_provider;

impl Drop for McpProvider {
    fn drop(&mut self) {
        // The child process is kill_on_drop, so it will be cleaned up.
        // Close stdin to let the process know we're done.
    }
}
