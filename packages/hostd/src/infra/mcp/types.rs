use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(super) struct McpRequest {
    pub(super) jsonrpc: String,
    pub(super) id: u64,
    pub(super) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpResponse {
    pub(super) result: Option<serde_json::Value>,
    pub(super) error: Option<McpError>,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpError {
    pub(super) code: i32,
    pub(super) message: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpToolDef {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(rename = "inputSchema")]
    pub(super) input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpListToolsResult {
    pub(super) tools: Vec<McpToolDef>,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpCallToolResult {
    pub(super) content: Vec<McpContent>,
    #[serde(default)]
    #[serde(rename = "isError")]
    pub(super) is_error: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpContent {
    #[serde(rename = "type")]
    pub(super) content_type: String,
    #[serde(default)]
    pub(super) text: String,
}
