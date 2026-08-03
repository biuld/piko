//! Built-in `mcp_resource` tool backed by connected MCP providers.
//!
//! One provider (`mcp-host`) owns every successfully connected server and
//! routes list/search/read calls by the `server` argument, so resource
//! access works across all configured servers without one tool per server.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};
use piko_protocol::tools::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolExposure, ToolMetadata, ToolProviderSource,
};

use super::provider::{McpProvider, ResourceReadError};

/// Provider id / executor target for the built-in resource tool.
pub(crate) const MCP_RESOURCE_PROVIDER_ID: &str = "mcp-host";
/// Public tool name for MCP resource access.
pub(crate) const MCP_RESOURCE_TOOL_NAME: &str = "mcp_resource";

pub(crate) fn mcp_resource_tool_def() -> ToolDef {
    let version = piko_orchd_api::stable_internal_id(
        "mcp-tool",
        &[MCP_RESOURCE_PROVIDER_ID, MCP_RESOURCE_TOOL_NAME, "1"],
    );
    ToolDef {
        name: MCP_RESOURCE_TOOL_NAME.to_string(),
        version: version.clone(),
        provenance: piko_protocol::PromptSource::new("mcp-host", MCP_RESOURCE_TOOL_NAME)
            .with_version(version),
        description: "List or read resources exposed by a connected MCP server. \
            Pass `server` plus an optional `query` to list/search the server's \
            resources; pass `server` plus `uri` to read a resource's text content."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Connected MCP server name from settings"
                },
                "query": {
                    "type": "string",
                    "description": "Optional substring filter over uri/name/description when listing"
                },
                "uri": {
                    "type": "string",
                    "description": "Resource URI to read; when present, `query` is ignored"
                }
            },
            "required": ["server"]
        }),
        executor: ToolExecutorRef {
            kind: "mcp".into(),
            target: MCP_RESOURCE_PROVIDER_ID.into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Sequential),
        exposure: Some(ToolExposure::Direct),
        capabilities: Some(vec![ToolCapability::Network]),
        approval: Some(ToolApprovalRequirement::Never),
        metadata: Some(ToolMetadata {
            title: None,
            read_only: Some(true),
            destructive: Some(false),
            mutates_workspace: Some(false),
            produces_artifact: Some(false),
        }),
    }
}

/// Routes `mcp_resource` calls to the connected MCP server named in the
/// arguments.
pub struct McpResourceProvider {
    servers: HashMap<String, Arc<McpProvider>>,
}

impl McpResourceProvider {
    pub(crate) fn new(servers: HashMap<String, Arc<McpProvider>>) -> Self {
        Self { servers }
    }
}

#[async_trait]
impl ToolProvider for McpResourceProvider {
    fn id(&self) -> &str {
        MCP_RESOURCE_PROVIDER_ID
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Mcp
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![mcp_resource_tool_def()]
    }

    async fn execute(
        &self,
        call: piko_protocol::ToolCall,
        _context: ToolExecutionContext,
    ) -> ToolExecResult {
        let server = call
            .arguments
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let Some(provider) = self.servers.get(&server) else {
            return ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "mcp_resource_server_unknown".into(),
                    message: format!("unknown MCP server '{server}'"),
                    retryable: Some(false),
                }),
            };
        };

        let uri = call.arguments.get("uri").and_then(|v| v.as_str());
        let query = call.arguments.get("query").and_then(|v| v.as_str());
        let result = if let Some(uri) = uri {
            provider
                .read_resource(uri)
                .await
                .map_err(|error| match error {
                    ResourceReadError::NotFound(message) => ToolExecError {
                        code: "mcp_resource_not_found".into(),
                        message,
                        retryable: Some(false),
                    },
                    ResourceReadError::BlobUnsupported(message) => ToolExecError {
                        code: "mcp_resource_blob_unsupported".into(),
                        message,
                        retryable: Some(false),
                    },
                    ResourceReadError::Transport(message) => ToolExecError {
                        code: "mcp_error".into(),
                        message,
                        retryable: Some(true),
                    },
                })
        } else {
            provider
                .list_resources(query)
                .await
                .map_err(|message| ToolExecError {
                    code: "mcp_error".into(),
                    message,
                    retryable: Some(true),
                })
        };

        match result {
            Ok(value) => ToolExecResult {
                ok: true,
                value: Some(value),
                error: None,
            },
            Err(error) => ToolExecResult {
                ok: false,
                value: None,
                error: Some(error),
            },
        }
    }
}
