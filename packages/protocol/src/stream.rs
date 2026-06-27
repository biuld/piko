// ---- Protocol: event_stream — async push-based event stream ----
//
// In Rust, we use native `Stream` trait from `futures-core` instead of a custom EventStream class.
// The TS `EventStream<T, R>` pattern (stream of T events + final R result) is expressed as:
//   - `Pin<Box<dyn Stream<Item = T> + Send>>` for the stream part
//   - A `tokio::sync::oneshot` channel for the final result
//
// This module provides convenience wrappers.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::sync::oneshot;

/// A stream of events of type T with a deferred final result of type R.
pub struct EventStream<T, R> {
    inner: Pin<Box<dyn Stream<Item = T> + Send>>,
    result_rx: oneshot::Receiver<R>,
}

impl<T, R> EventStream<T, R> {
    /// Create a new EventStream from a raw stream and a oneshot receiver for the final result.
    pub fn new(
        inner: Pin<Box<dyn Stream<Item = T> + Send>>,
        result_rx: oneshot::Receiver<R>,
    ) -> Self {
        Self { inner, result_rx }
    }

    /// Await the final result. Consumes the stream.
    pub async fn result(self) -> Result<R, oneshot::error::RecvError> {
        self.result_rx.await
    }
}

impl<T, R> Stream for EventStream<T, R> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Create a push-based event stream (like TS EventStream) using a channel.
///
/// Returns (sender, EventStream) tuple.
/// Call `sender.send(event)` to push events, `sender.finalize(result)` to end with result.
pub fn create_event_stream<T: Send + 'static, R: Send + 'static>(
    buffer: usize,
) -> (EventStreamSender<T, R>, EventStream<T, R>) {
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(buffer);
    let (result_tx, result_rx) = oneshot::channel();

    let sender = EventStreamSender {
        event_tx,
        result_tx: Some(result_tx),
    };

    let stream = EventStream {
        inner: Box::pin(tokio_stream::wrappers::ReceiverStream::new(event_rx)),
        result_rx,
    };

    (sender, stream)
}

pub struct EventStreamSender<T, R> {
    event_tx: tokio::sync::mpsc::Sender<T>,
    result_tx: Option<oneshot::Sender<R>>,
}

impl<T, R> EventStreamSender<T, R> {
    /// Push an event into the stream.
    pub async fn send(&self, event: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        self.event_tx.send(event).await
    }

    /// End the stream with a final result.
    pub fn finalize(mut self, result: R) -> Result<(), R> {
        if let Some(tx) = self.result_tx.take() {
            tx.send(result)
        } else {
            Err(result)
        }
    }
}
// ---- Protocol: runtime_stream — runtime message types and conversions ----

use serde::{Deserialize, Serialize};

use crate::messages::{ContentBlock, Message, MessageContent, Usage};

// ---- Ordering types ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOrder {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeToolOrder {
    #[serde(skip_serializing_if = "Option::is_none", rename = "parentMessageId")]
    pub parent_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "contentIndex")]
    pub content_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "toolCallIndex")]
    pub tool_call_index: Option<u32>,
}

// ---- Runtime messages ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeMessageRole {
    User,
    Assistant,
    ToolResult,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTextBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

impl RuntimeTextBlock {
    pub fn new(text: String) -> Self {
        Self {
            block_type: "text".to_string(),
            text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeThinkingBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub thinking: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingSignature")]
    pub thinking_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeToolCallBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "partialJson")]
    pub partial_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeUserContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeAssistantContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingSignature")]
        thinking_signature: Option<String>,
    },
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none", rename = "partialJson")]
        partial_json: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum RuntimeMessage {
    #[serde(rename = "user")]
    User {
        id: String,
        content: Vec<RuntimeUserContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        id: String,
        content: Vec<RuntimeAssistantContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "isStreaming")]
        is_streaming: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "stopReason")]
        stop_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "errorMessage")]
        error_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "toolResult")]
    ToolResult {
        id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "toolName")]
        tool_name: Option<String>,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none", rename = "isError")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "custom")]
    Custom {
        id: String,
        #[serde(rename = "customType")]
        custom_type: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
}

// ---- RuntimeAssistantMessageEvent ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeAssistantMessageEvent {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "text_start")]
    TextStart {
        #[serde(rename = "contentIndex")]
        content_index: u32,
    },
    #[serde(rename = "text_delta")]
    TextDelta {
        #[serde(rename = "contentIndex")]
        content_index: u32,
        delta: String,
    },
    #[serde(rename = "text_end")]
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: u32,
    },
    #[serde(rename = "thinking_start")]
    ThinkingStart {
        #[serde(rename = "contentIndex")]
        content_index: u32,
    },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        content_index: u32,
        delta: String,
    },
    #[serde(rename = "thinking_end")]
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: u32,
        #[serde(skip_serializing_if = "Option::is_none", rename = "contentSignature")]
        content_signature: Option<String>,
    },
    #[serde(rename = "toolcall_start")]
    ToolCallStart {
        #[serde(rename = "contentIndex")]
        content_index: u32,
        id: String,
        name: String,
    },
    #[serde(rename = "toolcall_delta")]
    ToolCallDelta {
        #[serde(rename = "contentIndex")]
        content_index: u32,
        delta: String,
    },
    #[serde(rename = "toolcall_end")]
    ToolCallEnd {
        #[serde(rename = "contentIndex")]
        content_index: u32,
    },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "error")]
    Error { message: String },
}

