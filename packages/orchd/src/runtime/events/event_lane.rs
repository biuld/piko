use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::runtime::events::collector::{SharedAssistantMessageCollector, SharedPersistCollector};
use crate::runtime::events::identity::{AgentDispatchContext, StepEventConsumer};
use piko_protocol::PersistEvent;

use super::delta_lane::AssistantMessageState;

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
            agent_instance_id: ctx.agent_instance_id.clone(),
            agent_id: ctx.agent_id.clone(),
            source_turn_id: ctx.work_id.to_string(),
            message: assistant_message,
        });
    }
}
