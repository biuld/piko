use piko_protocol::{
    HistoryAgentSummary, HistoryItemKind, HistoryItemRef, HistoryLaneBlock, HistoryLaneBlockKind,
    HistoryProvenance, HistoryRelation, HistoryStreamItem, SessionHistoryOverview,
};
use piko_session_store::{HistoryEvent, InspectionBundle};

pub(super) fn overview(session_id: &str, bundle: &InspectionBundle) -> SessionHistoryOverview {
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
    let mut agents: Vec<HistoryAgentSummary> = current
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
        })
        .collect();
    agents.sort_by(|left, right| {
        left.parent_agent_instance_id
            .cmp(&right.parent_agent_instance_id)
            .then_with(|| left.agent_instance_id.cmp(&right.agent_instance_id))
    });
    SessionHistoryOverview {
        session_id: session_id.to_string(),
        cwd: current.cwd.clone().unwrap_or_default(),
        name: current.name.clone(),
        revision: bundle.revision,
        agents,
        next_cursor: None,
    }
}

/// Stream kinds: the flat stream only carries inputs, messages, and model
/// steps. Everything else stays reachable through detail enrichment.
fn is_stream_event(event: &HistoryEvent) -> bool {
    matches!(
        event.event_type.as_str(),
        "message_committed" | "agent_input_admitted_v1" | "model_step_committed"
    )
}

pub(super) fn stream_item(
    commit_revision: u64,
    commit_time: i64,
    index: usize,
    event: &HistoryEvent,
    snapshot_revision: u64,
    bundle: &InspectionBundle,
) -> HistoryStreamItem {
    let kind = kind(event);
    let entity = event.entity_id.clone();
    let tool_call_id = tool_call_id(event, &kind, bundle);
    let relation = HistoryRelation {
        agent_instance_id: event.agent_instance_id.clone(),
        root_input_id: event.root_input_id.clone(),
        model_step_id: event.model_step_id.clone(),
        input_id: kind.0.contains("input").then_some(entity.clone()).flatten(),
        message_id: (kind.0 == "message").then_some(entity.clone()).flatten(),
        tool_call_id,
    };
    let timing = diagnostic_timing(event, &kind, bundle);
    let status = event_status(event, &kind);
    let (badge, summary) = presentation(event, &kind, bundle);
    HistoryStreamItem {
        item_ref: HistoryItemRef {
            revision: snapshot_revision,
            token: format!("event:{}:{index}", commit_revision),
        },
        revision: commit_revision,
        event_index: index as u32,
        committed_at: commit_time,
        kind,
        badge,
        relation,
        summary,
        status,
        duration_ms: timing.duration_ms,
        has_detail: true,
    }
}

/// Semantic role badge plus a display-ready content preview. Journal commit
/// order stays authoritative; only the wording is presentation.
fn presentation(
    event: &HistoryEvent,
    kind: &HistoryItemKind,
    bundle: &InspectionBundle,
) -> (String, String) {
    let entity = event.entity_id.as_deref().unwrap_or_default();
    match kind.0.as_str() {
        "input" => {
            let origin = bundle
                .current
                .agent_inputs
                .get(entity)
                .map(|stored| stored.input.origin);
            let badge = match origin {
                Some(piko_protocol::AgentInputOrigin::Agent) => "AGENT".into(),
                Some(piko_protocol::AgentInputOrigin::System) => "SYSTEM".into(),
                _ => "USER".into(),
            };
            let preview = bundle
                .current
                .agent_inputs
                .get(entity)
                .map(|stored| stored.input.preview())
                .unwrap_or_else(|| event.summary.clone());
            (badge, preview)
        }
        "message" => {
            let stored = event
                .entity_id
                .as_deref()
                .and_then(|id| bundle.current.messages.get(id));
            let message = stored.map(|stored| &stored.data.message);
            let badge = match message {
                Some(piko_protocol::Message::Assistant { .. }) => "ASSISTANT".into(),
                Some(piko_protocol::Message::ToolCall { .. }) => "TOOL".into(),
                Some(piko_protocol::Message::ToolResult { .. }) => "RESULT".into(),
                Some(piko_protocol::Message::Context { .. }) => "CONTEXT".into(),
                _ => "USER".into(),
            };
            let preview = message
                .map(message_preview)
                .unwrap_or_else(|| event.summary.clone());
            (badge, preview)
        }
        "model_step" => {
            let stored = event
                .entity_id
                .as_deref()
                .and_then(|id| bundle.current.model_steps.get(id));
            let step_index = stored.map(|stored| stored.data.step_index).unwrap_or(0);
            // step_index is 1-based and matches the step id suffix (`step_6`).
            let badge = format!("STEP {}", step_index);
            let outcome = stored
                .map(|stored| step_outcome_word(stored.data.outcome))
                .unwrap_or_else(|| event.summary.as_str())
                .to_string();
            (badge, outcome)
        }
        _ => ("FACT".into(), event.summary.clone()),
    }
}

fn step_outcome_word(outcome: piko_protocol::ModelStepOutcome) -> &'static str {
    match outcome {
        piko_protocol::ModelStepOutcome::Completed => "completed",
        piko_protocol::ModelStepOutcome::ToolCalls => "tool calls",
        piko_protocol::ModelStepOutcome::Failed => "failed",
        piko_protocol::ModelStepOutcome::Cancelled => "cancelled",
    }
}

