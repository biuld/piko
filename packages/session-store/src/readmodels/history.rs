//! Ordered, body-light session history projection (F-52 / D-69).
//!
//! The projection preserves commit/event order and causal identifiers while
//! leaving large canonical bodies in `current.json` and diagnostic bodies in
//! `trajectory.json`.

use std::collections::BTreeMap;
use std::path::Path;

use piko_protocol::{
    AgentInputDisposition, AgentInstanceLifecycle, AgentWorkProcessingStatus, Message,
    PendingActionSummary, Usage,
};
use serde::{Deserialize, Serialize};

use super::files::{self, READ_MODEL_SCHEMA, atomic_json};
use crate::Result;
use crate::journal::DurableCommit;
use crate::schema::EventData;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProvenance {
    Fact,
    Diagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryTransition {
    InputAdmitted {
        disposition: AgentInputDisposition,
    },
    InputDispositionChanged {
        disposition: AgentInputDisposition,
    },
    PendingActionRequested {
        action: PendingActionSummary,
    },
    PendingActionResolved,
    InterruptRequested,
    AgentLifecycleChanged {
        lifecycle: AgentInstanceLifecycle,
    },
    BranchSelected,
    UsageCorrected {
        usage_id: String,
        reason: String,
        replacement: Usage,
    },
    InboxReportConsumed,
    WorkFinished {
        status: AgentWorkProcessingStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub event_id: String,
    pub event_type: String,
    pub provenance: HistoryProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<HistoryTransition>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommit {
    pub revision: u64,
    pub commit_id: String,
    pub committed_at: i64,
    pub producer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub events: Vec<HistoryEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryProjection {
    pub revision: u64,
    pub commits: Vec<HistoryCommit>,
    #[serde(default)]
    pub work_commit_indexes: BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    pub agent_commit_indexes: BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    pub message_to_step: BTreeMap<String, String>,
    #[serde(default)]
    pub tool_call_to_step: BTreeMap<String, String>,
    #[serde(default)]
    pub child_origins: BTreeMap<String, crate::AgentOriginRecordedV1>,
    pub usage_relations: BTreeMap<String, (String, String, String)>,
    pub report_roots: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFile {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub through_revision: u64,
    pub through_checksum: String,
    pub projection: HistoryProjection,
}

pub(crate) fn write(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    projection: &HistoryProjection,
    checksum: &str,
) -> Result<()> {
    atomic_json(
        &files::history_path(path),
        &HistoryFile {
            schema_version: READ_MODEL_SCHEMA,
            session_id: session_id.to_string(),
            journal_generation: journal_generation.to_string(),
            through_revision: projection.revision,
            through_checksum: checksum.to_string(),
            projection: projection.clone(),
        },
    )
}

pub(crate) fn load(path: &Path) -> Result<Option<HistoryFile>> {
    files::load_json(&files::history_path(path))
}

pub fn apply_commit(projection: &mut HistoryProjection, commit: &DurableCommit) {
    let commit_index = projection.commits.len();
    let messages = commit
        .events
        .iter()
        .filter_map(|raw| match raw.decode().ok().flatten()? {
            EventData::MessageCommitted(message) => Some((message.message_id, message.message)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    let mut events = commit
        .events
        .iter()
        .map(|raw| history_event(raw, projection, &messages))
        .collect::<Vec<_>>();
    // ModelStep facts can follow their messages in the same atomic commit.
    // Resolve only after all persisted declarations have populated the indexes.
    for event in &mut events {
        if event.event_type == "message_committed" {
            event.model_step_id = event
                .entity_id
                .as_ref()
                .and_then(|id| projection.message_to_step.get(id))
                .cloned();
            if event.model_step_id.is_none()
                && let Some(Message::ToolResult { tool_call_id, .. }) =
                    event.entity_id.as_ref().and_then(|id| messages.get(id))
            {
                event.model_step_id = projection.tool_call_to_step.get(tool_call_id).cloned();
            }
        }
    }

    let mut work_ids = events
        .iter()
        .filter(|event| event.provenance == HistoryProvenance::Fact)
        .filter_map(|event| event.root_input_id.clone())
        .collect::<Vec<_>>();
    work_ids.sort();
    work_ids.dedup();
    for root_input_id in work_ids {
        projection
            .work_commit_indexes
            .entry(root_input_id)
            .or_default()
            .push(commit_index);
    }

    let mut agent_ids = events
        .iter()
        .filter(|event| event.provenance == HistoryProvenance::Fact)
        .filter_map(|event| event.agent_instance_id.clone())
        .collect::<Vec<_>>();
    agent_ids.sort();
    agent_ids.dedup();
    for agent_instance_id in agent_ids {
        projection
            .agent_commit_indexes
            .entry(agent_instance_id)
            .or_default()
            .push(commit_index);
    }

    projection.commits.push(HistoryCommit {
        revision: commit.revision,
        commit_id: commit.commit_id.clone(),
        committed_at: commit.committed_at,
        producer: commit.producer.component.clone(),
        causation_id: commit.causation_id.clone(),
        correlation_id: commit.correlation_id.clone(),
        events,
    });
    projection.revision = commit.revision;
}

fn history_event(
    raw: &crate::RawEvent,
    projection: &mut HistoryProjection,
    messages: &BTreeMap<String, Message>,
) -> HistoryEvent {
    let provenance = if raw.compatibility.ignorable {
        HistoryProvenance::Diagnostic
    } else {
        HistoryProvenance::Fact
    };
    let mut event = HistoryEvent {
        event_id: raw.event_id.clone(),
        event_type: raw.event_type.clone(),
        provenance,
        agent_instance_id: None,
        root_input_id: None,
        model_step_id: None,
        entity_id: None,
        transition: None,
        summary: raw.event_type.clone(),
    };
    let Ok(Some(decoded)) = raw.decode() else {
        if matches!(
            raw.event_type.as_str(),
            piko_protocol::TRAJECTORY_EVENT_ASSEMBLY
                | piko_protocol::TRAJECTORY_EVENT_MODEL_STEP
                | piko_protocol::TRAJECTORY_EVENT_TOOL_CALL
                | piko_protocol::TRAJECTORY_EVENT_CHILD_RUN
                | piko_protocol::TRAJECTORY_EVENT_SYSTEM_NOTIFICATION
                | piko_protocol::TRAJECTORY_EVENT_TERMINAL
        ) {
            attach_optional_identity(&mut event, &raw.payload);
        }
        return event;
    };
    match decoded {
        EventData::SessionCreated { root, .. } => {
            event.agent_instance_id = Some(root.agent_instance_id.clone());
            event.entity_id = Some(root.agent_instance_id);
            event.summary = "session created".into();
        }
        EventData::MessageCommitted(message) => {
            event.agent_instance_id = Some(message.agent_instance_id);
            event.root_input_id = message.root_input_id;
            event.entity_id = Some(message.message_id);
            event.summary = format!("{} message committed", message.message.role());
        }
        EventData::AgentInputAdmittedV1(admitted) => {
            event.agent_instance_id = Some(admitted.input.agent_instance_id);
            event.root_input_id = if admitted.disposition == AgentInputDisposition::AppliedAsRoot {
                Some(admitted.input.input_id.clone())
            } else {
                admitted.root_input_id
            };
            event.entity_id = Some(admitted.input.input_id);
            event.transition = Some(HistoryTransition::InputAdmitted {
                disposition: admitted.disposition,
            });
            event.summary = format!("input admitted as {:?}", admitted.disposition);
        }
        EventData::AgentInputDispositionChangedV1(changed) => {
            event.agent_instance_id = Some(changed.agent_instance_id);
            event.root_input_id = changed.root_input_id;
            event.model_step_id = changed.model_step_id;
            event.entity_id = Some(changed.input_id);
            event.transition = Some(HistoryTransition::InputDispositionChanged {
                disposition: changed.disposition,
            });
            event.summary = format!("input became {:?}", changed.disposition);
        }
        EventData::AgentInputAppliedV1(applied) => {
            event.agent_instance_id = Some(applied.agent_instance_id);
            event.root_input_id = Some(applied.root_input_id);
            event.entity_id = Some(applied.input_id);
            event.summary = "input applied to transcript".into();
        }
        EventData::AgentInputProcessingStartedV1(started) => {
            event.agent_instance_id = Some(started.agent_instance_id);
            event.root_input_id = Some(started.root_input_id.clone());
            event.entity_id = Some(started.root_input_id);
            event.summary = "work processing started".into();
        }
        EventData::AgentInputProcessingFinishedV1(finished) => {
            let status = finished.report.outcome.status();
            event.agent_instance_id = Some(finished.agent_instance_id);
            event.root_input_id = Some(finished.root_input_id.clone());
            event.entity_id = Some(finished.report.report_id);
            event.summary = format!("work processing finished: {status:?}");
            event.transition = Some(HistoryTransition::WorkFinished { status });
        }
        EventData::ModelStepCommitted(step) => {
            projection.message_to_step.insert(
                step.assistant_message_id.clone(),
                step.model_step_id.clone(),
            );
            for message_id in &step.tool_call_message_ids {
                projection
                    .message_to_step
                    .insert(message_id.clone(), step.model_step_id.clone());
                if let Some(Message::ToolCall { id, .. }) = messages.get(message_id) {
                    projection
                        .tool_call_to_step
                        .insert(id.clone(), step.model_step_id.clone());
                }
            }
            event.agent_instance_id = Some(step.agent_instance_id);
            event.root_input_id = Some(step.root_input_id);
            event.model_step_id = Some(step.model_step_id.clone());
            event.entity_id = Some(step.model_step_id);
            event.summary = format!(
                "model step {} committed: {:?}",
                step.step_index, step.outcome
            );
        }
        EventData::AgentPendingActionRequestedV1(requested) => {
            event.agent_instance_id = Some(requested.agent_instance_id);
            event.root_input_id = Some(requested.root_input_id);
            event.entity_id = Some(requested.action.action_id.clone());
            event.transition = Some(HistoryTransition::PendingActionRequested {
                action: requested.action,
            });
            event.summary = "pending action requested".into();
        }
        EventData::AgentPendingActionResolvedV1(resolved) => {
            event.agent_instance_id = Some(resolved.agent_instance_id);
            event.root_input_id = Some(resolved.root_input_id);
            event.entity_id = Some(resolved.action_id);
            event.transition = Some(HistoryTransition::PendingActionResolved);
            event.summary = "pending action resolved".into();
        }
        EventData::AgentInterruptRequestedV1(interrupt) => {
            event.agent_instance_id = Some(interrupt.agent_instance_id);
            event.root_input_id = Some(interrupt.root_input_id.clone());
            event.entity_id = Some(interrupt.root_input_id);
            event.transition = Some(HistoryTransition::InterruptRequested);
            event.summary = "agent interrupt requested".into();
        }
        EventData::AgentCreated { identity, .. } => {
            event.agent_instance_id = Some(identity.agent_instance_id.clone());
            event.entity_id = Some(identity.agent_instance_id);
            event.summary = "agent created".into();
        }
        EventData::AgentOriginRecordedV1(origin) => {
            event.agent_instance_id = Some(origin.child_agent_instance_id.clone());
            event.root_input_id = Some(origin.parent_root_input_id.clone());
            event.model_step_id = Some(origin.origin_model_step_id.clone());
            event.entity_id = Some(origin.origin_tool_call_id.clone());
            event.summary = "child agent origin recorded".into();
            projection
                .child_origins
                .insert(origin.child_agent_instance_id.clone(), origin);
        }
        EventData::AgentLifecycleChanged {
            agent_instance_id,
            lifecycle,
            ..
        } => {
            event.agent_instance_id = Some(agent_instance_id.clone());
            event.entity_id = Some(agent_instance_id);
            event.transition = Some(HistoryTransition::AgentLifecycleChanged { lifecycle });
            event.summary = format!("agent lifecycle changed: {lifecycle:?}");
        }
        EventData::BranchSelected {
            selected_tree_entry_id,
            ..
        } => {
            event.entity_id = selected_tree_entry_id;
            event.transition = Some(HistoryTransition::BranchSelected);
            event.summary = "session branch selected".into();
        }
        EventData::UsageRecorded(usage) => {
            projection.usage_relations.insert(
                usage.usage_id.clone(),
                (
                    usage.attribution.agent_instance_id.clone(),
                    usage.attribution.root_input_id.clone(),
                    usage.attribution.model_step_id.clone(),
                ),
            );
            event.agent_instance_id = Some(usage.attribution.agent_instance_id);
            event.root_input_id = Some(usage.attribution.root_input_id);
            event.model_step_id = Some(usage.attribution.model_step_id);
            event.entity_id = Some(usage.usage_id);
            event.summary = format!("usage recorded for {}/{}", usage.provider, usage.model_id);
        }
        EventData::UsageCorrected(correction) => {
            if let Some((agent, root, step)) = projection.usage_relations.get(&correction.usage_id)
            {
                event.agent_instance_id = Some(agent.clone());
                event.root_input_id = Some(root.clone());
                event.model_step_id = Some(step.clone());
            }
            event.entity_id = Some(correction.correction_id);
            event.transition = Some(HistoryTransition::UsageCorrected {
                usage_id: correction.usage_id,
                reason: correction.reason,
                replacement: correction.replacement,
            });
            event.summary = "usage corrected".into();
        }
        EventData::InboxReportCommitted { item } => {
            projection
                .report_roots
                .insert(item.report_id.clone(), item.report.root_input_id.clone());
            event.agent_instance_id = Some(item.recipient_agent_instance_id);
            event.root_input_id = Some(item.report.root_input_id);
            event.entity_id = Some(item.report_id);
            event.summary = "agent report committed to inbox".into();
        }
        EventData::InboxReportConsumed {
            report_id,
            recipient_agent_instance_id,
            ..
        } => {
            event.agent_instance_id = Some(recipient_agent_instance_id);
            event.root_input_id = projection.report_roots.get(&report_id).cloned();
            event.entity_id = Some(report_id);
            event.transition = Some(HistoryTransition::InboxReportConsumed);
            event.summary = "agent report consumed".into();
        }
        EventData::CompactionRecorded(compaction) => {
            event.entity_id = Some(compaction.compaction_id);
            event.summary = "context compacted".into();
        }
        EventData::TreeEntryRecorded(entry) => {
            event.entity_id = Some(entry.entry_id);
            event.summary = format!("{} tree entry recorded", entry.entry_type);
        }
        _ => {}
    }
    event
}

fn attach_optional_identity(event: &mut HistoryEvent, payload: &serde_json::Value) {
    let identity = payload.get("identity");
    event.agent_instance_id = identity
        .and_then(|value| value.get("agentInstanceId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    event.root_input_id = identity
        .and_then(|value| value.get("rootInputId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    event.model_step_id = payload
        .get("stepId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    event.entity_id = event.model_step_id.clone().or_else(|| {
        payload
            .get("callId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    });
}
