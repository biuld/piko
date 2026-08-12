use std::collections::VecDeque;
use std::path::Path;

use crate::api::{AgentInfo, AgentStatus, Message, ServerMessage, SessionTreeEntry};
use crate::domain::sessions::AgentViewState;
use crate::domain::sessions::SessionState;

use super::super::recovery::agent_transcript_entries;
use super::super::session_store::{SessionProjection, SessionStore};
use super::super::types::{PersistedSession, SessionStorageError};

pub(crate) fn load_session_dir(dir: &Path) -> Result<PersistedSession, SessionStorageError> {
    let identity_path = dir.join("session.json");
    if !identity_path.exists() {
        return Err(SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: "missing session.json".into(),
        });
    }
    let store = SessionStore::new(dir);
    let projection = store.load_projection()?;
    let mut state = SessionState::new(projection.session_id.clone(), projection.cwd.clone());
    state.name = projection.name.clone();
    state.current_leaf_id = projection.current_leaf_id.clone();
    state.entries = projection.entries.clone();
    state.last_model = projection.last_model.clone();
    state.world_state_baseline = projection.world_state_baseline.clone();
    let mut recovered_root_leaf = None;
    for agent_instance_id in store.list_agents(&projection.session_id)? {
        let recovered = store.load_agent(&projection.session_id, &agent_instance_id)?;
        if projection.root_agent_instance_id.as_deref() == Some(agent_instance_id.as_str()) {
            recovered_root_leaf = Some(resolve_recovered_root_leaf(&projection, &recovered));
        }
        for entry in agent_transcript_entries(&recovered) {
            if let SessionTreeEntry::Message(message) = &entry {
                state
                    .task_heads
                    .insert(agent_instance_id.clone(), message.id.clone());
            }
            state.entries.push(entry);
        }
    }
    if let Some(root_leaf) = recovered_root_leaf {
        state.current_leaf_id = root_leaf;
    }
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    state.cumulative_usage = store
        .usage_summary(&piko_session_store::UsageQuery {
            incurred_only: true,
            ..piko_session_store::UsageQuery::default()
        })?
        .usage;
    for agent_instance_id in projection.agents.keys() {
        let accounting = store.usage_summary(&piko_session_store::UsageQuery {
            agent_instance_id: Some(agent_instance_id.clone()),
            incurred_only: true,
            ..piko_session_store::UsageQuery::default()
        })?;
        if accounting.fact_count > 0 {
            state
                .agent_usage
                .insert(agent_instance_id.clone(), accounting.usage);
        }
    }
    restore_agent_runtime_state(&mut state, &projection);
    // F-27: seed durable todo lists from agent projection fields.
    for agent in projection.agents.values() {
        if let Some(list) = &agent.todo_list {
            state
                .todo_lists
                .insert(agent.identity.agent_instance_id.clone(), list.clone());
        }
    }
    Ok(PersistedSession {
        state,
        path: dir.to_path_buf(),
        created_at: projection.created_at.to_string(),
        parent_session_path: None,
    })
}

fn resolve_recovered_root_leaf(
    projection: &super::super::session_store::SessionProjection,
    recovered: &super::super::session_store::RecoveredAgent,
) -> Option<String> {
    let Some(head_id) = recovered.head_message_id.as_ref() else {
        return projection.current_leaf_id.clone();
    };
    if projection.current_leaf_id.as_ref() == Some(head_id) {
        return Some(head_id.clone());
    }

    let head_timestamp = recovered
        .transcript
        .iter()
        .find(|message| &message.id == head_id)
        .map(|message| message.timestamp)
        .unwrap_or(i64::MIN);
    let selection_timestamp = projection
        .entries
        .iter()
        .filter(|entry| match entry {
            SessionTreeEntry::Leaf(leaf) => leaf.target_id == projection.current_leaf_id,
            _ => projection.current_leaf_id.as_deref() == Some(entry.id()),
        })
        .filter_map(|entry| entry.timestamp().parse::<i64>().ok())
        .max()
        .unwrap_or(i64::MIN);

    if head_timestamp > selection_timestamp {
        Some(head_id.clone())
    } else {
        projection.current_leaf_id.clone()
    }
}

