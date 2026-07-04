// ---- Protocol: messages — core message types ----
// These mirror the pi-ai message types for Rust.

use serde::{Deserialize, Serialize};

// ---- Content blocks ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_signature: Option<String>,
    },
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        #[serde(alias = "args")]
        arguments: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_json: Option<String>,
    },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AssistantContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_signature: Option<String>,
    },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl From<AssistantContentBlock> for ContentBlock {
    fn from(block: AssistantContentBlock) -> Self {
        match block {
            AssistantContentBlock::Text { text } => Self::Text { text },
            AssistantContentBlock::Thinking {
                thinking,
                thinking_signature,
            } => Self::Thinking {
                thinking,
                thinking_signature,
            },
            AssistantContentBlock::Image { data, mime_type } => Self::Image { data, mime_type },
        }
    }
}

impl TryFrom<ContentBlock> for AssistantContentBlock {
    type Error = ContentBlock;

    fn try_from(block: ContentBlock) -> Result<Self, Self::Error> {
        match block {
            ContentBlock::Text { text } => Ok(Self::Text { text }),
            ContentBlock::Thinking {
                thinking,
                thinking_signature,
            } => Ok(Self::Thinking {
                thinking,
                thinking_signature,
            }),
            ContentBlock::Image { data, mime_type } => Ok(Self::Image { data, mime_type }),
            ContentBlock::ToolCall { .. } => Err(block),
        }
    }
}

// ---- Usage / cost ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost: UsageCost,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

// ---- Model ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

// ---- Message enum ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum Message {
    #[serde(rename = "user")]
    User {
        content: MessageContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        content: Vec<AssistantContentBlock>,
        api: String,
        provider: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "toolCall")]
    #[serde(alias = "tool_call")]
    ToolCall {
        id: String,
        name: String,
        #[serde(alias = "args")]
        arguments: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "toolResult")]
    #[serde(alias = "tool_result")]
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        content: Vec<ContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MessageContent {
    String(String),
    Blocks(Vec<ContentBlock>),
}

// ---- Type aliases for pi-ai compat ----

pub type TextContent = ContentBlock;
pub type ThinkingContent = ContentBlock;
pub type ImageContent = ContentBlock;
pub type ToolResultMessage = Message;
pub type UserMessage = Message;
pub type AssistantMessage = Message;
pub type ToolCall = ContentBlock;

// ---- Helpers ----

impl Message {
    pub fn role(&self) -> &str {
        match self {
            Message::User { .. } => "user",
            Message::Assistant { .. } => "assistant",
            Message::ToolCall { .. } => "toolCall",
            Message::ToolResult { .. } => "toolResult",
        }
    }
}

impl Usage {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_content_block_rejects_tool_call_blocks() {
        let block = ContentBlock::ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "Cargo.toml"}),
            partial_json: None,
        };

        assert!(AssistantContentBlock::try_from(block).is_err());
    }

    #[test]
    fn assistant_content_block_round_trips_to_general_content_block() {
        let block = AssistantContentBlock::Thinking {
            thinking: "considering".into(),
            thinking_signature: Some("sig".into()),
        };

        let general: ContentBlock = block.clone().into();

        assert_eq!(AssistantContentBlock::try_from(general), Ok(block));
    }
}
