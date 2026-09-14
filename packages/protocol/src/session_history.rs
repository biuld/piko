//! Read-only session trajectory DTOs (F-52 / D-69).

use serde::{Deserialize, Serialize};

use crate::{AgentInstanceLifecycle, Message, Usage};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProvenance {
    Fact,
    Diagnostic,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// One flat stream row: a durable input, assistant message, or tool call of
/// one agent in journal order across works.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStreamItem {
    pub item_ref: HistoryItemRef,
    pub revision: u64,
    pub event_index: u32,
    pub committed_at: i64,
    pub kind: HistoryItemKind,
    /// Semantic role badge for the row ("USER", "ASSISTANT", "TOOL",
    /// "RESULT", "CONTEXT", "STEP 3", …).
    pub badge: String,
    pub relation: HistoryRelation,
    /// Display-ready content preview; the full body is fetched on open.
    pub summary: String,
    /// Recorded status/outcome label, when a durable fact carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Recorded duration in milliseconds, when a diagnostic observation
    /// joined by persisted identity carries it. Absence is not zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// True when diagnostic detail (payload, result, timing) can be fetched.
    pub has_detail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStreamPage {
    pub session_id: String,
    pub agent_instance_id: String,
    pub revision: u64,
    pub items: Vec<HistoryStreamItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryLaneBlockKind {
    ModelStep,
    ToolCall,
}

/// One activity block in the lane strip. Timing comes from optional
/// trajectory diagnostics; absent timing degrades the strip to sequence mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLaneBlock {
    pub kind: HistoryLaneBlockKind,
    /// The stream row this block refers to.
    pub reference: HistoryItemRef,
    pub label: String,
    pub status: String,
    /// Journal-order position used when timing is unavailable.
    pub sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLaneSummary {
    pub session_id: String,
    pub agent_instance_id: String,
    pub revision: u64,
    pub blocks: Vec<HistoryLaneBlock>,
    /// False when no diagnostic timing joined at all; the strip renders in
    /// sequence mode and says so.
    pub timing_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryItemContent {
    Input {
        input: crate::AgentInput,
    },
    Message {
        message_id: String,
        message: Message,
    },
    ModelStep {
        boundary: crate::ModelStepBoundary,
    },
    Usage {
        usage: Usage,
    },
    PromptAssembly {
        assembly: Box<crate::TrajectoryAssemblyRecord>,
    },
    DiagnosticRecord {
        record: Box<crate::TrajectoryRecord>,
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
    /// Optional trajectory observation joined by persisted identity. Absent
    /// means no diagnostic data exists; it is never fabricated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<Box<crate::TrajectoryRecord>>,
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
            diagnostic: None,
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

    #[test]
    fn lane_summary_degrades_to_sequence_mode() {
        let summary = HistoryLaneSummary {
            session_id: "s".into(),
            agent_instance_id: "a".into(),
            revision: 3,
            blocks: vec![HistoryLaneBlock {
                kind: HistoryLaneBlockKind::ModelStep,
                reference: HistoryItemRef {
                    revision: 3,
                    token: "fact:3:1".into(),
                },
                label: "step 0".into(),
                status: "completed".into(),
                sequence: 0,
                started_at: None,
                duration_ms: None,
            }],
            timing_available: false,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(
            serde_json::from_value::<HistoryLaneSummary>(json).unwrap(),
            summary
        );
    }
}
