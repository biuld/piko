use piko_protocol::{
    AgentInput, AgentInputDisposition, AgentInstanceIdentity, AgentInstanceLifecycle, AgentSpec,
    AgentWorkReport,
};
use serde::{Deserialize, Serialize};

use crate::{ExecutionStartedV1, MessageCommittedV1, ModelStepCommittedV1, TreeEntryRecordedV1};

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
    #[serde(default)]
    pub model_step_ids: Vec<String>,
    pub report: Option<AgentWorkReport>,
    pub finished_at: Option<i64>,
}

/// Write-time projection of the primitive AgentInput admission fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredAgentInput {
    pub input: AgentInput,
    /// Immutable admission proposal fields retained separately from the
    /// current disposition/correlation projection. A duplicate request must
    /// remain idempotent after the input has advanced or been cancelled.
    #[serde(default = "default_admission_disposition")]
    pub admission_disposition: AgentInputDisposition,
    #[serde(default)]
    pub admission_root_input_id: Option<String>,
    pub disposition: AgentInputDisposition,
    pub admission_revision: u64,
    pub admission_event_id: String,
    pub admitted_at: i64,
    #[serde(default)]
    pub root_input_id: Option<String>,
    #[serde(default)]
    pub model_step_id: Option<String>,
    #[serde(default)]
    pub applied_message_id: Option<String>,
}

fn default_admission_disposition() -> AgentInputDisposition {
    AgentInputDisposition::PendingFollowUp
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredModelStep {
    pub revision: u64,
    pub event_id: String,
    pub data: ModelStepCommittedV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelContinuity {
    pub provider: String,
    pub model_id: String,
}
