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
pub(super) enum ResourceReadError {
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

impl McpProvider {
    /// Connect to an MCP server and discover its tools and resources.
    ///
    /// The whole handshake + discovery sequence is bounded by `timeout`
    /// (F-13 prewarm): a server that exceeds it fails closed for that server
    /// only — the caller logs and continues with the other servers.
    pub async fn connect(
        config: &McpServerConfig,
        timeout: std::time::Duration,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server {}: {e}", config.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("MCP child has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or("MCP child has no stdout".to_string())?;
        let reader = BufReader::new(stdout);

        let provider = McpProvider {
            id: config.name.clone(),
            name: config.name.clone(),
            tools: Vec::new(),
            resources: Vec::new(),
            templates: Vec::new(),
            child: Arc::new(Mutex::new(Some(child))),
            next_id: Arc::new(Mutex::new(1)),
            stdin: Arc::new(Mutex::new(Some(stdin))),
            stdout: Arc::new(Mutex::new(Some(reader))),
        };

        let (tools, resources, templates) = match tokio::time::timeout(timeout, async {
            // Initialize handshake
            provider
                .rpc_call(
                    "initialize",
                    Some(serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": { "name": "piko-hostd", "version": "0.1.0" }
                    })),
                )
                .await?;

            // Send initialized notification
            provider
                .rpc_notify("notifications/initialized", None)
                .await?;

            // Discover tools, then resources (best-effort: a server without
            // resource support contributes an empty catalog, not a failure).
            let tools = provider.discover().await?;
            let (resources, templates) = provider.discover_resources().await;
            Ok::<_, String>((tools, resources, templates))
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return Err(format!("MCP connect to {} failed: {error}", config.name));
            }
            Err(_elapsed) => {
                return Err(format!(
                    "MCP connect to {} timed out after {} ms",
                    config.name,
                    timeout.as_millis()
                ));
            }
        };

