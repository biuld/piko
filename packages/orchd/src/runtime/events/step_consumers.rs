use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::domain::RealtimeFrame;
use crate::domain::transcript::Message;
use crate::runtime::events::TaskEventEmitter;
use crate::runtime::events::collector::{
    SharedAssistantMessageCollector, SharedPersistCollector, SharedRealtimeCollector,
};
use crate::runtime::events::delta_lane::AssistantMessageState;
use crate::runtime::events::identity::{AgentDispatchContext, StepEventConsumer};
use piko_protocol::PersistEvent;
use piko_protocol::agent_runtime::RealtimeDelta;

pub(crate) struct EmitterRealtimeConsumer {
    emitter: TaskEventEmitter,
    collector: SharedRealtimeCollector,
    state: AssistantMessageState,
}

impl EmitterRealtimeConsumer {
    pub(crate) fn new(emitter: TaskEventEmitter, collector: SharedRealtimeCollector) -> Self {
        Self {
            emitter,
            collector,
            state: AssistantMessageState::new(),
        }
    }

    async fn emit(&self, frame: RealtimeFrame) {
        self.emitter.emit_realtime(frame.clone()).await;
        self.collector.push(frame);
    }
}

#[async_trait]
impl StepEventConsumer for EmitterRealtimeConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.emit(RealtimeFrame::new(
            ctx.task_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ))
        .await;
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                self.emit(RealtimeFrame::new(
                    ctx.task_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Text {
                        content_index: self.state.text.len() as u32,
                        delta: delta.clone(),
                    },
                ))
                .await;
            }
            GatewayEvent::ReasoningDelta(delta) => {
                self.emit(RealtimeFrame::new(
                    ctx.task_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Thinking {
                        content_index: self.state.reasoning.len() as u32,
                        delta: delta.clone(),
                    },
                ))
                .await;
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.emit(RealtimeFrame::new(
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
        ))
        .await;
    }
}

pub(crate) struct EmitterPersistConsumer {
    emitter: TaskEventEmitter,
    collector: SharedPersistCollector,
    assistant_message_collector: SharedAssistantMessageCollector,
    state: AssistantMessageState,
}

impl EmitterPersistConsumer {
    pub(crate) fn new(
        emitter: TaskEventEmitter,
        collector: SharedPersistCollector,
        assistant_message_collector: SharedAssistantMessageCollector,
    ) -> Self {
        Self {
            emitter,
            collector,
            assistant_message_collector,
            state: AssistantMessageState::new(),
        }
    }

    async fn emit(&self, event: PersistEvent) {
        self.emitter.emit_persist(event.clone()).await;
        self.collector.push(event);
    }
}

#[async_trait]
impl StepEventConsumer for EmitterPersistConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.assistant_message_collector
            .set(assistant_message.clone());
        self.emit(PersistEvent::Finalized {
            session_id: ctx.session_id.clone(),
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            work_id: ctx.work_id.to_string(),
            message: assistant_message,
        })
        .await;
    }
}
