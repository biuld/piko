use std::pin::Pin;

use futures_core::Stream;
use llmd::gateway::GatewayEvent;

use crate::domain::ModelSpec;
use piko_protocol::{AgentId, MessageId, SessionId, TaskId};

pub(crate) enum StepDispatchSource {
    StepStream(StepDispatchInput),
    StepFailure(StepFailureInput),
}

pub(crate) struct StepDispatchInput {
    pub(crate) session_id: SessionId,
    pub(crate) task_id: TaskId,
    pub(crate) agent_id: AgentId,
    pub(crate) message_id: MessageId,
    pub(crate) model: ModelSpec,
    pub(crate) events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
}

pub(crate) struct StepFailureInput {
    pub(crate) session_id: SessionId,
    pub(crate) task_id: TaskId,
    pub(crate) agent_id: AgentId,
    pub(crate) message_id: MessageId,
    pub(crate) model: ModelSpec,
    pub(crate) error_message: String,
}
