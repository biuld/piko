use piko_protocol::{AgentInstanceIdentity, AgentInstanceLifecycle, AgentRunReport, AgentSpec};
use serde::{Deserialize, Serialize};

use crate::{ExecutionStartedV1, MessageCommittedV1, TreeEntryRecordedV1};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredMessage {
    pub revision: u64,
    pub event_id: String,
    pub data: MessageCommittedV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredTreeEntry {
    pub revision: u64,
    pub event_id: String,
    pub data: TreeEntryRecordedV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredAgent {
    pub identity: AgentInstanceIdentity,
    pub spec: Option<AgentSpec>,
    pub lifecycle: AgentInstanceLifecycle,
    pub created_at: i64,
    pub changed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredExecution {
    pub started: ExecutionStartedV1,
    pub message_head: Option<String>,
    pub report: Option<AgentRunReport>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelContinuity {
    pub provider: String,
    pub model_id: String,
}