        Ok(McpProvider {
            id: config.name.clone(),
            name: config.name.clone(),
            tools,
            resources,
            templates,
            child: Arc::clone(&provider.child),
            next_id: Arc::clone(&provider.next_id),
            stdin: Arc::clone(&provider.stdin),
            stdout: Arc::clone(&provider.stdout),
        })
    }

    /// Best-effort resource discovery: failures yield an empty catalog.
    async fn discover_resources(&self) -> (Vec<McpResource>, Vec<McpResourceTemplate>) {
        let resources = match self.rpc_call("resources/list", None).await {
            Ok(value) => serde_json::from_value::<McpListResourcesResult>(value)
                .map(|result| result.resources)
                .unwrap_or_default(),
            Err(error) => {
                tracing::debug!(
                    server = %self.name,
                    error = %error,
                    "MCP server does not support resources/list; empty resource catalog"
                );
                Vec::new()
            }
        };
        let templates = match self.rpc_call("resources/templates/list", None).await {
            Ok(value) => serde_json::from_value::<McpListResourceTemplatesResult>(value)
                .map(|result| result.resource_templates)
                .unwrap_or_default(),
            Err(error) => {
                tracing::debug!(
                    server = %self.name,
                    error = %error,
                    "MCP server does not support resources/templates/list"
                );
                Vec::new()
            }
        };
        (resources, templates)
    }

    /// List the server's resources and resource templates, optionally
    /// filtered client-side over uri/name/description (F-13 "search").
    pub async fn list_resources(&self, query: Option<&str>) -> Result<serde_json::Value, String> {
        let filter = |value: &str| {
            query
                .map(|q| {
                    !q.trim().is_empty() && value.to_lowercase().contains(&q.trim().to_lowercase())
                })
                .unwrap_or(true)
        };
        let resources: Vec<serde_json::Value> = self
            .resources
            .iter()
            .filter(|r| {
                filter(&r.uri)
                    || filter(&r.name)
                    || (!r.description.is_empty() && filter(&r.description))
            })
            .map(|r| serde_json::to_value(r).unwrap_or_default())
            .collect();
        let templates: Vec<serde_json::Value> = self
            .templates
            .iter()
            .filter(|t| {
                filter(&t.uri_template)
                    || filter(&t.name)
                    || (!t.description.is_empty() && filter(&t.description))
            })
            .map(|t| serde_json::to_value(t).unwrap_or_default())
            .collect();
        Ok(serde_json::json!({
            "server": self.name,
            "resources": resources,
            "templates": templates,
        }))
    }

    /// Read a resource by URI and return its text content.
    pub(super) async fn read_resource(
        &self,
        uri: &str,
    ) -> Result<serde_json::Value, ResourceReadError> {
        let response = self
            .rpc_call("resources/read", Some(serde_json::json!({ "uri": uri })))
            .await
            .map_err(ResourceReadError::Transport)?;
        let result: McpReadResourceResult = serde_json::from_value(response).map_err(|e| {
            ResourceReadError::Transport(format!("Failed to parse MCP resources/read result: {e}"))
        })?;
        for content in result.contents {
            if let Some(text) = content.text {
                return Ok(serde_json::json!({
                    "server": self.name,
                    "uri": content.uri,
                    "text": text,
                }));
            }
            if content.blob.is_some() {
                return Err(ResourceReadError::BlobUnsupported(
                    "resource content is a blob; text content only".to_string(),
                ));
            }
        }
        Err(ResourceReadError::NotFound(
            "resource has no content".to_string(),
        ))
    }

    async fn discover(&self) -> Result<Vec<ToolDef>, String> {
        let response = self
            .rpc_call("tools/list", None)
            .await
            .map_err(|e| format!("MCP tools/list failed: {e}"))?;

        let result: McpListToolsResult = serde_json::from_value(response)
            .map_err(|e| format!("Failed to parse MCP tool list: {e}"))?;

        let tools: Vec<ToolDef> = result
            .tools
            .into_iter()
            .map(|mcp_tool| {
                let version_input = serde_json::json!({
                    "name": &mcp_tool.name,
                    "description": &mcp_tool.description,
                    "inputSchema": &mcp_tool.input_schema,
                });
                let version = piko_orchd_api::stable_internal_id(
                    "mcp-tool",
                    &[&self.id, &version_input.to_string()],
                );
                ToolDef {
                    name: mcp_tool.name,
                    version: version.clone(),
                    provenance: piko_protocol::PromptSource::new("mcp-server", &self.id)
                        .with_version(version),
                    description: mcp_tool.description,
                    input_schema: mcp_tool.input_schema,
                    executor: ToolExecutorRef {
                        kind: "mcp".into(),
                        target: self.id.clone(),
                        extra: None,
                    },
                    execution_mode: Some(ToolExecutionMode::Sequential),
                    exposure: Some(ToolExposure::Direct),
                    capabilities: Some(vec![ToolCapability::Network]),
                    approval: Some(ToolApprovalRequirement::OnRequest),
                    metadata: Some(ToolMetadata {
                        title: None,
                        read_only: Some(false),
                        destructive: Some(false),
                        mutates_workspace: Some(false),
                        produces_artifact: Some(false),
                    }),
                }
            })
            .collect();

        Ok(tools)
    }

    /// Send a JSON-RPC request and get the result.
    async fn rpc_call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let id = {
            let mut next = self.next_id.lock().await;
            let id = *next;
            *next += 1;
            id
        };

        let request = McpRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };

        let req_json =
            serde_json::to_string(&request).map_err(|e| format!("MCP serialize error: {e}"))?;

        // Write request
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard
                .as_mut()
                .ok_or("MCP stdin not available".to_string())?;
            stdin
                .write_all(req_json.as_bytes())
                .await
                .map_err(|e| format!("MCP write error: {e}"))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| format!("MCP write error: {e}"))?;
            stdin
                .flush()
                .await
                .map_err(|e| format!("MCP flush error: {e}"))?;
        }

        // Read response
        let mut line = String::new();
        {
            let mut stdout_guard = self.stdout.lock().await;
            let stdout = stdout_guard
                .as_mut()
                .ok_or("MCP stdout not available".to_string())?;
            let n = stdout
                .read_line(&mut line)
                .await
                .map_err(|e| format!("MCP read error: {e}"))?;
            if n == 0 {
                return Err("MCP process closed stdout".to_string());
            }
        }

        let response: McpResponse = serde_json::from_str(line.trim())
            .map_err(|e| format!("MCP parse error: {e} (line: {})", line.trim()))?;

        if let Some(error) = response.error {
            return Err(format!(
                "MCP error {} ({}): {}",
                method, error.code, error.message
            ));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn rpc_notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let req_json =
            serde_json::to_string(&request).map_err(|e| format!("MCP serialize error: {e}"))?;

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or("MCP stdin not available".to_string())?;
        stdin
            .write_all(req_json.as_bytes())
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("MCP flush error: {e}"))?;
        Ok(())
    }
}

#[async_trait]
impl ToolProvider for McpProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Mcp
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        self.tools.clone()
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        let tool_name = call.name.clone();
        let arguments = call.arguments.clone();

        match self
            .rpc_call(
                "tools/call",
                Some(serde_json::json!({
                    "name": tool_name,
                    "arguments": arguments
                })),
            )
            .await
        {
            Ok(result) => {
                // Parse MCP call result
                let call_result: Result<McpCallToolResult, _> =
                    serde_json::from_value(result.clone());

                match call_result {
                    Ok(cr) => {
                        let text = cr
                            .content
                            .iter()
                            .filter(|c| c.content_type == "text")
                            .map(|c| c.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");

                        if cr.is_error {
                            let msg = text.clone();
                            ToolExecResult {
                                ok: false,
                                value: Some(serde_json::Value::String(text)),
                                error: Some(ToolExecError {
                                    code: "mcp_tool_error".into(),
                                    message: msg,
                                    retryable: Some(false),
                                }),
                            }
                        } else {
                            ToolExecResult {
                                ok: true,
                                value: Some(serde_json::Value::String(text)),
                                error: None,
                            }
                        }
                    }
                    Err(_) => ToolExecResult {
                        ok: true,
                        value: Some(result),
                        error: None,
                    },
                }
            }
            Err(e) => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "mcp_error".into(),
                    message: e,
                    retryable: Some(true),
                }),
            },
        }
    }
}

impl Drop for McpProvider {
    fn drop(&mut self) {
        // The child process is kill_on_drop, so it will be cleaned up.
        // Close stdin to let the process know we're done.
    }
}
