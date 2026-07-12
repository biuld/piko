// ---- MCP: Model Context Protocol integration ----
//
// Connects to MCP servers via stdio, discovers tools, and registers
// them as ToolProvider implementations for orchd's ToolRegistry.
//
// Protocol: JSON-RPC 2.0 over stdin/stdout (newline-delimited).

#[allow(unused_imports)]
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;

use orchd_api::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};
use piko_protocol::ToolCall;
use piko_protocol::tools::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolExposure, ToolMetadata, ToolProviderSource,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ---- MCP JSON-RPC types ----

#[derive(Debug, Serialize)]
struct McpRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct McpResponse {
    result: Option<serde_json::Value>,
    error: Option<McpError>,
}

#[derive(Debug, Deserialize)]
struct McpError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct McpToolDef {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct McpListToolsResult {
    tools: Vec<McpToolDef>,
}

#[derive(Debug, Deserialize)]
struct McpCallToolResult {
    content: Vec<McpContent>,
    #[serde(default)]
    #[serde(rename = "isError")]
    is_error: bool,
}

#[derive(Debug, Deserialize)]
struct McpContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: String,
}

// ---- MCP server config ----
// Re-exported from domain config
use crate::domain::config::McpServerConfig;

// ---- MCP provider ----

/// A ToolProvider backed by an MCP server process connected via stdio.
///
/// `Clone` shares the same child process / stdio handles so the provider can be
/// registered on both the classic Runtime and the Execution runtime.
#[derive(Clone)]
pub struct McpProvider {
    id: String,
    name: String,
    tools: Vec<ToolDef>,
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
    /// Connect to an MCP server and discover its tools.
    pub async fn connect(config: &McpServerConfig) -> Result<Self, String> {
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
            child: Arc::new(Mutex::new(Some(child))),
            next_id: Arc::new(Mutex::new(1)),
            stdin: Arc::new(Mutex::new(Some(stdin))),
            stdout: Arc::new(Mutex::new(Some(reader))),
        };

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

        // Discover tools
        let tools = provider.discover().await?;

        Ok(McpProvider {
            id: config.name.clone(),
            name: config.name.clone(),
            tools,
            child: Arc::clone(&provider.child),
            next_id: Arc::clone(&provider.next_id),
            stdin: Arc::clone(&provider.stdin),
            stdout: Arc::clone(&provider.stdout),
        })
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
            .map(|mcp_tool| ToolDef {
                name: mcp_tool.name,
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

// ---- Initialization helper ----

/// Connect to all configured MCP servers and register their tools on the
/// Execution runtime.
pub async fn initialize_mcp_tools(
    configs: &[McpServerConfig],
    runtime: &orchd::AgentRuntime,
) -> Vec<String> {
    let mut registered = Vec::new();

    for config in configs {
        match McpProvider::connect(config).await {
            Ok(provider) => {
                let name = config.name.clone();
                let tool_count = provider.tools.len();

                use piko_protocol::tools::{ToolSet, ToolSetMetadata, ToolSetToolRef};
                let tool_set = ToolSet {
                    id: format!("mcp_{name}"),
                    name: format!("mcp/{name}"),
                    description: Some(format!("MCP tools from {name} server")),
                    tools: vec![ToolSetToolRef::ProviderNamespace {
                        provider_id: name.clone(),
                        namespace: String::new(),
                        alias: None,
                        policy: None,
                    }],
                    policy: None,
                    metadata: Some(ToolSetMetadata {
                        source: Some("mcp".into()),
                        tags: None,
                    }),
                };

                runtime.register_tool_provider(Box::new(provider)).await;
                runtime.register_tool_set(tool_set).await;

                registered.push(format!("{name} ({tool_count} tools)"));
            }
            Err(e) => {
                tracing::warn!("Failed to connect to MCP server {}: {e}", config.name);
            }
        }
    }

    registered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_deserialize() {
        let config: McpServerConfig = serde_json::from_str(
            r#"{"name": "filesystem", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"], "env": {}}"#,
        )
        .unwrap();
        assert_eq!(config.name, "filesystem");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args.len(), 3);
    }

    #[test]
    fn test_mcp_server_config_defaults() {
        let config: McpServerConfig =
            serde_json::from_str(r#"{"name": "echo", "command": "cat"}"#).unwrap();
        assert_eq!(config.args, Vec::<String>::new());
        assert_eq!(config.env, HashMap::new());
    }
}
