// ---- Protocol: messages — core message types ----
// These mirror the pi-ai message types for Rust.

use serde::{Deserialize, Serialize};

// ---- Content block (the only one — ToolCall extracted to ToolCall) ----

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
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// A parsed tool call — the standalone type for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_json: Option<String>,
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
    /// Data-only context injected by a trusted runtime component. It has no
    /// User instruction authority even when a provider must render it using a
    /// user-role transport message.
    #[serde(rename = "context")]
    Context {
        content: MessageContent,
        trust: crate::ContentTrust,
        source: crate::PromptSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "user")]
    User {
        content: MessageContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        content: Vec<ContentBlock>,
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

// ---- Helpers ----

impl Message {
    pub fn role(&self) -> &str {
        match self {
            Message::Context { .. } => "context",
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

    /// Add another usage record into this one (token counts and cost).
    pub fn accumulate(&mut self, other: &Usage) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.total_tokens = self.total_tokens.saturating_add(other.total_tokens);
        self.cost.input += other.cost.input;
        self.cost.output += other.cost.output;
        self.cost.cache_read += other.cost.cache_read;
        self.cost.cache_write += other.cost.cache_write;
        self.cost.total += other.cost.total;
    }

    /// Prompt-side fill used for context chrome (`used` in F-22 usage projection).
    pub fn context_fill(&self) -> u64 {
        self.input.saturating_add(self.cache_read)
    }
}

#[cfg(test)]
mod usage_tests {
    use super::{Usage, UsageCost};

    #[test]
    fn accumulate_sums_tokens_and_cost() {
        let mut total = Usage {
            input: 10,
            output: 5,
            cache_read: 1,
            cache_write: 0,
            total_tokens: 16,
            cost: UsageCost {
                input: 0.1,
                output: 0.2,
                cache_read: 0.0,
                cache_write: 0.0,
                total: 0.3,
            },
        };
        total.accumulate(&Usage {
            input: 3,
            output: 7,
            cache_read: 0,
            cache_write: 2,
            total_tokens: 12,
            cost: UsageCost {
                input: 0.03,
                output: 0.07,
                cache_read: 0.0,
                cache_write: 0.01,
                total: 0.11,
            },
        });
        assert_eq!(total.input, 13);
        assert_eq!(total.output, 12);
        assert_eq!(total.cache_read, 1);
        assert_eq!(total.cache_write, 2);
        assert_eq!(total.total_tokens, 28);
        assert!((total.cost.total - 0.41).abs() < f64::EPSILON);
    }
}

/// Durable, model-visible marker appended when a turn is interrupted (F-01).
///
/// Context keeps `authority=None` so the marker is data, not instruction or
/// fabricated model output; the gateway renders it to the next run's prompt.
pub fn turn_abort_marker(execution_id: &str) -> Message {
    Message::Context {
        content: MessageContent::String(
            "The previous turn was interrupted on purpose. Any tools or commands that were aborted may have partially executed."
                .into(),
        ),
        trust: crate::ContentTrust::Trusted,
        source: crate::PromptSource::new("turn_aborted", execution_id),
        timestamp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        ),
    }
}

/// Stable message id for a turn's abort marker. Live cancellation and crash
/// recovery share it so re-applying the marker is idempotent.
pub fn turn_abort_marker_message_id(execution_id: &str) -> String {
    format!("{execution_id}/abort_marker")
}

/// Stable message id for a run's world-state Context message (F-04 slice 2).
/// Committed before the run input so the durable transcript chain stays
/// linear: head → world-state → input.
pub fn world_state_message_id(execution_id: &str) -> String {
    format!("{execution_id}/world_state")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_block_serde_round_trip() {
        let block = ContentBlock::Thinking {
            thinking: "considering".into(),
            thinking_signature: Some("sig".into()),
        };
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn tool_call_data_serde_round_trip() {
        let tc = ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "Cargo.toml"}),
            partial_json: None,
        };
        let json = serde_json::to_string(&tc).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, parsed);
    }
}
