use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{AgentId, AgentInstanceId, MessageId, TaskId};

/// Identity + realtime payload for one orchd step observation frame.
///
/// Public host observation uses `SessionOutput::Delta`; this type stays inside the runtime.
#[derive(Debug, Clone)]
pub struct RealtimeFrame {
    pub agent_instance_id: AgentInstanceId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub message_id: MessageId,
    pub delta: RealtimeDelta,
}

impl RealtimeFrame {
    pub fn new(
        agent_instance_id: impl Into<AgentInstanceId>,
        task_id: impl Into<TaskId>,
        agent_id: impl Into<AgentId>,
        message_id: impl Into<MessageId>,
        delta: RealtimeDelta,
    ) -> Self {
        Self {
            agent_instance_id: agent_instance_id.into(),
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            message_id: message_id.into(),
            delta,
        }
    }
}
