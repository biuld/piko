use async_trait::async_trait;
use piko_llmd::gateway::InferenceEvent;

use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::ToolCallItem;
use piko_protocol::{AgentId, AgentInstanceId, Message, MessageId, SessionId};

#[derive(Clone)]
pub(crate) struct DispatchIdentity {
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    agent_id: AgentId,
}

impl DispatchIdentity {
    pub(crate) fn new(
        session_id: SessionId,
        agent_instance_id: AgentInstanceId,
        _root_input_id: String,
        agent_id: AgentId,
    ) -> Self {
        Self {
            session_id,
            agent_instance_id,
            agent_id,
        }
    }

    pub(crate) fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub(crate) fn as_context<'a>(
        &'a self,
        message_id: &'a MessageId,
        model: Option<&'a ModelSpec>,
        root_input_id: &'a str,
    ) -> AgentDispatchContext<'a> {
        AgentDispatchContext {
            session_id: &self.session_id,
            agent_instance_id: &self.agent_instance_id,
            agent_id: &self.agent_id,
            message_id,
            root_input_id,
            model,
        }
    }
}

pub(crate) struct AgentDispatchContext<'a> {
    pub session_id: &'a SessionId,
    pub agent_instance_id: &'a AgentInstanceId,
    pub root_input_id: &'a str,
    pub agent_id: &'a AgentId,
    pub message_id: &'a MessageId,
    pub model: Option<&'a ModelSpec>,
}

/// Consumer hooks for a single LLM step dispatch.
#[async_trait]
pub(crate) trait StepEventConsumer: Send {
    async fn on_step_started(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, _event: &InferenceEvent) {
    }

    async fn on_step_finished(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_assistant_message_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _message: &Message,
        _tool_calls: &[ToolCallItem],
    ) {
    }
}