// ---- HostRuntimeEvent ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostRuntimeEvent {
    #[serde(rename = "agent_start")]
    AgentStart {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    #[serde(rename = "agent_end")]
    AgentEnd {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        status: RuntimeRunStatus,
        #[serde(skip_serializing_if = "Option::is_none", rename = "totalSteps")]
        total_steps: Option<u32>,
    },
    #[serde(rename = "turn_start")]
    TurnStart {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnIndex")]
        turn_index: u32,
    },
    #[serde(rename = "turn_end")]
    TurnEnd {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnIndex")]
        turn_index: u32,
    },
    #[serde(rename = "message_start")]
    MessageStart {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "message_update")]
    MessageUpdate {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        message: RuntimeMessage,
        #[serde(skip_serializing_if = "Option::is_none", rename = "assistantEvent")]
        assistant_event: Option<RuntimeAssistantMessageEvent>,
    },
    #[serde(rename = "message_end")]
    MessageEnd {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: serde_json::Value,
    },
    #[serde(rename = "tool_execution_update")]
    ToolExecutionUpdate {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: serde_json::Value,
        #[serde(rename = "partialResult")]
        partial_result: serde_json::Value,
    },
    #[serde(rename = "tool_execution_end")]
    ToolExecutionEnd {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: serde_json::Value,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    #[serde(rename = "queue_update")]
    QueueUpdate {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "steerCount")]
        steer_count: u32,
        #[serde(rename = "followUpCount")]
        follow_up_count: u32,
        #[serde(rename = "nextTurnCount")]
        next_turn_count: u32,
        #[serde(skip_serializing_if = "Option::is_none", rename = "steerPreview")]
        steer_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "followUpPreview")]
        follow_up_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "nextTurnPreview")]
        next_turn_preview: Option<String>,
    },
    #[serde(rename = "failure")]
    Failure {
        #[serde(flatten)]
        order: RuntimeOrder,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        aborted: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeRunStatus {
    Completed,
    #[serde(rename = "context_overflow")]
    ContextOverflow,
    Aborted,
    Error,
}

// ---- Conversion functions ----

pub fn runtime_tool_entity_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{}:tool:{}", parent_message_id, tool_call_index)
}

/// Convert a pi-ai compatible Message to a RuntimeMessage.
pub fn to_runtime_message(message: &Message, id: String) -> RuntimeMessage {
    let timestamp = match message {
        Message::User { timestamp, .. } => *timestamp,
        Message::Assistant { timestamp, .. } => *timestamp,
        Message::ToolResult { timestamp, .. } => *timestamp,
    };

    match message {
        Message::User { content, .. } => {
            let blocks = match content {
                MessageContent::String(text) => {
                    vec![RuntimeUserContentBlock::Text { text: text.clone() }]
                }
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => {
                            RuntimeUserContentBlock::Text { text: text.clone() }
                        }
                        ContentBlock::Image { data, mime_type } => RuntimeUserContentBlock::Image {
                            data: data.clone(),
                            mime_type: mime_type.clone(),
                        },
                        _ => RuntimeUserContentBlock::Text {
                            text: format!("{:?}", b),
                        },
                    })
                    .collect(),
            };
            RuntimeMessage::User {
                id,
                content: blocks,
                timestamp,
            }
        }
        Message::Assistant {
            content,
            model,
            provider,
            usage,
            stop_reason,
            error_message,
            timestamp,
            ..
        } => {
            let blocks: Vec<RuntimeAssistantContentBlock> = content
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => {
                        RuntimeAssistantContentBlock::Text { text: text.clone() }
                    }
                    ContentBlock::Thinking {
                        thinking,
                        thinking_signature,
                    } => RuntimeAssistantContentBlock::Thinking {
                        thinking: thinking.clone(),
                        thinking_signature: thinking_signature.clone(),
                    },
                    ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                        ..
                    } => RuntimeAssistantContentBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                        partial_json: None,
                    },
                    _ => RuntimeAssistantContentBlock::Text {
                        text: format!("{:?}", b),
                    },
                })
                .collect();

            RuntimeMessage::Assistant {
                id,
                content: blocks,
                is_streaming: None,
                stop_reason: stop_reason.clone(),
                error_message: error_message.clone(),
                usage: usage.clone(),
                provider: Some(provider.clone()),
                model: Some(model.clone()),
                timestamp: *timestamp,
            }
        }
        Message::ToolResult {
            tool_call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp,
        } => {
            let value = details
                .clone()
                .unwrap_or_else(|| serde_json::to_value(content).unwrap_or_default());
            RuntimeMessage::ToolResult {
                id,
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                content: value,
                is_error: *is_error,
                timestamp: *timestamp,
            }
        }
    }
}

