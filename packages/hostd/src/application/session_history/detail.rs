use piko_protocol::{HistoryAvailability, HistoryItemContent, HistoryItemDetail, HistoryItemRef};
use piko_session_store::{HistoryEvent, InspectionBundle};

use super::mapping::provenance;
use crate::api::ProtocolError;

pub(super) fn resolve(
    item_ref: &HistoryItemRef,
    bundle: &InspectionBundle,
) -> Result<HistoryItemDetail, ProtocolError> {
    let (revision, event_index) = parse_ref(item_ref)?;
    let commit = bundle
        .history
        .commits
        .iter()
        .find(|commit| commit.revision == revision)
        .ok_or_else(|| not_found(item_ref))?;
    let event = commit
        .events
        .get(event_index)
        .ok_or_else(|| not_found(item_ref))?;
    let content = fact_content(event, bundle).or_else(|| diagnostic_content(event, bundle));
    if content.is_none()
        && matches!(
            event.event_type.as_str(),
            "message_committed"
                | "agent_input_admitted_v1"
                | "agent_input_disposition_changed_v1"
                | "agent_input_applied_v1"
                | "model_step_committed"
                | "usage_recorded"
                | "agent_input_processing_finished_v1"
                | "tree_entry_recorded"
                | "agent_origin_recorded_v1"
        )
    {
        return Err(ProtocolError::InvalidCommand(format!(
            "history integrity failure: {} references a missing canonical entity",
            event.event_id
        )));
    }
    Ok(HistoryItemDetail {
        item_ref: item_ref.clone(),
        provenance: provenance(event),
        availability: if content.is_some() {
            HistoryAvailability::Available
        } else {
            HistoryAvailability::Unavailable {
                reason: "canonical detail is not retained for this event".into(),
            }
        },
        content,
    })
}

fn parse_ref(item_ref: &HistoryItemRef) -> Result<(u64, usize), ProtocolError> {
    let mut fields = item_ref.token.split(':');
    let valid = fields.next() == Some("event");
    let revision = fields.next().and_then(|value| value.parse().ok());
    let index = fields.next().and_then(|value| value.parse().ok());
    if !valid
        || fields.next().is_some()
        || revision.is_none()
        || index.is_none()
        || revision.is_some_and(|revision| revision > item_ref.revision)
        || item_ref.token.len() > 128
    {
        return Err(not_found(item_ref));
    }
    Ok((revision.unwrap_or_default(), index.unwrap_or(usize::MAX)))
}

fn fact_content(event: &HistoryEvent, bundle: &InspectionBundle) -> Option<HistoryItemContent> {
    let id = event.entity_id.as_deref().unwrap_or_default();
    match event.event_type.as_str() {
        "agent_origin_recorded_v1" => bundle
            .history
            .child_origins
            .get(event.agent_instance_id.as_deref()?)
            .and_then(|origin| serde_json::to_value(origin).ok())
            .map(|value| HistoryItemContent::Structured { value }),
        "message_committed" => {
            bundle
                .current
                .messages
                .get(id)
                .map(|stored| HistoryItemContent::Message {
                    message_id: id.to_string(),
                    message: stored.data.message.clone(),
                })
        }
        "agent_input_admitted_v1"
        | "agent_input_disposition_changed_v1"
        | "agent_input_applied_v1" => {
            bundle
                .current
                .agent_inputs
                .get(id)
                .map(|stored| HistoryItemContent::Input {
                    input: stored.input.clone(),
                })
        }
        "model_step_committed" => {
            bundle
                .current
                .model_steps
                .get(id)
                .map(|stored| HistoryItemContent::ModelStep {
                    boundary: piko_protocol::ModelStepBoundary {
                        session_id: bundle.current.session_id.clone().unwrap_or_default(),
                        root_input_id: stored.data.root_input_id.clone(),
                        agent_instance_id: stored.data.agent_instance_id.clone(),
                        model_step_id: stored.data.model_step_id.clone(),
                        step_index: stored.data.step_index,
                        started_at: stored.data.started_at,
                        finished_at: stored.data.finished_at,
                        outcome: stored.data.outcome,
                        assistant_message_id: stored.data.assistant_message_id.clone(),
                        tool_call_message_ids: stored.data.tool_call_message_ids.clone(),
                    },
                })
        }
        "usage_recorded" => {
            bundle
                .current
                .accounting
                .fact(id)
                .map(|fact| HistoryItemContent::Usage {
                    usage: fact.effective_usage.clone(),
                })
        }
        "agent_input_processing_finished_v1" => bundle
            .current
            .agent_inputs
            .values()
            .filter_map(|input| input.processing.as_ref()?.report.as_ref())
            .find(|report| report.report_id == id)
            .cloned()
            .map(|report| HistoryItemContent::Report { report }),
        "tree_entry_recorded" => bundle
            .current
            .tree_entries
            .get(id)
            .and_then(|stored| serde_json::from_value(stored.data.payload.clone()).ok())
            .map(|entry| HistoryItemContent::TreeEntry { entry }),
        _ => event.transition.as_ref().and_then(|transition| {
            serde_json::to_value(transition)
                .ok()
                .map(|value| HistoryItemContent::Structured { value })
        }),
    }
}

fn diagnostic_content(
    event: &HistoryEvent,
    bundle: &InspectionBundle,
) -> Option<HistoryItemContent> {
    let root = event.root_input_id.as_deref()?;
    let run = bundle.trajectory.runs.get(root)?;
    if event.event_type == "trajectory.assembly" {
        return run
            .assembly
            .clone()
            .map(|assembly| HistoryItemContent::PromptAssembly {
                assembly: Box::new(assembly),
            });
    }
    let id = event.entity_id.as_deref();
    run.records
        .iter()
        .find(|record| record_id(record) == id)
        .cloned()
        .map(|record| HistoryItemContent::DiagnosticRecord {
            record: Box::new(record),
        })
}

fn record_id(record: &piko_protocol::TrajectoryRecord) -> Option<&str> {
    match record {
        piko_protocol::TrajectoryRecord::ModelStep(value) => Some(&value.step_id),
        piko_protocol::TrajectoryRecord::ToolCall(value) => Some(&value.call_id),
        piko_protocol::TrajectoryRecord::ChildRun(value) => Some(&value.child_agent_instance_id),
        piko_protocol::TrajectoryRecord::Assembly(value) => Some(&value.identity.root_input_id),
        piko_protocol::TrajectoryRecord::SystemNotification(value) => {
            Some(&value.identity.root_input_id)
        }
        piko_protocol::TrajectoryRecord::Terminal(value) => Some(&value.identity.root_input_id),
    }
}

fn not_found(item_ref: &HistoryItemRef) -> ProtocolError {
    ProtocolError::InvalidCommand(format!("history item {} not found", item_ref.token))
}
