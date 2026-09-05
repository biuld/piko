//! Read-only Session History DTOs (F-52 / D-69).

use serde::{Deserialize, Serialize};

use crate::{
    AgentInput, AgentInputOrigin, AgentInstanceLifecycle, AgentWorkProcessingStatus,
    AgentWorkReport, Message, ModelStepBoundary, SessionTreeEntry, TrajectoryAssemblyRecord,
    TrajectoryRecord, Usage,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProvenance {
    Fact,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProvenanceFilter {
    #[default]
    All,
    Facts,
    Diagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HistoryAvailability {
    Available,
    Unavailable { reason: String },
}

/// Open item kind. `name` is stable product vocabulary; unknown names render
/// through the generic TUI path instead of breaking the page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct HistoryItemKind(pub String);

impl HistoryItemKind {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItemRef {
    /// Published snapshot revision used to resolve the token, not event position.
    pub revision: u64,
    pub token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRelation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItemSummary {
    pub item_ref: HistoryItemRef,
    pub revision: u64,
    pub event_index: u32,
    pub committed_at: i64,
    pub kind: HistoryItemKind,
    pub provenance: HistoryProvenance,
    pub availability: HistoryAvailability,
    pub relation: HistoryRelation,
    pub summary: String,
    pub has_detail: bool,
    /// Optional diagnostic observations joined by persisted identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<HistoryItemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAgentSummary {
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_instance_id: Option<String>,
    pub lifecycle: AgentInstanceLifecycle,
    pub work_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<HistoryAgentOrigin>,
    pub origin_availability: HistoryAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAgentOrigin {
    pub parent_agent_instance_id: String,
    pub parent_root_input_id: String,
    pub origin_model_step_id: String,
    pub origin_tool_call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryWorkSummary {
    pub root_input_id: String,
    pub agent_instance_id: String,
    pub origin: AgentInputOrigin,
    pub input_preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<AgentWorkProcessingStatus>,
    pub step_count: u32,
    pub tool_count: u32,
    pub message_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryOverview {
    pub session_id: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub revision: u64,
    pub agents: Vec<HistoryAgentSummary>,
    pub works: Vec<HistoryWorkSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryWorkPage {
    pub session_id: String,
    pub revision: u64,
    pub root_input_id: String,
    pub items: Vec<HistoryItemSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommitSummary {
    pub revision: u64,
    pub commit_id: String,
    pub committed_at: i64,
    pub producer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub events: Vec<HistoryItemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryJournalPage {
    pub session_id: String,
    pub revision: u64,
    pub commits: Vec<HistoryCommitSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// One transcript row in ancestry/tree order, independent from causal work order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTranscriptItem {
    pub item_ref: HistoryItemRef,
    pub kind: HistoryItemKind,
    pub depth: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_step_id: Option<String>,
    pub summary: String,
    pub selected: bool,
    pub off_branch: bool,
    pub has_detail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTranscriptPage {
    pub session_id: String,
    pub revision: u64,
    pub items: Vec<HistoryTranscriptItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryItemContent {
    Input {
        input: AgentInput,
    },
    Message {
        message_id: String,
        message: Message,
    },
    ModelStep {
        boundary: ModelStepBoundary,
    },
    Usage {
        usage: Usage,
    },
    Report {
        report: AgentWorkReport,
    },
    TreeEntry {
        entry: SessionTreeEntry,
    },
    PromptAssembly {
        assembly: Box<TrajectoryAssemblyRecord>,
    },
    DiagnosticRecord {
        record: Box<TrajectoryRecord>,
    },
    Structured {
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItemDetail {
    pub item_ref: HistoryItemRef,
    pub provenance: HistoryProvenance,
    pub availability: HistoryAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<HistoryItemContent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_change_is_a_structured_command_result() {
        let result = crate::CommandResult::HistoryRevisionChanged {
            session_id: "session-1".into(),
            current_revision: 12,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["type"], "history_revision_changed");
        assert!(matches!(
            serde_json::from_value::<crate::CommandResult>(json).unwrap(),
            crate::CommandResult::HistoryRevisionChanged {
                current_revision: 12,
                ..
            }
        ));
    }

    #[test]
    fn unknown_item_kind_and_unavailable_detail_round_trip() {
        let detail = HistoryItemDetail {
            item_ref: HistoryItemRef {
                revision: 7,
                token: "fact:7:2".into(),
            },
            provenance: HistoryProvenance::Fact,
            availability: HistoryAvailability::Unavailable {
                reason: "legacy relation absent".into(),
            },
            content: None,
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            serde_json::from_value::<HistoryItemDetail>(json).unwrap(),
            detail
        );

        let kind = HistoryItemKind::new("future_fact_kind");
        let json = serde_json::to_value(&kind).unwrap();
        assert_eq!(
            serde_json::from_value::<HistoryItemKind>(json).unwrap(),
            kind
        );
    }
}
