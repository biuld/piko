// ---- Protocol: messages — core message types ----
// These mirror the pi-ai message types for Rust.

use serde::{Deserialize, Serialize};

mod cost;
pub use cost::{UsageCost, UsageCostBasis, UsageCostEntry};

// ---- Content block (the only one — ToolCall extracted to ToolCall) ----

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamActivityStatus {
    Started,
    InProgress,
    Completed,
    Failed,
}

impl UpstreamActivityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Typed provider-side upstream action for a known tool (e.g. a web search).
/// Decoded at the llmd boundary from the provider-echoed `action` value, with
/// provider-internal markers stripped, so consumers read fields rather than
/// JSON-parsing the opaque `arguments` echo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpstreamAction {
    /// Provider selected a `search` action. `queries` is the cleaned,
    /// user-visible list (provider-internal `ws_call_id=` markers removed).
    Search { queries: Vec<String> },
    /// Provider selected an `open_page` action to fetch a URL. The trailing
    /// `#ws_call_id=` fragment is stripped.
    OpenPage { url: String },
}

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
        /// Monotonic model-runtime duration for this ordered thinking run.
        /// Legacy transcript blocks omit the field and decode as `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    UpstreamToolActivity {
        activity_id: String,
        tool_name: String,
        /// Open provider-neutral catalog identifier such as `search`.
        kind: String,
        status: UpstreamActivityStatus,
        /// Provider-echoed call arguments (e.g. `{ "type": "search", "query": … }`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arguments: Option<serde_json::Value>,
        /// Typed, cleaned view of the activity action for known upstream tools.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<UpstreamAction>,
    },
    UpstreamToolApproval {
        approval_id: String,
        tool_name: String,
        summary: String,
    },
    Source {
        source_id: String,
        title: Option<String>,
        uri: Option<String>,
    },
    Citation {
        source_id: String,
        output_item_id: String,
        start: Option<u32>,
        end: Option<u32>,
    },
    Artifact {
        artifact_id: String,
        media_type: String,
        namespace: String,
        resource: String,
    },
}

impl ContentBlock {
    pub fn text_projection(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Thinking { thinking, .. } => format!("[thinking] {thinking}"),
            Self::Image { mime_type, .. } => format!("[image: {mime_type}]"),
            Self::UpstreamToolActivity {
                tool_name, status, ..
            } => format!("[upstream tool: {tool_name} ({})]", status.as_str()),
            Self::UpstreamToolApproval {
                tool_name, summary, ..
            } => format!("[upstream tool approval required: {tool_name}: {summary}]"),
            Self::Source { title, uri, .. } => format!(
                "[source: {}]",
                title.as_deref().or(uri.as_deref()).unwrap_or("untitled")
            ),
            Self::Citation { source_id, .. } => format!("[citation: {source_id}]"),
            Self::Artifact {
                artifact_id,
                media_type,
                ..
            } => format!("[artifact: {artifact_id} ({media_type})]"),
        }
    }
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

/// Opaque llmd checkpoint retained with a completed assistant model step.
///
/// The serialized token is deliberately private. Protocol consumers can
/// clone, compare, persist, and restore this value, but cannot branch on or
/// rewrite llmd's checkpoint representation.
#[derive(Clone, PartialEq, Eq)]
pub struct OpaqueModelCheckpoint {
    token: String,
}

const MAX_OPAQUE_CHECKPOINT_TOKEN_BYTES: usize = 128 * 1024;

impl Serialize for OpaqueModelCheckpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.token)
    }
}

impl<'de> Deserialize<'de> for OpaqueModelCheckpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        if token.len() > MAX_OPAQUE_CHECKPOINT_TOKEN_BYTES {
            return Err(serde::de::Error::custom(
                "opaque checkpoint exceeds size limit",
            ));
        }
        Ok(Self { token })
    }
}

impl std::fmt::Debug for OpaqueModelCheckpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpaqueModelCheckpoint")
            .field("token", &"[REDACTED]")
            .finish()
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
    /// Protocol-normalized billable quantities not represented by the common
    /// token counters. Names are stable llmd contracts, never raw wire keys.
    pub units: std::collections::BTreeMap<String, f64>,
    pub cost: UsageCost,
}

// ---- Model ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checkpoint: Option<Box<OpaqueModelCheckpoint>>,
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
        for (name, quantity) in &other.units {
            *self.units.entry(name.clone()).or_default() += quantity;
        }
        self.cost.accumulate(&other.cost);
    }

    /// Prompt-side fill used for context chrome (`used` in F-22 usage projection).
    pub fn context_fill(&self) -> u64 {
        self.input.saturating_add(self.cache_read)
    }
}

#[cfg(test)]
mod usage_tests {
    use super::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

    fn usd(total: f64) -> UsageCost {
        UsageCost {
            entries: vec![UsageCostEntry {
                currency: "USD".into(),
                basis: UsageCostBasis::ListPrice,
                components: [("input_tokens".into(), total)].into(),
                total,
            }],
        }
    }

