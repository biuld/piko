use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::domain::model::step::ModelSpec;
use crate::domain::tasks::task::HostTaskContext;
use crate::ports::tool_provider::ToolExecutionContext;
use crate::runtime::types::ToolCallItem;
use piko_protocol::{AgentId, Message, MessageId, SessionId, TaskId};

#[derive(Clone)]
pub(crate) struct DispatchIdentity {
    session_id: SessionId,
    task_id: TaskId,
    agent_id: AgentId,
}

impl DispatchIdentity {
    pub(crate) fn new(session_id: SessionId, task_id: TaskId, agent_id: AgentId) -> Self {
        Self {
            session_id,
            task_id,
            agent_id,
        }
    }

    pub(crate) fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub(crate) fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    pub(crate) fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    pub(crate) fn host_task_context(&self, turn_id: impl Into<String>) -> HostTaskContext {
        HostTaskContext {
            session_id: self.session_id.clone(),
            turn_id: turn_id.into(),
        }
    }

    pub(crate) fn from_tool_execution(context: &ToolExecutionContext) -> Self {
        if let Some(ref host_context) = context.host_context {
            Self::new(
                host_context.session_id.clone(),
                context.task_id.clone(),
                context.agent_id.clone(),
            )
        } else {
            Self::new(
                context.task_id.clone(),
                context.task_id.clone(),
                context.agent_id.clone(),
            )
        }
    }

    pub(crate) fn as_context<'a>(
        &'a self,
        message_id: &'a MessageId,
        model: Option<&'a ModelSpec>,
        work_id: &'a str,
    ) -> AgentDispatchContext<'a> {
        AgentDispatchContext {
            session_id: &self.session_id,
            task_id: &self.task_id,
            agent_id: &self.agent_id,
            message_id,
            work_id,
            model,
        }
    }
}

pub(crate) fn host_task_context_from_execution(context: &ToolExecutionContext) -> HostTaskContext {
    let turn_id = context
        .host_context
        .as_ref()
        .map(|hc| hc.turn_id.clone())
        .unwrap_or_else(|| context.task_id.clone());
    DispatchIdentity::from_tool_execution(context).host_task_context(turn_id)
}

pub(crate) struct AgentDispatchContext<'a> {
    pub session_id: &'a SessionId,
    pub task_id: &'a TaskId,
    pub agent_id: &'a AgentId,
    pub message_id: &'a MessageId,
    pub work_id: &'a str,
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
