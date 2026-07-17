use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::ToolCallItem;
use crate::ports::tool_provider::ToolExecutionContext;
use piko_protocol::agents::HostSessionContext;
use piko_protocol::{AgentId, AgentInstanceId, ExecutionId, Message, MessageId, SessionId};

#[derive(Clone)]
pub(crate) struct DispatchIdentity {
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    execution_id: ExecutionId,
    agent_id: AgentId,
}

impl DispatchIdentity {
    pub(crate) fn new(
        session_id: SessionId,
        agent_instance_id: AgentInstanceId,
        execution_id: ExecutionId,
        agent_id: AgentId,
    ) -> Self {
        Self {
            session_id,
            agent_instance_id,
            execution_id,
            agent_id,
        }
    }

    pub(crate) fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub(crate) fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    pub(crate) fn agent_instance_id(&self) -> &AgentInstanceId {
        &self.agent_instance_id
    }

    pub(crate) fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    pub(crate) fn host_session_context(&self) -> HostSessionContext {
        HostSessionContext::new(self.session_id.clone())
    }

    pub(crate) fn from_tool_execution(context: &ToolExecutionContext) -> Self {
        if let Some(ref host_context) = context.host_context {
            Self::new(
                host_context.session_id.clone(),
                context.agent_instance_id.clone(),
                context.execution_id.clone(),
                context.agent_id.clone(),
            )
        } else {
            Self::new(
                context.execution_id.clone(),
                context.agent_instance_id.clone(),
                context.execution_id.clone(),
                context.agent_id.clone(),
            )
        }
    }

    pub(crate) fn as_context<'a>(
        &'a self,
        message_id: &'a MessageId,
        model: Option<&'a ModelSpec>,
        source_turn_id: &'a str,
    ) -> AgentDispatchContext<'a> {
        AgentDispatchContext {
            session_id: &self.session_id,
            agent_instance_id: &self.agent_instance_id,
            execution_id: &self.execution_id,
            agent_id: &self.agent_id,
            message_id,
            source_turn_id,
            model,
        }
    }
}

pub(crate) fn host_session_context_from_execution(
    context: &ToolExecutionContext,
) -> HostSessionContext {
    DispatchIdentity::from_tool_execution(context).host_session_context()
}

pub(crate) struct AgentDispatchContext<'a> {
    pub session_id: &'a SessionId,
    pub agent_instance_id: &'a AgentInstanceId,
    pub execution_id: &'a ExecutionId,
    pub agent_id: &'a AgentId,
    pub message_id: &'a MessageId,
    pub source_turn_id: &'a str,
    pub model: Option<&'a ModelSpec>,
}

/// Consumer hooks for a single LLM step dispatch.
#[async_trait]
pub(crate) trait StepEventConsumer: Send {
    async fn on_step_started(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, _event: &GatewayEvent) {}

    async fn on_step_finished(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_assistant_message_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _message: &Message,
        _tool_calls: &[ToolCallItem],
    ) {
    }
}
