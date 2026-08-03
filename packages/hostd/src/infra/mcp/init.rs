use crate::domain::config::McpServerConfig;

use super::provider::McpProvider;
use super::resource::{MCP_RESOURCE_PROVIDER_ID, MCP_RESOURCE_TOOL_NAME, McpResourceProvider};

// ---- Initialization helper ----

/// Connect to all configured MCP servers and register their tools on the
/// Execution runtime. Each server is connected under `timeout` (F-13
/// prewarm); a failed or timed-out server is skipped with a warning and the
/// others register normally. When at least one server connected, the
/// built-in `mcp_resource` tool is registered too.
pub async fn initialize_mcp_tools(
    configs: &[McpServerConfig],
    timeout: std::time::Duration,
    runtime: &piko_orchd::AgentRuntime,
) -> Vec<String> {
    let mut registered = Vec::new();
    let mut providers = std::collections::HashMap::new();

    for config in configs {
        match McpProvider::connect(config, timeout).await {
            Ok(provider) => {
                let name = config.name.clone();
                let tool_count = provider.tools.len();
                let resource_count = provider.resources.len();
                let template_count = provider.templates.len();

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

                runtime
                    .register_tool_provider(Box::new(provider.clone()))
                    .await;
                runtime.register_tool_set(tool_set).await;

                providers.insert(name.clone(), std::sync::Arc::new(provider));
                registered.push(format!(
                    "{name} ({tool_count} tools, {resource_count} resources, {template_count} templates)"
                ));
            }
            Err(e) => {
                tracing::warn!("Failed to connect to MCP server {}: {e}", config.name);
            }
        }
    }

    if !providers.is_empty() {
        use piko_protocol::tools::{ToolSet, ToolSetMetadata, ToolSetToolRef};
        let resource_tool_set = ToolSet {
            id: "mcp-resources".into(),
            name: "mcp/resources".into(),
            description: Some("MCP resource access across connected servers".into()),
            tools: vec![ToolSetToolRef::ProviderTool {
                provider_id: MCP_RESOURCE_PROVIDER_ID.into(),
                tool_name: MCP_RESOURCE_TOOL_NAME.into(),
                alias: None,
                policy: None,
            }],
            policy: None,
            metadata: Some(ToolSetMetadata {
                source: Some("mcp".into()),
                tags: None,
            }),
        };
        runtime
            .register_tool_provider(Box::new(McpResourceProvider::new(providers)))
            .await;
        runtime.register_tool_set(resource_tool_set).await;
    }

    registered
}