/// Convert a RuntimeMessage back to a Message (pi-ai compatible).
pub fn to_message(runtime_message: &RuntimeMessage) -> Message {
    let timestamp = match runtime_message {
        RuntimeMessage::User { timestamp, .. } => {
            timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
        }
        RuntimeMessage::Assistant { timestamp, .. } => {
            timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
        }
        RuntimeMessage::ToolResult { timestamp, .. } => {
            timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
        }
        RuntimeMessage::Custom { timestamp, .. } => {
            timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
        }
    };

    match runtime_message {
        RuntimeMessage::User { content, .. } => {
            let blocks: Vec<ContentBlock> = content
                .iter()
                .map(|b| match b {
                    RuntimeUserContentBlock::Text { text } => {
                        ContentBlock::Text { text: text.clone() }
                    }
                    RuntimeUserContentBlock::Image { data, mime_type } => ContentBlock::Image {
                        data: data.clone(),
                        mime_type: mime_type.clone(),
                    },
                })
                .collect();
            Message::User {
                content: MessageContent::Blocks(blocks),
                timestamp: Some(timestamp),
            }
        }
        RuntimeMessage::Assistant {
            content,
            provider,
            model,
            usage,
            stop_reason,
            error_message,
            ..
        } => {
            let blocks: Vec<ContentBlock> = content
                .iter()
                .map(|b| match b {
                    RuntimeAssistantContentBlock::Text { text } => {
                        ContentBlock::Text { text: text.clone() }
                    }
                    RuntimeAssistantContentBlock::Thinking {
                        thinking,
                        thinking_signature,
                    } => ContentBlock::Thinking {
                        thinking: thinking.clone(),
                        thinking_signature: thinking_signature.clone(),
                    },
                    RuntimeAssistantContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                        ..
                    } => ContentBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                        partial_json: None,
                    },
                })
                .collect();

            Message::Assistant {
                content: blocks,
                api: "openai-completions".to_string(),
                provider: provider.clone().unwrap_or_else(|| "unknown".to_string()),
                model: model.clone().unwrap_or_else(|| "unknown".to_string()),
                usage: usage.clone(),
                stop_reason: stop_reason.clone(),
                error_message: error_message.clone(),
                timestamp: Some(timestamp),
            }
        }
        RuntimeMessage::ToolResult {
            tool_call_id,
            tool_name,
            content,
            is_error,
            ..
        } => {
            let text = match content {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            };
            Message::ToolResult {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                content: vec![ContentBlock::Text { text }],
                details: Some(content.clone()),
                is_error: *is_error,
                timestamp: Some(timestamp),
            }
        }
        RuntimeMessage::Custom { content, .. } => Message::User {
            content: MessageContent::String(match content {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            }),
            timestamp: Some(timestamp),
        },
    }
}

/// Convert a provider partial (pi-ai AssistantMessage) to a RuntimeAssistantMessage.
pub fn provider_partial_to_runtime_assistant(
    partial: &Message, // In pi-ai this is an AssistantMessage
    id: String,
    is_streaming: bool,
) -> RuntimeMessage {
    match partial {
        Message::Assistant {
            content,
            stop_reason,
            error_message,
            usage,
            provider,
            model,
            timestamp,
            ..
        } => {
            let blocks: Vec<RuntimeAssistantContentBlock> = content
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => {
                        RuntimeAssistantContentBlock::Text { text: text.clone() }
                    }
                    ContentBlock::Thinking {
                        thinking,
                        thinking_signature,
                    } => RuntimeAssistantContentBlock::Thinking {
                        thinking: thinking.clone(),
                        thinking_signature: thinking_signature.clone(),
                    },
                    ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                        ..
                    } => RuntimeAssistantContentBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                        partial_json: None,
                    },
                    _ => RuntimeAssistantContentBlock::Text {
                        text: format!("{:?}", b),
                    },
                })
                .collect();

            RuntimeMessage::Assistant {
                id,
                content: blocks,
                is_streaming: Some(is_streaming),
                stop_reason: stop_reason.clone(),
                error_message: error_message.clone(),
                usage: usage.clone(),
                provider: Some(provider.clone()),
                model: Some(model.clone()),
                timestamp: *timestamp,
            }
        }
        _ => RuntimeMessage::Assistant {
            id,
            content: vec![],
            is_streaming: Some(is_streaming),
            stop_reason: None,
            error_message: None,
            usage: None,
            provider: None,
            model: None,
            timestamp: None,
        },
    }
}
