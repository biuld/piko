use piko_protocol::{
    HistoryAgentOrigin, HistoryAgentSummary, HistoryAvailability, HistoryCommitSummary,
    HistoryItemKind, HistoryItemRef, HistoryItemSummary, HistoryProvenance,
    HistoryProvenanceFilter, HistoryRelation, HistoryWorkSummary, SessionHistoryOverview,
};
use piko_session_store::{HistoryCommit, HistoryEvent, InspectionBundle, UsageQuery};

pub(super) fn overview(
    session_id: &str,
    bundle: &InspectionBundle,
    offset: usize,
    limit: usize,
) -> SessionHistoryOverview {
    let current = &bundle.current;
    let work_count_by_agent = current
        .agent_inputs
        .values()
        .filter(|input| input.root_input_id.as_deref() == Some(input.input.input_id.as_str()))
        .fold(
            std::collections::HashMap::<&str, u32>::new(),
            |mut counts, input| {
                *counts.entry(&input.input.agent_instance_id).or_default() += 1;
                counts
            },
        );
    let agents = current
        .agents
        .values()
        .map(|agent| HistoryAgentSummary {
            agent_instance_id: agent.identity.agent_instance_id.clone(),
            agent_spec_id: agent.identity.agent_spec_id.clone(),
            parent_agent_instance_id: agent.identity.parent_agent_instance_id.clone(),
            lifecycle: agent.lifecycle,
            work_count: work_count_by_agent
                .get(agent.identity.agent_instance_id.as_str())
                .copied()
                .unwrap_or(0),
            origin: bundle
                .history
                .child_origins
                .get(&agent.identity.agent_instance_id)
                .map(|origin| HistoryAgentOrigin {
                    parent_agent_instance_id: origin.parent_agent_instance_id.clone(),
                    parent_root_input_id: origin.parent_root_input_id.clone(),
                    origin_model_step_id: origin.origin_model_step_id.clone(),
                    origin_tool_call_id: origin.origin_tool_call_id.clone(),
                }),
            origin_availability: agent_origin_availability(agent, bundle),
        })
        .collect();
    let mut works = current
        .agent_inputs
        .values()
        .filter(|stored| stored.root_input_id.as_deref() == Some(stored.input.input_id.as_str()))
        .map(|stored| {
            let root = &stored.input.input_id;
            let processing = stored.processing.as_ref();
            let steps = current
                .model_steps
                .values()
                .filter(|step| step.data.root_input_id == *root)
                .collect::<Vec<_>>();
            let tool_count = steps
                .iter()
                .map(|step| step.data.tool_call_message_ids.len() as u32)
                .sum();
            let message_count = current
                .messages
                .values()
                .filter(|message| message.data.root_input_id.as_deref() == Some(root))
                .count() as u32;
            let usage = current.accounting.summarize(&UsageQuery {
                root_input_id: Some(root.clone()),
                incurred_only: true,
                ..UsageQuery::default()
            });
            HistoryWorkSummary {
                root_input_id: root.clone(),
                agent_instance_id: stored.input.agent_instance_id.clone(),
                origin: stored.input.origin,
                input_preview: stored.input.preview(),
                started_at: processing.map(|value| value.started_at),
                finished_at: processing.and_then(|value| value.finished_at),
                outcome: processing
                    .and_then(|value| value.report.as_ref())
                    .map(|report| report.outcome.status()),
                step_count: steps.len() as u32,
                tool_count,
                message_count,
                usage: (usage.fact_count > 0).then_some(usage.usage),
            }
        })
        .collect::<Vec<_>>();
    works.sort_by(|left, right| {
        let position = |root: &str| {
            bundle
                .history
                .work_commit_indexes
                .get(root)
                .and_then(|indexes| indexes.first())
        };
        position(&right.root_input_id)
            .cmp(&position(&left.root_input_id))
            .then_with(|| right.root_input_id.cmp(&left.root_input_id))
    });
    let total = works.len();
    let works = works
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next = offset.saturating_add(works.len());
    SessionHistoryOverview {
        session_id: session_id.to_string(),
        cwd: current.cwd.clone().unwrap_or_default(),
        name: current.name.clone(),
        revision: bundle.revision,
        agents,
        works,
        next_cursor: (next < total).then(|| format!("work:{}:{next}", bundle.revision)),
    }
}