fn message_preview(message: &piko_protocol::Message) -> String {
    let first_line = |text: &str| {
        text.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect::<String>()
    };
    match message {
        piko_protocol::Message::User { content, .. }
        | piko_protocol::Message::Context { content, .. } => match content {
            piko_protocol::MessageContent::String(text) => first_line(text),
            piko_protocol::MessageContent::Blocks(blocks) => blocks
                .iter()
                .find_map(|block| match block {
                    piko_protocol::ContentBlock::Text { text } => Some(first_line(text)),
                    _ => None,
                })
                .unwrap_or_default(),
        },
        piko_protocol::Message::Assistant { content, .. } => content
            .iter()
            .find_map(|block| match block {
                piko_protocol::ContentBlock::Text { text } => Some(first_line(text)),
                piko_protocol::ContentBlock::Thinking { thinking, .. } => {
                    Some(format!("[thinking] {}", first_line(thinking)))
                }
                _ => None,
            })
            .unwrap_or_default(),
        piko_protocol::Message::ToolCall {
            name, arguments, ..
        } => {
            let args = serde_json::to_string(arguments).unwrap_or_default();
            let args = args.chars().take(90).collect::<String>();
            format!("{name} {args}")
        }
        piko_protocol::Message::ToolResult {
            content, is_error, ..
        } => {
            let text = content
                .iter()
                .find_map(|block| match block {
                    piko_protocol::ContentBlock::Text { text } => Some(first_line(text)),
                    _ => None,
                })
                .unwrap_or_default();
            if *is_error == Some(true) {
                format!("[error] {text}")
            } else {
                text
            }
        }
    }
}

pub(super) fn is_stream_event_public(event: &HistoryEvent) -> bool {
    is_stream_event(event)
}

pub(super) struct DiagnosticTiming {
    pub(super) duration_ms: Option<u64>,
}

pub(super) fn diagnostic_timing(
    event: &HistoryEvent,
    kind: &HistoryItemKind,
    bundle: &InspectionBundle,
) -> DiagnosticTiming {
    let root = event.root_input_id.as_deref();
    let record_id = match (kind.0.as_str(), event.entity_id.as_deref()) {
        ("model_step", Some(id)) => Some(id),
        ("message", Some(id)) => match bundle.current.messages.get(id) {
            Some(stored) => match &stored.data.message {
                piko_protocol::Message::ToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            },
            None => None,
        },
        _ => None,
    };
    let (Some(root), Some(record_id)) = (root, record_id) else {
        return DiagnosticTiming { duration_ms: None };
    };
    let Some(run) = bundle.trajectory.runs.get(root) else {
        return DiagnosticTiming { duration_ms: None };
    };
    let duration_ms = run.records.iter().find_map(|record| match record {
        piko_protocol::TrajectoryRecord::ModelStep(value) => (value.step_id == record_id)
            .then_some(value.duration_ms)
            .flatten(),
        piko_protocol::TrajectoryRecord::ToolCall(value) => (value.call_id == record_id)
            .then_some(value.duration_ms)
            .flatten(),
        _ => None,
    });
    DiagnosticTiming { duration_ms }
}

fn event_status(event: &HistoryEvent, kind: &HistoryItemKind) -> Option<String> {
    match kind.0.as_str() {
        "model_step" => Some(
            event
                .summary
                .rsplit_once(": ")
                .map(|(_, tail)| tail.to_string())
                .unwrap_or_else(|| event.summary.clone()),
        ),
        "input" => Some(
            event
                .summary
                .rsplit_once("as ")
                .map(|(_, tail)| match tail {
                    "AppliedAsRoot" => "root work".to_string(),
                    "AppliedToStep" => "applied steer".to_string(),
                    "PendingFollowUp" => "queued follow-up".to_string(),
                    "PendingSteer" => "queued steer".to_string(),
                    "Cancelled" => "cancelled".to_string(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| event.summary.clone()),
        ),
        _ => None,
    }
}

pub(super) fn provenance(event: &HistoryEvent) -> HistoryProvenance {
    match event.provenance {
        piko_session_store::HistoryProvenance::Fact => HistoryProvenance::Fact,
        piko_session_store::HistoryProvenance::Diagnostic => HistoryProvenance::Diagnostic,
    }
}

pub(super) fn tool_call_id(
    event: &HistoryEvent,
    kind: &HistoryItemKind,
    bundle: &InspectionBundle,
) -> Option<String> {
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

pub(super) fn kind(event: &HistoryEvent) -> HistoryItemKind {
    let name = match event.event_type.as_str() {
        "message_committed" => "message",
        "agent_input_admitted_v1"
        | "agent_input_disposition_changed_v1"
        | "agent_input_applied_v1" => "input",
        "model_step_committed" => "model_step",
        value => value,
    };
    HistoryItemKind::new(name)
}

pub(super) fn lane_block(
    kind: HistoryLaneBlockKind,
    stream_item: &HistoryStreamItem,
    sequence: u32,
    label: String,
    status: String,
) -> HistoryLaneBlock {
    HistoryLaneBlock {
        kind,
        reference: stream_item.item_ref.clone(),
        label,
        status,
        sequence,
        started_at: None,
        duration_ms: stream_item.duration_ms,
    }
}
