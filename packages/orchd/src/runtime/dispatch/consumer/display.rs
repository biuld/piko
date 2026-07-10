use std::sync::Arc;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;

use crate::domain::ModelSpec;
use crate::domain::model::transcript::{ContentBlock, MessageUsage};
use crate::runtime::utils::now_ms;
use piko_protocol::Message;

use crate::runtime::dispatch::DisplayEvent;
use crate::runtime::dispatch::step::collectors::SharedDisplayCollector;

use super::{AgentDispatchContext, StepEventConsumer};

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

pub(crate) struct DisplayChannelConsumer {
    tx: mpsc::Sender<Arc<DisplayEvent>>,
    state: AssistantMessageState,
}

impl DisplayChannelConsumer {
    pub(crate) fn new(tx: mpsc::Sender<Arc<DisplayEvent>>, state: AssistantMessageState) -> Self {
        Self { tx, state }
    }

    async fn emit(&self, message_id: &str, event: DisplayEvent) {
        if self.tx.send(Arc::new(event)).await.is_err() {
            tracing::error!(%message_id, "display channel closed");
        }
    }
}

#[async_trait]
impl StepEventConsumer for DisplayChannelConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.emit(
            ctx.message_id,
            DisplayEvent::MessageStart {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                role: piko_protocol::MessageRole::Assistant,
            },
        )
        .await;
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                self.emit(
                    ctx.message_id,
                    DisplayEvent::TextDelta {
                        message_id: ctx.message_id.clone(),
                        task_id: ctx.task_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        content_index: self.state.text.len() as u32,
                        delta: delta.clone(),
                    },
                )
                .await;
            }
            GatewayEvent::ReasoningDelta(delta) => {
                self.emit(
                    ctx.message_id,
                    DisplayEvent::ThinkingDelta {
                        message_id: ctx.message_id.clone(),
                        task_id: ctx.task_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        content_index: self.state.reasoning.len() as u32,
                        delta: delta.clone(),
                    },
                )
                .await;
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.emit(
            ctx.message_id,
            DisplayEvent::MessageEnd {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            },
        )
        .await;
    }

    async fn on_assistant_message_committed(
        &mut self,
        ctx: &AgentDispatchContext<'_>,
        assistant_message: &Message,
        _tool_calls: &[crate::runtime::types::ToolCallItem],
    ) {
        self.emit(
            ctx.message_id,
            DisplayEvent::Finalized {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                content: match &assistant_message {
                    Message::Assistant { content, .. } => content.clone(),
                    _ => Vec::new(),
                },
                usage: match &assistant_message {
                    Message::Assistant { usage, .. } => usage.clone(),
                    _ => None,
                },
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            },
        )
        .await;
    }
}

pub(crate) struct DisplayCollectingConsumer {
    collector: SharedDisplayCollector,
    state: AssistantMessageState,
}

impl DisplayCollectingConsumer {
    pub(crate) fn new(collector: SharedDisplayCollector, state: AssistantMessageState) -> Self {
        Self { collector, state }
    }
}

#[async_trait]
impl StepEventConsumer for DisplayCollectingConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.collector.push(DisplayEvent::MessageStart {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            role: piko_protocol::MessageRole::Assistant,
        });
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                self.collector.push(DisplayEvent::TextDelta {
                    message_id: ctx.message_id.clone(),
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    content_index: self.state.text.len() as u32,
                    delta: delta.clone(),
                });
            }
            GatewayEvent::ReasoningDelta(delta) => {
                self.collector.push(DisplayEvent::ThinkingDelta {
                    message_id: ctx.message_id.clone(),
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    content_index: self.state.reasoning.len() as u32,
                    delta: delta.clone(),
                });
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.collector.push(DisplayEvent::MessageEnd {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            stop_reason: match &assistant_message {
                Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                _ => None,
            },
            error_message: match &assistant_message {
                Message::Assistant { error_message, .. } => error_message.clone(),
                _ => None,
            },
        });
    }

    async fn on_assistant_message_committed(
        &mut self,
        ctx: &AgentDispatchContext<'_>,
        assistant_message: &Message,
        _tool_calls: &[crate::runtime::types::ToolCallItem],
    ) {
        self.collector.push(DisplayEvent::Finalized {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            content: match &assistant_message {
                Message::Assistant { content, .. } => content.clone(),
                _ => Vec::new(),
            },
            usage: match &assistant_message {
                Message::Assistant { usage, .. } => usage.clone(),
                _ => None,
            },
            stop_reason: match &assistant_message {
                Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                _ => None,
            },
            error_message: match &assistant_message {
                Message::Assistant { error_message, .. } => error_message.clone(),
                _ => None,
            },
        });
    }
}
