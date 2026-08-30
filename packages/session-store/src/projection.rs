use piko_protocol::{
    AgentInput, AgentInputDisposition, AgentInstanceIdentity, AgentInstanceLifecycle, AgentSpec,
    AgentWorkReport,
};
use serde::{Deserialize, Serialize};

use crate::{MessageCommittedV1, ModelStepCommittedV1, TreeEntryRecordedV1};

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

/// Write-time projection of the processing facts on an applied root AgentInput.
/// Start, finish, and outcome live on the root input; there is no Execution
/// aggregate. The correlation anchors are interim until the message,
/// model-step, and usage grains rekey onto `root_input_id` (slice 6.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredRootProcessing {
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
    #[serde(default)]
    pub report: Option<AgentWorkReport>,
    #[serde(default)]
    pub base_message_id: Option<String>,
    #[serde(default)]
    pub tree_base_entry_id: Option<String>,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    #[serde(default)]
    pub detached_recipient_agent_instance_id: Option<String>,
    #[serde(default)]
    pub prompt_assembly_version: u32,
    #[serde(default)]
    pub prompt_digest: String,
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
    /// Processing facts on this input when it is an applied root. Absent for
    /// steers, follow-ups, and cancelled inputs.
    #[serde(default)]
    pub processing: Option<StoredRootProcessing>,
}

impl StoredAgentInput {
    /// True when this input is an applied root whose processing has started
    /// but has no finish fact yet.
    pub fn has_unfinished_processing(&self) -> bool {
        self.processing
            .as_ref()
            .is_some_and(|processing| processing.finished_at.is_none())
    }
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
