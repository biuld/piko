use crate::domain::config::McpServerConfig;

use super::provider::McpProvider;

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
