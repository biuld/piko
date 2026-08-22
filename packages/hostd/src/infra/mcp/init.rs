use crate::domain::config::McpServerConfig;

use super::provider::McpProvider;
use super::resource::{MCP_RESOURCE_PROVIDER_ID, MCP_RESOURCE_TOOL_NAME, McpResourceProvider};

// ---- Initialization helper ----

/// Connect to all configured MCP servers and register their tools on the
/// Execution runtime. Each server is connected under `timeout` (F-13
/// prewarm); a failed or timed-out server is skipped with a warning and the
/// others register normally. When at least one server connected, the
/// built-in `mcp_resource` tool is registered too.
///
/// Returns one status entry per configured server (`connected` + counts for
/// live providers, the connect error otherwise) for the `mcp.status`
/// client surface.
pub async fn initialize_mcp_tools(
    configs: &[McpServerConfig],
    timeout: std::time::Duration,
    runtime: &piko_orchd::AgentRuntime,
) -> Vec<piko_protocol::command::McpServerInfo> {
    let mut statuses = Vec::new();
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
                    feature: Some(piko_protocol::tools::ToolSetFeature::Family {
                        key: "mcp".into(),
                    }),
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

                if let Err(error) = runtime
                    .install_tool_contribution(piko_orchd::tools::ToolContribution {
                        provider: Box::new(provider.clone()),
                        tool_sets: vec![tool_set],
                    })
                    .await
                {
                    tracing::warn!("Failed to register MCP server {name}: {error}");
                    statuses.push(piko_protocol::command::McpServerInfo {
                        name,
                        connected: false,
                        tool_count: 0,
                        resource_count: 0,
                        template_count: 0,
                        error: Some(error),
                    });
                    continue;
                }

                providers.insert(name.clone(), std::sync::Arc::new(provider));
                statuses.push(piko_protocol::command::McpServerInfo {
                    name,
                    connected: true,
                    tool_count,
                    resource_count,
                    template_count,
                    error: None,
                });
            }
            Err(e) => {
                tracing::warn!("Failed to connect to MCP server {}: {e}", config.name);
                statuses.push(piko_protocol::command::McpServerInfo {
                    name: config.name.clone(),
                    connected: false,
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                    error: Some(e),
                });
            }
        }
    }

    if !providers.is_empty() {
        use piko_protocol::tools::{ToolSet, ToolSetMetadata, ToolSetToolRef};
        let resource_tool_set = ToolSet {
            id: "mcp-resources".into(),
            name: "mcp/resources".into(),
            description: Some("MCP resource access across connected servers".into()),
            feature: Some(piko_protocol::tools::ToolSetFeature::Family { key: "mcp".into() }),
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
        if let Err(error) = runtime
            .install_tool_contribution(piko_orchd::tools::ToolContribution {
                provider: Box::new(McpResourceProvider::new(providers)),
                tool_sets: vec![resource_tool_set],
            })
            .await
        {
            tracing::warn!("Failed to register MCP resource tools: {error}");
        }
    }

    statuses
}
