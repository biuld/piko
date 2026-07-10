use std::sync::Arc;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;

use crate::runtime::dispatch::PersistEvent;
use crate::runtime::dispatch::step::collectors::{
    SharedAssistantMessageCollector, SharedPersistCollector,
};

use super::display::AssistantMessageState;
use super::{AgentDispatchContext, StepEventConsumer};

pub(crate) struct AssistantPersistChannelConsumer {
    tx: mpsc::Sender<Arc<PersistEvent>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    state: AssistantMessageState,
}

impl AssistantPersistChannelConsumer {
    pub(crate) fn new(
        tx: mpsc::Sender<Arc<PersistEvent>>,
        assistant_message_collector: SharedAssistantMessageCollector,
        state: AssistantMessageState,
    ) -> Self {
        Self {
            tx,
            assistant_message_collector,
            state,
        }
    }
}

#[async_trait]
impl StepEventConsumer for AssistantPersistChannelConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.assistant_message_collector
            .set(assistant_message.clone());
        if self
            .tx
            .send(Arc::new(PersistEvent::Finalized {
                session_id: ctx.session_id.clone(),
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                message: assistant_message,
            }))
            .await
            .is_err()
        {
            tracing::error!(message_id = %ctx.message_id, "persist channel closed");
        }
    }
}

pub(crate) struct AssistantPersistCollectingConsumer {
    collector: SharedPersistCollector,
    assistant_message_collector: SharedAssistantMessageCollector,
    state: AssistantMessageState,
}

impl AssistantPersistCollectingConsumer {
    pub(crate) fn new(
        collector: SharedPersistCollector,
        assistant_message_collector: SharedAssistantMessageCollector,
        state: AssistantMessageState,
    ) -> Self {
        Self {
            collector,
            assistant_message_collector,
            state,
        }
    }
}

#[async_trait]
impl StepEventConsumer for AssistantPersistCollectingConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.assistant_message_collector
            .set(assistant_message.clone());
        self.collector.push(PersistEvent::Finalized {
            session_id: ctx.session_id.clone(),
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            message: assistant_message,
        });
    }
}
