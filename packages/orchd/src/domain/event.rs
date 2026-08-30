use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{AgentId, AgentInstanceId, MessageId};

/// Identity + realtime payload for one orchd step observation frame.
///
/// Public host observation uses `SessionOutput::Delta`; this type stays inside the runtime.
#[derive(Debug, Clone)]
pub struct RealtimeFrame {
    pub agent_instance_id: AgentInstanceId,
    pub root_input_id: String,
    pub agent_id: AgentId,
    pub message_id: MessageId,
    pub delta: RealtimeDelta,
}

impl RealtimeFrame {
    pub fn new(
        agent_instance_id: impl Into<AgentInstanceId>,
        root_input_id: impl Into<String>,
        agent_id: impl Into<AgentId>,
        message_id: impl Into<MessageId>,
        delta: RealtimeDelta,
    ) -> Self {
        Self {
            agent_instance_id: agent_instance_id.into(),
            root_input_id: root_input_id.into(),
            agent_id: agent_id.into(),
            message_id: message_id.into(),
            delta,
        }
    }
}
