use std::pin::Pin;

use futures::Stream;
use piko_protocol::messages::{
    ContentBlock, Message, MessageContent, OpaqueModelCheckpoint, Usage,
};
use serde::{Deserialize, Serialize};

pub use crate::capabilities::{
    ModelCapabilities, ModelDescriptor, ModelLimits, UpstreamToolDescriptor,
};
pub use crate::collector::collect_execution;
pub use crate::execution::{
    InferenceExecution, InferenceGateway, InferenceStatus, OpaqueEventCursor, OpaqueExecutionHandle,
};
pub use crate::tools::{
    GeneratedArtifact, InferenceAuxiliary, InferenceCitation, InferenceSource, InferenceTool,
    SemanticResourceRef, UpstreamApprovalPolicy, UpstreamApprovalRequest, UpstreamToolActivity,
    UpstreamToolDefinition,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationContext {
    pub session_id: String,
    pub agent_instance_id: String,
    pub run_id: String,
    pub step_id: String,
    /// Assistant message id assigned by the orchestrator for this step's
    /// output. Links the trajectory model-step record to the committed
    /// transcript message without any positional or temporal heuristics.
    pub step_message_id: String,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub instructions: piko_protocol::SemanticRunPrompt,
    pub items: Vec<ConversationItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConversationItem {
    pub id: ConversationItemId,
    pub kind: ConversationItemKind,
    pub checkpoint: Option<OpaqueModelCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ConversationItemKind {
    Context {
        content: MessageContent,
        trust: piko_protocol::ContentTrust,
        source: piko_protocol::PromptSource,
    },
    User {
        content: MessageContent,
    },
    Assistant {
        content: Vec<ContentBlock>,
    },
    ToolCall {
        call_id: ToolCallId,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        call_id: ToolCallId,
        name: Option<String>,
        content: Vec<ContentBlock>,
        is_error: bool,
    },
    UpstreamActivity(UpstreamToolActivity),
    Source(InferenceSource),
    Citation(InferenceCitation),
    Artifact(GeneratedArtifact),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ConversationItemId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct OutputItemId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ToolCallId(pub String);

impl Conversation {
    /// Project the durable protocol transcript into llmd's semantic model.
    /// IDs are derived from the canonical prefix, so they remain stable after
    /// persistence/restoration and distinguish duplicate adjacent items.
    pub fn from_messages(
        instructions: piko_protocol::SemanticRunPrompt,
        messages: Vec<Message>,
    ) -> Self {
        use sha2::{Digest, Sha256};

        let mut prefix = Sha256::new();
        let mut items = Vec::with_capacity(messages.len());
        for message in messages {
            let (kind, checkpoint) = match message {
                Message::Context {
                    content,
                    trust,
                    source,
                    ..
                } => (
                    ConversationItemKind::Context {
                        content,
                        trust,
                        source,
                    },
                    None,
                ),
                Message::User { content, .. } => (ConversationItemKind::User { content }, None),
                Message::Assistant {
                    content,
                    checkpoint,
                    ..
                } => (
                    ConversationItemKind::Assistant { content },
                    checkpoint.map(|value| *value),
                ),
                Message::ToolCall {
                    id,
                    name,
                    arguments,
                    ..
                } => (
                    ConversationItemKind::ToolCall {
                        call_id: ToolCallId(id),
                        name,
                        arguments,
                    },
                    None,
                ),
                Message::ToolResult {
                    tool_call_id,
                    tool_name,
                    content,
                    is_error,
                    ..
                } => (
                    ConversationItemKind::ToolResult {
                        call_id: ToolCallId(tool_call_id),
                        name: tool_name,
                        content,
                        is_error: is_error.unwrap_or(false),
                    },
                    None,
                ),
            };
            let canonical = serde_json::to_vec(&kind).unwrap_or_default();
            prefix.update((canonical.len() as u64).to_be_bytes());
            prefix.update(&canonical);
            let id = ConversationItemId(format!("ci_{}", hex_digest(prefix.clone().finalize())));
            items.push(ConversationItem {
                id,
                kind,
                checkpoint,
            });
        }
        Self {
            instructions,
            items,
        }
    }
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    Streaming,
    Assembled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Specific(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredOutputIntent {
    pub schema: serde_json::Value,
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceOptions {
    pub reasoning_effort: Option<piko_protocol::model::ThinkingLevel>,
    pub delivery: DeliveryMode,
    pub tool_choice: ToolChoice,
    pub parallel_tools: Option<bool>,
    pub max_output_tokens: Option<u32>,
    pub structured_output: Option<StructuredOutputIntent>,
    /// Permit and include upstream tools configured for the resolved target.
    /// The target catalog, not a process-wide host setting, owns the set.
    pub allow_upstream_tools: bool,
}

impl Default for InferenceOptions {
    fn default() -> Self {
        Self {
            reasoning_effort: None,
            delivery: DeliveryMode::Streaming,
            tool_choice: ToolChoice::Auto,
            parallel_tools: None,
            max_output_tokens: None,
            structured_output: None,
            allow_upstream_tools: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub model: ModelRef,
    pub conversation: Conversation,
    pub tools: Vec<InferenceTool>,
    pub options: InferenceOptions,
    pub context: InvocationContext,
}

impl InferenceRequest {
    pub fn text_task(
        model: ModelRef,
        system_prompt: impl Into<String>,
        messages: Vec<Message>,
        context: InvocationContext,
    ) -> Self {
        let mut instructions = piko_protocol::SemanticRunPrompt::default();
        instructions.blocks.push(piko_protocol::PromptBlock {
            id: "inference.text_task".into(),
            kind: piko_protocol::PromptBlockKind::Instruction,
            authority: piko_protocol::InstructionAuthority::Platform,
            trust: piko_protocol::ContentTrust::Trusted,
            source: piko_protocol::PromptSource::new("llmd", "text_task"),
            content: system_prompt.into(),
            content_digest: String::new(),
            cache_scope: piko_protocol::CacheScope::NoCache,
        });
        Self {
            model,
            conversation: Conversation::from_messages(instructions, messages),
            tools: Vec::new(),
            options: InferenceOptions {
                delivery: DeliveryMode::Assembled,
                ..Default::default()
            },
            context,
        }
    }
}

pub type InferenceEventStream = Pin<Box<dyn Stream<Item = InferenceEvent> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    Completed { reason: String },
    Incomplete { reason: Option<String> },
    Failed { message: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InferenceItem {
    Text {
        text: String,
        id: OutputItemId,
    },
    Refusal {
        text: String,
        id: OutputItemId,
    },
    Reasoning {
        text: String,
        id: OutputItemId,
    },
    ToolCall {
        name: String,
        arguments: String,
        call_id: ToolCallId,
    },
    ToolResult {
        output: String,
        call_id: ToolCallId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct InferenceResult {
    pub items: Vec<InferenceItem>,
    pub usage: Option<Usage>,
    pub finish_reason: FinishReason,
    pub checkpoint: Option<OpaqueModelCheckpoint>,
    pub auxiliary: Vec<InferenceAuxiliary>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InferenceEvent {
    Cursor(OpaqueEventCursor),
    TextDelta {
        item_id: OutputItemId,
        delta: String,
    },
    RefusalDelta {
        item_id: OutputItemId,
        delta: String,
    },
    ReasoningDelta {
        item_id: OutputItemId,
        delta: String,
    },
    ToolCallDelta {
        call_id: ToolCallId,
        name: String,
        arguments_delta: String,
    },
    Usage(Usage),
    Checkpoint(OpaqueModelCheckpoint),
    UpstreamActivity(UpstreamToolActivity),
    ApprovalRequired(UpstreamApprovalRequest),
    Source(InferenceSource),
    Citation(InferenceCitation),
    Artifact(GeneratedArtifact),
    Completed(FinishReason),
    Error(InferenceError),
}

impl InferenceEvent {
    pub fn text(delta: impl Into<String>) -> Self {
        Self::TextDelta {
            item_id: OutputItemId("output_text".into()),
            delta: delta.into(),
        }
    }

    pub fn reasoning(delta: impl Into<String>) -> Self {
        Self::ReasoningDelta {
            item_id: OutputItemId("output_reasoning".into()),
            delta: delta.into(),
        }
    }

    pub fn function_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_delta: impl Into<String>,
    ) -> Self {
        Self::ToolCallDelta {
            call_id: ToolCallId(id.into()),
            name: name.into(),
            arguments_delta: arguments_delta.into(),
        }
    }

    pub fn completed(reason: impl Into<String>) -> Self {
        Self::Completed(FinishReason::Completed {
            reason: reason.into(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Target,
    UnsupportedCapability,
    CheckpointRejected,
    ContinuationUnavailable,
    Authentication,
    Transport,
    Timeout,
    RateLimit,
    Upstream,
    Sse,
    Protocol,
    Cancelled,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{target} {operation}: {message}")]
pub struct InferenceError {
    pub class: ErrorClass,
    pub target: String,
    pub operation: &'static str,
    pub message: String,
    pub status: Option<u16>,
    pub request_id: Option<String>,
    pub retry_after_ms: Option<u64>,
}

impl InferenceError {
    pub fn new(
        class: ErrorClass,
        target: impl Into<String>,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class,
            target: target.into(),
            operation,
            message: message.into(),
            status: None,
            request_id: None,
            retry_after_ms: None,
        }
    }

    pub fn cancelled(target: impl Into<String>) -> Self {
        Self::new(ErrorClass::Cancelled, target, "start", "cancelled")
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self.class,
            ErrorClass::Transport | ErrorClass::Timeout | ErrorClass::RateLimit
        ) || self
            .status
            .is_some_and(|status| matches!(status, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504))
    }
}