pub(super) fn commit_summary(
    commit: &HistoryCommit,
    filter: HistoryProvenanceFilter,
    snapshot_revision: u64,
    bundle: &InspectionBundle,
) -> Option<HistoryCommitSummary> {
    let events = commit
        .events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches_filter(event, filter))
        .map(|(index, event)| event_summary(commit, index, event, snapshot_revision, bundle))
        .collect::<Vec<_>>();
    if events.is_empty() {
        return None;
    }
    Some(HistoryCommitSummary {
        revision: commit.revision,
        commit_id: commit.commit_id.clone(),
        committed_at: commit.committed_at,
        producer: commit.producer.clone(),
        causation_id: commit.causation_id.clone(),
        correlation_id: commit.correlation_id.clone(),
        events,
    })
}

pub(super) fn event_summary(
    commit: &HistoryCommit,
    index: usize,
    event: &HistoryEvent,
    snapshot_revision: u64,
    bundle: &InspectionBundle,
) -> HistoryItemSummary {
    let provenance = provenance(event);
    let entity = event.entity_id.clone();
    let kind = kind(event);
    let tool_call_id = tool_call_id(event, &kind, bundle);
    let relation = HistoryRelation {
        agent_instance_id: event.agent_instance_id.clone(),
        root_input_id: event.root_input_id.clone(),
        model_step_id: event.model_step_id.clone(),
        input_id: kind.0.contains("input").then_some(entity.clone()).flatten(),
        message_id: (kind.0 == "message").then_some(entity.clone()).flatten(),
        tool_call_id,
    };
    HistoryItemSummary {
        item_ref: HistoryItemRef {
            revision: snapshot_revision,
            token: format!("event:{}:{index}", commit.revision),
        },
        revision: commit.revision,
        event_index: index as u32,
        committed_at: commit.committed_at,
        kind,
        provenance,
        availability: HistoryAvailability::Available,
        relation,
        summary: event.summary.clone(),
        has_detail: true,
        children: Vec::new(),
    }
}

fn agent_origin_availability(
    agent: &piko_session_store::StoredAgent,
    bundle: &InspectionBundle,
) -> HistoryAvailability {
    if agent.identity.parent_agent_instance_id.is_none() {
        return HistoryAvailability::Available;
    }
    if bundle
        .history
        .child_origins
        .contains_key(&agent.identity.agent_instance_id)
    {
        HistoryAvailability::Available
    } else {
        HistoryAvailability::Unavailable {
            reason: "exact origin was not recorded".into(),
        }
    }
}

fn tool_call_id(
    event: &HistoryEvent,
    kind: &HistoryItemKind,
    bundle: &InspectionBundle,
) -> Option<String> {
    if kind.0 == "tool_call" || kind.0 == "agent_origin" {
        return event.entity_id.clone();
    }
    if kind.0 != "message" {
        return None;
    }
    let message_id = event.entity_id.as_deref()?;
    match &bundle.current.messages.get(message_id)?.data.message {
        piko_protocol::Message::ToolCall { id, .. } => Some(id.clone()),
        piko_protocol::Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
        _ => None,
    }
}

pub(super) fn provenance(event: &HistoryEvent) -> HistoryProvenance {
    match event.provenance {
        piko_session_store::HistoryProvenance::Fact => HistoryProvenance::Fact,
        piko_session_store::HistoryProvenance::Diagnostic => HistoryProvenance::Diagnostic,
    }
}

fn matches_filter(event: &HistoryEvent, filter: HistoryProvenanceFilter) -> bool {
    matches!(filter, HistoryProvenanceFilter::All)
        || matches!(
            (filter, event.provenance),
            (
                HistoryProvenanceFilter::Facts,
                piko_session_store::HistoryProvenance::Fact
            ) | (
                HistoryProvenanceFilter::Diagnostics,
                piko_session_store::HistoryProvenance::Diagnostic
            )
        )
}

fn kind(event: &HistoryEvent) -> HistoryItemKind {
    let name = match event.event_type.as_str() {
        "message_committed" => "message",
        "agent_input_admitted_v1"
        | "agent_input_disposition_changed_v1"
        | "agent_input_applied_v1" => "input",
        "model_step_committed" => "model_step",
        "agent_origin_recorded_v1" => "agent_origin",
        "usage_recorded" | "usage_corrected" => "usage",
        "agent_input_processing_finished_v1" => "report",
        "tree_entry_recorded" => "tree_entry",
        "trajectory.assembly" => "prompt_assembly",
        "trajectory.tool_call" => "tool_call",
        value if value.starts_with("trajectory.") => "diagnostic",
        value => value,
    };
    HistoryItemKind::new(name)
}