fn restore_agent_runtime_state(state: &mut SessionState, projection: &SessionProjection) {
    let specs = crate::adapters::prompts::agent_loader::load_agents(&state.cwd);
    for agent in projection.agents.values() {
        let spec = specs.get(&agent.identity.agent_spec_id);
        let unread_report_count = projection
            .agent_inbox
            .iter()
            .filter(|item| {
                item.recipient_agent_instance_id == agent.identity.agent_instance_id
                    && item.consumed_at.is_none()
            })
            .count() as u32;
        let status = match agent.lifecycle {
            piko_protocol::AgentInstanceLifecycle::Open => AgentStatus::Idle,
            piko_protocol::AgentInstanceLifecycle::Closed => AgentStatus::Closed,
            piko_protocol::AgentInstanceLifecycle::Terminated => AgentStatus::Stopped,
            piko_protocol::AgentInstanceLifecycle::Unavailable => AgentStatus::Failed,
        };
        state.active_agents.insert(
            agent.identity.agent_instance_id.clone(),
            AgentInfo {
                session_id: state.session_id.clone(),
                agent_instance_id: agent.identity.agent_instance_id.clone(),
                agent_id: agent.identity.agent_spec_id.clone(),
                parent_agent_instance_id: agent.identity.parent_agent_instance_id.clone(),
                lifecycle: agent.lifecycle,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count,
                name: spec
                    .map(|spec| spec.name.clone())
                    .unwrap_or_else(|| agent.identity.agent_spec_id.clone()),
                role: spec
                    .map(|spec| spec.role.clone())
                    .unwrap_or_else(|| "assistant".into()),
                status,
            },
        );
    }

    state.active_agent_instance_id = projection
        .selected_agent_instance_id
        .clone()
        .filter(|selected| state.active_agents.contains_key(selected))
        .or_else(|| projection.root_agent_instance_id.clone())
        .filter(|selected| state.active_agents.contains_key(selected))
        .or_else(|| state.active_agents.keys().next().cloned());

    let entries = state.entries.clone();
    for entry in entries {
        for (agent_instance_id, agent_id, message) in
            project_agent_view_from_entry(&state.session_id, &entry)
        {
            let seq = state.next_agent_view_seq;
            state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
            let view = state
                .agent_views
                .entry(agent_instance_id.clone())
                .or_insert_with(|| AgentViewState {
                    agent_instance_id: agent_instance_id.clone(),
                    agent_id: agent_id.clone(),
                    events: VecDeque::new(),
                    next_seq: 1,
                });
            view.next_seq = seq.saturating_add(1);
            view.events
                .push_back(piko_protocol::SequencedServerMessage {
                    seq,
                    message: Box::new(message),
                });
        }
    }
}

fn project_agent_view_from_entry(
    session_id: &str,
    entry: &SessionTreeEntry,
) -> Vec<(String, String, ServerMessage)> {
    match entry {
        SessionTreeEntry::Message(message) => {
            let agent_instance_id = &message.agent_instance_id;
            let agent_id = &message.agent_id;
            match &message.message {
                Message::User { .. } | Message::Assistant { .. } | Message::ToolResult { .. } => {
                    vec![(
                        agent_instance_id.clone(),
                        agent_id.clone(),
                        ServerMessage::TranscriptCommitted(
                            piko_protocol::TranscriptCommittedEvent {
                                session_id: session_id.to_string(),
                                agent_instance_id: agent_instance_id.clone(),
                                agent_id: agent_id.clone(),
                                source_turn_id: message.source_turn_id.clone(),
                                message_id: message.id.clone(),
                                transcript_seq: message.transcript_seq,
                                message: message.message.clone(),
                            },
                        ),
                    )]
                }
                _ => Vec::new(),
            }
        }
        SessionTreeEntry::ToolCall(tool) => {
            let (Some(agent_instance_id), Some(agent_id)) =
                (&tool.agent_instance_id, &tool.agent_id)
            else {
                return Vec::new();
            };
            let tool_event = piko_protocol::ToolExecutionEvent::Started {
                session_id: session_id.to_string(),
                agent_instance_id: agent_instance_id.clone(),
                agent_id: agent_id.clone(),
                tool_call_id: tool.tool_call_id.clone(),
                tool_name: tool.tool_name.clone(),
                args: tool.arguments.clone(),
                parent_message_id: tool.parent_message_id.clone(),
                source_turn_id: None,
            };
            piko_protocol::StreamItemPatch::from_tool_execution(&tool_event)
                .into_iter()
                .map(|patch| {
                    (
                        agent_instance_id.clone(),
                        agent_id.clone(),
                        ServerMessage::StreamItem(patch),
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    }
}
