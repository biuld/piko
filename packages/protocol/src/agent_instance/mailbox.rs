use serde::{Deserialize, Serialize};

use super::*;

/// Best-effort mailbox notification emitted by an AgentActor after a durable
/// state change (F-10 / D-10). Consumers must not rely on replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentMailboxEvent {
    InboxReport {
        agent_instance_id: AgentInstanceId,
        report_id: String,
        source_agent_instance_id: AgentInstanceId,
    },
    WorkFinished {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        report_id: String,
    },
    InputQueued {
        agent_instance_id: AgentInstanceId,
        request_id: String,
    },
}

impl AgentMailboxEvent {
    pub fn agent_instance_id(&self) -> &str {
        match self {
            Self::InboxReport {
                agent_instance_id, ..
            }
            | Self::WorkFinished {
                agent_instance_id, ..
            }
            | Self::InputQueued {
                agent_instance_id, ..
            } => agent_instance_id.as_str(),
        }
    }
}
