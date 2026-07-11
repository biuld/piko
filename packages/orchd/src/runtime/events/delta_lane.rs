use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

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
}

impl AssistantMessageState {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            usage: None,
            stop_reason: "stop".into(),
            error_message: None,
        }
    }

    pub(crate) fn apply_gateway_event(&mut self, event: &GatewayEvent) {
        match event {
            GatewayEvent::ContentDelta(delta) => self.text.push_str(delta),
            GatewayEvent::ReasoningDelta(delta) => self.reasoning.push_str(delta),
            GatewayEvent::Usage(usage) => self.usage = Some(usage.clone()),
            GatewayEvent::Done(reason) => self.stop_reason = reason.clone(),
            GatewayEvent::Error(error) => {
                tracing::error!("Stream error: {error}");
                self.stop_reason = "error".into();
                self.error_message = Some(error.clone());
            }
            GatewayEvent::ToolCallChunk { .. } => {}
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
            api: "openai-completions".into(),
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
            ctx.task_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ));
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                self.collector.push(RealtimeFrame::new(
                    ctx.task_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Text {
                        content_index: self.state.text.len() as u32,
                        delta: delta.clone(),
                    },
                ));
            }
            GatewayEvent::ReasoningDelta(delta) => {
                self.collector.push(RealtimeFrame::new(
                    ctx.task_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Thinking {
                        content_index: self.state.reasoning.len() as u32,
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
            ctx.task_id.clone(),
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
