use std::collections::VecDeque;
use std::path::Path;

use crate::api::{AgentInfo, AgentStatus, Message, MessageEntry, ServerMessage, SessionTreeEntry};
use crate::domain::sessions::AgentViewState;
use crate::domain::sessions::SessionState;

use super::super::session_store::{SessionProjection, SessionStore};
use super::super::types::{PersistedSession, SessionStorageError};

pub(crate) fn load_session_dir(
    store: &SessionStore,
    dir: &Path,
) -> Result<PersistedSession, SessionStorageError> {
    let identity_path = dir.join("session.json");
    if !identity_path.exists() {
        return Err(SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: "missing session.json".into(),
        });
    }
    let aggregate =
        piko_session_store::query_current(dir).map_err(|error| SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: error.to_string(),
        })?;
    let projection = store.project_session(&aggregate)?;
    let mut state = SessionState::new(projection.session_id.clone(), projection.cwd.clone());
    state.name = projection.name.clone();
    state.current_leaf_id = projection.current_leaf_id.clone();
    state.entries = projection.entries.clone();
    state.last_model = projection.last_model.clone();
    state.world_state_baseline = projection.world_state_baseline.clone();
    for (agent_instance_id, entry) in message_entries(&aggregate) {
        if let SessionTreeEntry::Message(message) = &entry {
            state
                .task_heads
                .insert(agent_instance_id, message.id.clone());
        }
        state.entries.push(entry);
    }
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    state.cumulative_usage = aggregate
        .accounting
        .summarize(&piko_session_store::UsageQuery {
            incurred_only: true,
            ..piko_session_store::UsageQuery::default()
        })
        .usage;
    for agent_instance_id in projection.agents.keys() {
        let accounting = aggregate
            .accounting
            .summarize(&piko_session_store::UsageQuery {
                agent_instance_id: Some(agent_instance_id.clone()),
                incurred_only: true,
                ..piko_session_store::UsageQuery::default()
            });
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

fn message_entries(
    aggregate: &piko_session_store::SessionAggregate,
) -> Vec<(String, SessionTreeEntry)> {
    let mut messages = aggregate.messages.values().collect::<Vec<_>>();
    messages.sort_by_key(|message| message.revision);
    messages
        .into_iter()
        .map(|stored| {
            let agent_instance_id = stored.data.agent_instance_id.clone();
            let spec_id = aggregate
                .agents
                .get(&agent_instance_id)
                .map(|agent| agent.identity.agent_spec_id.clone())
                .unwrap_or_default();
            let seq = aggregate
                .messages
                .values()
                .filter(|message| message.data.agent_instance_id == agent_instance_id)
                .filter(|message| message.revision <= stored.revision)
                .count() as u64;
            (
                agent_instance_id.clone(),
                SessionTreeEntry::Message(MessageEntry {
                    id: stored.data.message_id.clone(),
                    parent_id: stored.data.tree_parent_entry_id.clone(),
                    timestamp: stored.data.committed_at.to_string(),
                    agent_id: spec_id,
                    agent_instance_id,
                    source_turn_id: stored.data.source_turn_id.clone().unwrap_or_default(),
                    transcript_seq: seq,
                    message: stored.data.message.clone(),
                }),
            )
        })
        .collect()
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