    #[test]
    fn accumulate_sums_tokens_and_cost() {
        let mut total = Usage {
            input: 10,
            output: 5,
            cache_read: 1,
            cache_write: 0,
            total_tokens: 16,
            units: [("search_call".into(), 1.0)].into(),
            cost: usd(0.3),
        };
        total.accumulate(&Usage {
            input: 3,
            output: 7,
            cache_read: 0,
            cache_write: 2,
            total_tokens: 12,
            units: [("search_call".into(), 2.0)].into(),
            cost: usd(0.11),
        });
        assert_eq!(total.input, 13);
        assert_eq!(total.output, 12);
        assert_eq!(total.cache_read, 1);
        assert_eq!(total.cache_write, 2);
        assert_eq!(total.total_tokens, 28);
        assert_eq!(total.units["search_call"], 3.0);
        assert!((total.cost.entries[0].total - 0.41).abs() < f64::EPSILON);

        total.accumulate(&Usage {
            cost: UsageCost {
                entries: vec![UsageCostEntry {
                    currency: "CNY".into(),
                    basis: UsageCostBasis::ListPrice,
                    components: [("input_tokens".into(), 1.0)].into(),
                    total: 1.0,
                }],
            },
            ..Default::default()
        });
        assert_eq!(total.cost.entries.len(), 2);
    }

    #[test]
    fn prior_usage_and_fixed_cost_shapes_are_not_accepted() {
        let missing_units = serde_json::json!({
            "input": 1,
            "output": 1,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": 2,
            "cost": { "entries": [] }
        });
        assert!(serde_json::from_value::<Usage>(missing_units).is_err());

        let fixed_components = serde_json::json!({
            "input": 1,
            "output": 1,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": 2,
            "units": {},
            "cost": { "entries": [{
                "currency": "USD",
                "basis": "list_price",
                "input": 0.1,
                "output": 0.2,
                "cacheRead": 0.0,
                "cacheWrite": 0.0,
                "total": 0.3
            }] }
        });
        assert!(serde_json::from_value::<Usage>(fixed_components).is_err());
    }
}

/// Durable, model-visible marker appended when a turn is interrupted (F-01).
///
/// Context keeps `authority=None` so the marker is data, not instruction or
/// fabricated model output; the gateway renders it to the next run's prompt.
pub fn turn_abort_marker(root_input_id: &str) -> Message {
    Message::Context {
        content: MessageContent::String(
            "The previous turn was interrupted on purpose. Any tools or commands that were aborted may have partially executed."
                .into(),
        ),
        trust: crate::ContentTrust::Trusted,
        source: crate::PromptSource::new("turn_aborted", root_input_id),
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
pub fn turn_abort_marker_message_id(root_input_id: &str) -> String {
    format!("{root_input_id}/abort_marker")
}

/// Stable message id for a run's world-state Context message (F-04 slice 2).
/// Committed before the run input so the durable transcript chain stays
/// linear: head → world-state → input.
pub fn world_state_message_id(root_input_id: &str) -> String {
    format!("{root_input_id}/world_state")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_block_serde_round_trip() {
        let block = ContentBlock::Thinking {
            thinking: "considering".into(),
            thinking_signature: Some("sig".into()),
            duration_ms: Some(2400),
        };
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn legacy_thinking_block_defaults_duration_to_none() {
        let block: ContentBlock = serde_json::from_value(serde_json::json!({
            "type": "thinking",
            "thinking": "legacy",
        }))
        .unwrap();
        assert!(matches!(
            block,
            ContentBlock::Thinking {
                duration_ms: None,
                ..
            }
        ));
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

    #[test]
    fn assistant_checkpoint_round_trips_as_an_opaque_token() {
        let checkpoint: OpaqueModelCheckpoint =
            serde_json::from_value(serde_json::json!("opaque.checkpoint.token")).unwrap();
        let message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "done".into(),
            }],
            checkpoint: Some(Box::new(checkpoint.clone())),
            provider: "fixture".into(),
            model: "model".into(),
            usage: None,
            stop_reason: Some("stop".into()),
            error_message: None,
            timestamp: None,
        };
        let restored: Message =
            serde_json::from_str(&serde_json::to_string(&message).unwrap()).unwrap();
        assert_eq!(restored, message);
        assert!(!format!("{checkpoint:?}").contains("opaque.checkpoint.token"));
    }

    #[test]
    fn oversized_checkpoint_carrier_is_rejected_during_restore() {
        let encoded =
            serde_json::to_string(&"x".repeat(MAX_OPAQUE_CHECKPOINT_TOKEN_BYTES + 1)).unwrap();
        assert!(serde_json::from_str::<OpaqueModelCheckpoint>(&encoded).is_err());
    }
}
