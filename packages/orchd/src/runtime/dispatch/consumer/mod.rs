use async_trait::async_trait;
use llmd::gateway::GatewayEvent;

use crate::domain::ModelSpec;
use crate::runtime::types::ToolCallItem;
use piko_protocol::{AgentId, Message, MessageId, SessionId, TaskId};

pub mod display;
pub mod lifecycle;
pub mod persist;
pub mod tool;

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

    pub(crate) fn into_parts(self) -> (SessionId, TaskId, AgentId) {
        (self.session_id, self.task_id, self.agent_id)
    }

    pub(crate) fn as_context<'a>(
        &'a self,
        message_id: &'a MessageId,
        model: Option<&'a ModelSpec>,
    ) -> AgentDispatchContext<'a> {
        AgentDispatchContext {
            session_id: &self.session_id,
            task_id: &self.task_id,
            agent_id: &self.agent_id,
            message_id,
            model,
        }
    }
}

pub(crate) struct AgentDispatchContext<'a> {
    pub session_id: &'a SessionId,
    pub task_id: &'a TaskId,
    pub agent_id: &'a AgentId,
    pub message_id: &'a MessageId,
    pub model: Option<&'a ModelSpec>,
}

#[allow(dead_code)]
#[async_trait]
pub(crate) trait AgentEventConsumer: Send {
    async fn on_task_created(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _parent_task_id: Option<&str>,
        _source_agent_id: Option<&str>,
        _prompt: &str,
        _turn_id: &str,
    ) {
    }

    async fn on_task_started(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_task_steered(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _source_task_id: &str,
        _source_agent_id: &str,
        _message: &str,
    ) {
    }

    async fn on_task_idle(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _total_steps: u32,
        _summary: &str,
    ) {
    }

    async fn on_task_failed(&mut self, _ctx: &AgentDispatchContext<'_>, _error: &str) {}

    async fn on_task_completed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _total_steps: u32,
        _summary: &str,
    ) {
    }

    async fn on_task_cancelled(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_task_closed(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_task_reopened(&mut self, _ctx: &AgentDispatchContext<'_>) {}

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

    async fn on_tool_started(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call: &ToolCallItem,
        _msg_id: &str,
    ) {
    }

    async fn on_tool_ended(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call: &ToolCallItem,
        _result: &serde_json::Value,
        _is_error: bool,
    ) {
    }

    async fn on_tool_result_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call_index: u32,
        _message: &Message,
        _msg_id: &str,
    ) {
    }
}
