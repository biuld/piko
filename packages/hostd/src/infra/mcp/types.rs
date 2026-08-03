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

/// `resources/list` result (first page; `nextCursor` is ignored for now).
#[derive(Debug, Deserialize)]
pub(super) struct McpListResourcesResult {
    #[serde(default)]
    pub(super) resources: Vec<McpResource>,
}

/// `resources/templates/list` result (first page; `nextCursor` ignored).
#[derive(Debug, Deserialize)]
pub(super) struct McpListResourceTemplatesResult {
    #[serde(default)]
    #[serde(rename = "resourceTemplates")]
    pub(super) resource_templates: Vec<McpResourceTemplate>,
}

/// A normalized MCP resource (`resources/list` item).
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct McpResource {
    pub(super) uri: String,
    pub(super) name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) description: String,
    #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub(super) mime_type: Option<String>,
}

impl<'de> serde::Deserialize<'de> for McpResource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            uri: String,
            name: String,
            #[serde(default)]
            description: String,
            #[serde(rename = "mimeType", default)]
            mime_type: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(McpResource {
            uri: raw.uri,
            name: raw.name,
            description: raw.description,
            mime_type: raw.mime_type,
        })
    }
}

/// A normalized MCP resource template (`resources/templates/list` item).
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct McpResourceTemplate {
    #[serde(rename = "uriTemplate")]
    pub(super) uri_template: String,
    pub(super) name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) description: String,
    #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub(super) mime_type: Option<String>,
}

impl<'de> serde::Deserialize<'de> for McpResourceTemplate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            #[serde(rename = "uriTemplate")]
            uri_template: String,
            name: String,
            #[serde(default)]
            description: String,
            #[serde(rename = "mimeType", default)]
            mime_type: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(McpResourceTemplate {
            uri_template: raw.uri_template,
            name: raw.name,
            description: raw.description,
            mime_type: raw.mime_type,
        })
    }
}

/// `resources/read` result.
#[derive(Debug, Deserialize)]
pub(super) struct McpReadResourceResult {
    pub(super) contents: Vec<McpResourceContents>,
}

#[derive(Debug, Deserialize)]
pub(super) struct McpResourceContents {
    pub(super) uri: String,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) blob: Option<String>,
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
