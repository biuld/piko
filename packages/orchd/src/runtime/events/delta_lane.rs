use async_trait::async_trait;
use piko_llmd::gateway::{ModelEvent, ModelOutputMetadata, TerminalStatus};

use crate::domain::model::step::ModelSpec;
use crate::domain::transcript::{ContentBlock, MessageUsage};
use crate::ports::clock::now_ms;
use piko_protocol::Message;
use piko_protocol::agent_runtime::RealtimeDelta;

use crate::domain::RealtimeFrame;
use crate::runtime::events::collector::SharedRealtimeCollector;
use crate::runtime::events::identity::{AgentDispatchContext, StepEventConsumer};

#[derive(Clone)]
pub(crate) struct AssistantMessageState {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) usage: Option<MessageUsage>,
    pub(crate) stop_reason: String,
    pub(crate) error_message: Option<String>,
    pub(crate) output_metadata: Option<ModelOutputMetadata>,
}

impl AssistantMessageState {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            usage: None,
            stop_reason: "stop".into(),
            error_message: None,
            output_metadata: None,
        }
    }

    pub(crate) fn apply_gateway_event(&mut self, event: &ModelEvent) {
        match event {
            ModelEvent::TextDelta { delta, .. } | ModelEvent::RefusalDelta { delta, .. } => {
                self.text.push_str(delta);
            }
            ModelEvent::ReasoningDelta { delta, .. } => {
                self.reasoning.push_str(delta);
            }
            ModelEvent::Usage(usage) => self.usage = Some(usage.clone()),
            ModelEvent::Completed(status) => {
                if matches!(
                    status,
                    TerminalStatus::Completed { .. } | TerminalStatus::Incomplete { .. }
                ) && self.output_metadata.is_none()
                {
                    self.stop_reason = "error".into();
                    self.error_message =
                        Some("model stream completed without output metadata".into());
                    return;
                }
                self.stop_reason = match status {
                    TerminalStatus::Completed { reason } => reason.clone(),
                    TerminalStatus::Incomplete { reason } => {
                        reason.clone().unwrap_or_else(|| "incomplete".into())
                    }
                    TerminalStatus::Failed { message } => {
                        self.error_message = Some(message.clone());
                        "error".into()
                    }
                    TerminalStatus::Cancelled => "abort".into(),
                };
            }
            ModelEvent::Error(error) => {
                tracing::error!("Stream error: {error}");
                self.stop_reason = "error".into();
                self.error_message = Some(error.to_string());
            }
            ModelEvent::OutputMetadata(metadata) => self.output_metadata = Some(metadata.clone()),
            ModelEvent::FunctionCallDelta { .. } => {}
        }
    }

    pub(crate) fn build_message(&self, model: &ModelSpec) -> Message {
        let mut blocks = Vec::new();
        if !self.reasoning.is_empty() {
            blocks.push(ContentBlock::Thinking {
                thinking: self.reasoning.clone(),
                thinking_signature: None,
            });
        }
        if !self.text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: self.text.clone(),
            });
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::Text {
                text: String::new(),
            });
        }
        Message::Assistant {
            content: blocks,
            continuation: self
                .output_metadata
                .as_ref()
                .and_then(|metadata| metadata.continuation.clone())
                .map(Box::new),
            provider: model.provider.clone(),
            model: model.id.clone(),
            usage: self.usage.clone(),
            stop_reason: Some(self.stop_reason.clone()),
            error_message: self.error_message.clone(),
            timestamp: Some(now_ms()),
        }
    }
}

pub(crate) struct RealtimeCollectingConsumer {
    collector: SharedRealtimeCollector,
    state: AssistantMessageState,
}

impl RealtimeCollectingConsumer {
    pub(crate) fn new(collector: SharedRealtimeCollector, state: AssistantMessageState) -> Self {
        Self { collector, state }
    }
}

#[async_trait]
impl StepEventConsumer for RealtimeCollectingConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.collector.push(RealtimeFrame::new(
            ctx.agent_instance_id.clone(),
            ctx.execution_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ));
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &ModelEvent) {
        self.state.apply_gateway_event(event);
        match event {
            ModelEvent::TextDelta { delta, .. } | ModelEvent::RefusalDelta { delta, .. } => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Text {
                        // Text chunks belong to one stable content segment;
                        // this is a segment id, not a byte offset.
                        content_index: 0,
                        delta: delta.clone(),
                    },
                ));
            }
            ModelEvent::ReasoningDelta { delta, .. } => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Thinking {
                        // Thought and text are distinct stream item kinds, so
                        // each kind owns its own segment-zero namespace.
                        content_index: 0,
                        delta: delta.clone(),
                    },
                ));
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.collector.push(RealtimeFrame::new(
            ctx.agent_instance_id.clone(),
            ctx.execution_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageEnded {
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use piko_llmd::gateway::{ModelEvent, ModelOutputMetadata};
    use piko_protocol::Message;

    use super::*;

    #[test]
    fn llmd_output_metadata_is_persisted_without_interpretation() {
        let mut state = AssistantMessageState::new();
        let continuation = serde_json::from_value(serde_json::json!({
            "adapter": "opaque-test-adapter",
            "state": { "private": ["call_1"] }
        }))
        .unwrap();
        let metadata = ModelOutputMetadata {
            continuation: Some(continuation),
        };
        state.apply_gateway_event(&ModelEvent::OutputMetadata(metadata.clone()));

        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });

        assert!(matches!(
            message,
            Message::Assistant {
                continuation: Some(continuation),
                ..
            } if Some(continuation.as_ref()) == metadata.continuation.as_ref()
        ));
    }

    #[test]
    fn successful_terminal_without_metadata_fails_closed() {
        let mut state = AssistantMessageState::new();
        state.apply_gateway_event(&ModelEvent::completed("stop"));

        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });
        assert!(matches!(
            message,
            Message::Assistant {
                continuation: None,
                stop_reason: Some(reason),
                error_message: Some(error),
                ..
            } if reason == "error" && error == "model stream completed without output metadata"
        ));
    }
}
