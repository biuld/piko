use std::collections::BTreeMap;

use piko_protocol::{
    AgentInboxItem, AgentInput, AgentInputDisposition, AgentInstanceIdentity,
    AgentInstanceLifecycle, AgentSpec, AgentWorkReport, Message, ModelStepOutcome, TodoList, Usage,
};
use serde::{Deserialize, Serialize};

pub(crate) fn validate_extensions(
    scope: &str,
    extensions: &BTreeMap<String, serde_json::Value>,
) -> crate::Result<()> {
    if let Some(key) = extensions.keys().find(|key| {
        let mut parts = key.splitn(2, '/');
        parts.next().is_none_or(str::is_empty) || parts.next().is_none_or(str::is_empty)
    }) {
        return Err(crate::StoreError::InvalidEvent(format!(
            "{scope} extension key is not namespaced: {key}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub required_reader_version: u32,
    pub ignorable: bool,
}

impl Compatibility {
    pub fn required() -> Self {
        Self {
            required_reader_version: crate::READER_VERSION,
            ignorable: false,
        }
    }

    pub fn optional() -> Self {
        Self {
            required_reader_version: crate::READER_VERSION,
            ignorable: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawEvent {
    pub event_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub version: u32,
    pub compatibility: Compatibility,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl RawEvent {
    pub fn new(event_id: impl Into<String>, data: EventData) -> crate::Result<Self> {
        Ok(Self {
            event_id: event_id.into(),
            event_type: data.event_type().to_string(),
            version: 1,
            compatibility: Compatibility::required(),
            payload: serde_json::to_value(data).map_err(|source| crate::StoreError::Json {
                path: "event payload".into(),
                source,
            })?,
            extensions: BTreeMap::new(),
        })
    }

    pub fn optional(
        event_id: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            event_type: event_type.into(),
            version: 1,
            compatibility: Compatibility::optional(),
            payload,
            extensions: BTreeMap::new(),
        }
    }

    pub(crate) fn decode(&self) -> crate::Result<Option<EventData>> {
        validate_extensions("event", &self.extensions)?;
        if self.compatibility.required_reader_version > crate::READER_VERSION {
            return self.unknown();
        }
        if self.version != 1 {
            return self.unknown();
        }
        let known = matches!(
            self.event_type.as_str(),
            "session_created"
                | "message_committed"
                | "branch_selected"
                | "usage_recorded"
                | "usage_corrected"
                | "session_metadata_changed"
                | "agent_created"
                | "agent_lifecycle_changed"
                | "agent_input_admitted_v1"
                | "agent_input_disposition_changed_v1"
                | "agent_input_applied_v1"
                | "agent_input_processing_started_v1"
                | "agent_input_processing_finished_v1"
                | "model_step_committed"
                | "inbox_report_committed"
                | "inbox_report_consumed"
                | "compaction_recorded"
                | "world_state_advanced"
                | "model_continuity_changed"
                | "todo_list_replaced"
                | "session_forked"
                | "tree_entry_recorded"
                | "agent_selected"
        );
        if !known {
            return self.unknown();
        }
        let decoded: EventData =
            serde_json::from_value(self.payload.clone()).map_err(|source| {
                crate::StoreError::InvalidEvent(format!(
                    "cannot decode {} v{}: {source}",
                    self.event_type, self.version
                ))
            })?;
        if decoded.event_type() != self.event_type {
            return Err(crate::StoreError::InvalidEvent(format!(
                "event type {} does not match payload kind {}",
                self.event_type,
                decoded.event_type()
            )));
        }
        Ok(Some(decoded))
    }

    fn unknown(&self) -> crate::Result<Option<EventData>> {
        if self.compatibility.ignorable {
            Ok(None)
        } else {
            Err(crate::StoreError::UpgradeRequired {
                event_type: self.event_type.clone(),
                version: self.version,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum EventData {
    SessionCreated {
        session_id: String,
        cwd: String,
        root: AgentInstanceIdentity,
        created_at: i64,
    },
    MessageCommitted(MessageCommittedV1),
    BranchSelected {
        selected_tree_entry_id: Option<String>,
        root_base_message_id: Option<String>,
    },
    UsageRecorded(UsageRecordedV1),
    UsageCorrected(UsageCorrectedV1),
    SessionMetadataChanged {
        name: Option<String>,
    },
    AgentCreated {
        identity: AgentInstanceIdentity,
        spec: AgentSpec,
        created_at: i64,
    },
    AgentLifecycleChanged {
        agent_instance_id: String,
        lifecycle: AgentInstanceLifecycle,
        changed_at: i64,
    },
    AgentInputAdmittedV1(AgentInputAdmittedV1),
    AgentInputDispositionChangedV1(AgentInputDispositionChangedV1),
    AgentInputAppliedV1(AgentInputAppliedV1),
    AgentInputProcessingStartedV1(AgentInputProcessingStartedV1),
    AgentInputProcessingFinishedV1(AgentInputProcessingFinishedV1),
    ModelStepCommitted(ModelStepCommittedV1),
    InboxReportCommitted {
        item: AgentInboxItem,
    },
    InboxReportConsumed {
        report_id: String,
        recipient_agent_instance_id: String,
        consumed_at: i64,
    },
    CompactionRecorded(CompactionRecordedV1),
    WorldStateAdvanced {
        facts: Option<serde_json::Value>,
    },
    ModelContinuityChanged {
        provider: Option<String>,
        model_id: Option<String>,
    },
    TodoListReplaced {
        agent_instance_id: String,
        todo_list: Option<TodoList>,
    },
    SessionForked(SessionForkedV1),
    TreeEntryRecorded(TreeEntryRecordedV1),
    AgentSelected {
        agent_instance_id: String,
        selected_at: i64,
    },
}

impl EventData {
    fn event_type(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session_created",
            Self::MessageCommitted(_) => "message_committed",
            Self::BranchSelected { .. } => "branch_selected",
            Self::UsageRecorded(_) => "usage_recorded",
            Self::UsageCorrected(_) => "usage_corrected",
            Self::SessionMetadataChanged { .. } => "session_metadata_changed",
            Self::AgentCreated { .. } => "agent_created",
            Self::AgentLifecycleChanged { .. } => "agent_lifecycle_changed",
            Self::AgentInputAdmittedV1(_) => "agent_input_admitted_v1",
            Self::AgentInputDispositionChangedV1(_) => "agent_input_disposition_changed_v1",
            Self::AgentInputAppliedV1(_) => "agent_input_applied_v1",
            Self::AgentInputProcessingStartedV1(_) => "agent_input_processing_started_v1",
            Self::AgentInputProcessingFinishedV1(_) => "agent_input_processing_finished_v1",
            Self::ModelStepCommitted(_) => "model_step_committed",
            Self::InboxReportCommitted { .. } => "inbox_report_committed",
            Self::InboxReportConsumed { .. } => "inbox_report_consumed",
            Self::CompactionRecorded(_) => "compaction_recorded",
            Self::WorldStateAdvanced { .. } => "world_state_advanced",
            Self::ModelContinuityChanged { .. } => "model_continuity_changed",
            Self::TodoListReplaced { .. } => "todo_list_replaced",
            Self::SessionForked(_) => "session_forked",
            Self::TreeEntryRecorded(_) => "tree_entry_recorded",
            Self::AgentSelected { .. } => "agent_selected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageCommittedV1 {
    pub message_id: String,
    pub agent_instance_id: String,
    pub agent_parent_message_id: Option<String>,
    pub tree_parent_entry_id: Option<String>,
    pub execution_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub committed_at: i64,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageAttribution {
    pub session_id: String,
    pub agent_instance_id: String,
    pub turn_id: Option<String>,
    pub execution_id: String,
    pub model_step_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordedV1 {
    pub usage_id: String,
    pub attribution: UsageAttribution,
    pub provider: String,
    pub model_id: String,
    pub api_surface: Option<String>,
    pub pricing_policy_id: Option<String>,
    pub pricing_revision: Option<String>,
    pub usage: Usage,
    pub incurred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageCorrectedV1 {
    pub correction_id: String,
    pub usage_id: String,
    pub replacement: Usage,
    pub reason: String,
}

/// Durable processing-start fact on the root AgentInput. This is the product
/// processing boundary; there is no Execution aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputProcessingStartedV1 {
    pub agent_instance_id: String,
    pub root_input_id: String,
    pub request_id: String,
    /// Interim commit-correlation identity for message, model-step, and usage
    /// grains until they rekey onto `root_input_id` (slice 6.4).
    #[serde(default)]
    pub execution_id: String,
    /// Interim run correlation; equals the root request id in orchd today.
    #[serde(default)]
    pub run_id: String,
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
    pub started_at: i64,
}

/// Durable processing-finish fact on the root AgentInput. The report carries
/// the work outcome; there is no Execution aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputProcessingFinishedV1 {
    pub agent_instance_id: String,
    pub root_input_id: String,
    pub report: AgentWorkReport,
    pub finished_at: i64,
}

/// Durable admission fact for one immutable AgentInput.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputAdmittedV1 {
    pub input: AgentInput,
    pub disposition: AgentInputDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<String>,
    pub admitted_at: i64,
}

/// Durable state transition for an already admitted AgentInput.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputDispositionChangedV1 {
    pub agent_instance_id: String,
    pub input_id: String,
    pub disposition: AgentInputDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_step_id: Option<String>,
    pub changed_at: i64,
}

/// Transcript application of an AgentInput. The user payload is resolved
/// from the admitted input and is never copied into this event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputAppliedV1 {
    pub input_id: String,
    pub message_id: String,
    pub agent_instance_id: String,
    pub agent_parent_message_id: Option<String>,
    pub tree_parent_entry_id: Option<String>,
    pub execution_id: String,
    pub source_turn_id: Option<String>,
    pub committed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelStepCommittedV1 {
    pub model_step_id: String,
    pub step_index: u32,
    pub run_id: String,
    pub execution_id: String,
    pub agent_instance_id: String,
    pub source_turn_id: Option<String>,
    pub assistant_message_id: String,
    #[serde(default)]
    pub tool_call_message_ids: Vec<String>,
    pub outcome: ModelStepOutcome,
    pub started_at: i64,
    pub finished_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionRecordedV1 {
    pub compaction_id: String,
    pub tree_parent_entry_id: Option<String>,
    pub summary: String,
    pub first_retained_entry_id: String,
    pub tokens_before: u64,
    pub committed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionForkedV1 {
    pub source_session_id: String,
    pub source_revision: u64,
    pub source_tree_entry_id: Option<String>,
}

/// Unknown-preserving host tree entry. `payload` is the complete client entry
/// representation; the duplicated core fields support stable validation and
/// indexing without closing the durable schema over every future entry kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntryRecordedV1 {
    pub entry_id: String,
    pub parent_entry_id: Option<String>,
    pub entry_type: String,
    pub timestamp: i64,
    pub payload: serde_json::Value,
}
