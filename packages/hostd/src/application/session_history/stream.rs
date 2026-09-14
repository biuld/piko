use piko_protocol::{HistoryLaneSummary, HistoryStreamPage};
use piko_session_store::InspectionBundle;

pub(super) fn agent_stream(
    session_id: &str,
    agent_instance_id: &str,
    bundle: &InspectionBundle,
    offset: usize,
    limit: usize,
) -> HistoryStreamPage {
    let mut items = Vec::new();
    let mut position = 0usize;
    'commits: for commit in &bundle.history.commits {
        for (index, event) in commit.events.iter().enumerate() {
            if event.agent_instance_id.as_deref() != Some(agent_instance_id)
                || !super::mapping::is_stream_event_public(event)
            {
                continue;
            }
            if position >= offset + limit {
                break 'commits;
            }
            let take = position >= offset;
            position += 1;
            if take {
                items.push(super::mapping::stream_item(
                    commit.revision,
                    commit.committed_at,
                    index,
                    event,
                    bundle.revision,
                    bundle,
                ));
            }
        }
    }
    let next_offset = offset + items.len();
    let next_cursor = (items.len() == limit
        && stream_count(bundle, agent_instance_id) > next_offset)
        .then(|| {
            format!(
                "agent:{agent_instance_id}:{}:{next_offset}",
                bundle.revision
            )
        });
    HistoryStreamPage {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        revision: bundle.revision,
        items,
        next_cursor,
    }
}

fn stream_count(bundle: &InspectionBundle, agent_instance_id: &str) -> usize {
    bundle
        .history
        .commits
        .iter()
        .flat_map(|commit| commit.events.iter())
        .filter(|event| {
            event.agent_instance_id.as_deref() == Some(agent_instance_id)
                && super::mapping::is_stream_event_public(event)
        })
        .count()
}

/// Bounded lane strip: every ModelStep and tool-call row of the agent becomes
/// one block. Blocks are positioned by their journal-order position in the
/// agent's stream, so a step and the tool calls it declared are adjacent
/// without colliding; commit timestamps alone cannot distinguish work inside
/// one atomic commit.
pub(super) fn lane_summary(
    session_id: &str,
    agent_instance_id: &str,
    bundle: &InspectionBundle,
) -> HistoryLaneSummary {
    let mut blocks = Vec::new();
    let mut stream_position = 0usize;
    'commits: for commit in &bundle.history.commits {
        for (index, event) in commit.events.iter().enumerate() {
            if event.agent_instance_id.as_deref() != Some(agent_instance_id) {
                continue;
            }
            if !super::mapping::is_stream_event_public(event) {
                continue;
            }
            let position = stream_position;
            stream_position += 1;
            if blocks.len() >= super::LANE_BLOCK_LIMIT {
                break 'commits;
            }
            let kind = super::mapping::kind(event);
            match (event.event_type.as_str(), kind.0.as_str()) {
                ("model_step_committed", "model_step") => {}
                ("message_committed", "message") => {}
                _ => continue,
            }
            let item = super::mapping::stream_item(
                commit.revision,
                commit.committed_at,
                index,
                event,
                bundle.revision,
                bundle,
            );
            let is_tool = bundle
                .current
                .messages
                .get(item.relation.message_id.as_deref().unwrap_or_default())
                .is_some_and(|stored| {
                    matches!(stored.data.message, piko_protocol::Message::ToolCall { .. })
                });
            if event.event_type != "model_step_committed" && !is_tool {
                continue;
            }
            let sequence = position as u32;
            let (kind, label) = if event.event_type == "model_step_committed" {
                (
                    piko_protocol::HistoryLaneBlockKind::ModelStep,
                    format!(
                        "step {}",
                        item.relation
                            .model_step_id
                            .as_deref()
                            .and_then(step_index)
                            .unwrap_or(sequence)
                    ),
                )
            } else {
                (
                    piko_protocol::HistoryLaneBlockKind::ToolCall,
                    tool_label(bundle, &item),
                )
            };
            blocks.push(super::mapping::lane_block(
                kind,
                &item,
                sequence,
                label,
                item.status.clone().unwrap_or_else(|| "committed".into()),
            ));
        }
    }
    let timing_available =
        !blocks.is_empty() && blocks.iter().all(|block| block.duration_ms.is_some());
    HistoryLaneSummary {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        revision: bundle.revision,
        blocks,
        timing_available,
    }
}

fn step_index(model_step_id: &str) -> Option<u32> {
    model_step_id.rsplit('-').next()?.parse().ok()
}

fn tool_label(bundle: &InspectionBundle, item: &piko_protocol::HistoryStreamItem) -> String {
    item.relation
        .message_id
        .as_deref()
        .and_then(|id| bundle.current.messages.get(id))
        .and_then(|stored| match &stored.data.message {
            piko_protocol::Message::ToolCall { name, .. } => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "tool".into())
}
