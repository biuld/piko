use serde::{Deserialize, Serialize};

use super::*;

// ── Dispatch framework: typed channel event types ──

/// persist channel — final-state events consumed by hostd and mapped to SessionTreeEntry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistEvent {
    /// User-role transcript input, including initial prompts and later steering.
    UserCommitted {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        message: crate::messages::Message,
    },
    /// Assistant message finalized.
    Finalized {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        message: crate::messages::Message,
    },
    /// Tool call committed.
    ToolCallCommitted {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        parent_message_id: MessageId,
        message: crate::messages::Message,
    },
    /// Tool execution result.
    ToolResultCommitted {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        parent_message_id: MessageId,
        message: crate::messages::Message,
    },
}
