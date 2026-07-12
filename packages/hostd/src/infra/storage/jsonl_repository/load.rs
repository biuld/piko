use std::collections::VecDeque;
use std::fs;
use std::io::BufRead;
use std::path::Path;

use crate::api::{AgentInfo, AgentStatus, Message, ServerMessage, SessionTreeEntry};
use crate::domain::sessions::AgentViewState;
use crate::domain::sessions::SessionState;

use super::super::jsonl_io::SessionHeader;
use super::super::recovery::{agent_task_state_from_manifest_entry, agent_transcript_entries};
use super::super::session_store::{SessionManifest, SessionStore};
use super::super::types::{PersistedSession, SessionStorageError};

pub(crate) fn load_session_dir(dir: &Path) -> Result<PersistedSession, SessionStorageError> {
    let manifest_path = dir.join("session.json");
    if !manifest_path.exists() {
        return Err(SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: "missing session.json".into(),
        });
    }
    let store = SessionStore::new(dir);
    let manifest = store.load_manifest()?;
    let mut state = SessionState::new(manifest.session_id.clone(), manifest.cwd.clone());
    state.name = manifest.name.clone();
    state.current_leaf_id = manifest.current_leaf_id.clone();
    state.entries = manifest.entries.clone();
    for agent_instance_id in store.list_agents(&manifest.session_id)? {
        let recovered = store.load_agent(&manifest.session_id, &agent_instance_id)?;
        if let Some(agent) = manifest.agents.get(&agent_instance_id) {
            state.tasks.insert(
                agent_instance_id.clone(),
                agent_task_state_from_manifest_entry(&manifest, agent),
            );
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
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    restore_agent_runtime_state(&mut state, &manifest);
    Ok(PersistedSession {
        state,
        path: dir.to_path_buf(),
        created_at: manifest.created_at.to_string(),
        parent_session_path: None,
    })
}

#[allow(dead_code)]
fn load_file_state(path: &Path) -> Result<(SessionState, SessionHeader), SessionStorageError> {
    let f = fs::File::open(path).map_err(|e| SessionStorageError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut lines = std::io::BufReader::new(f).lines();
    let hl = lines
        .next()
        .ok_or(SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "missing header".into(),
        })?
        .map_err(|e| SessionStorageError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    let h: SessionHeader = serde_json::from_str(&hl).map_err(|e| SessionStorageError::Json {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut state = SessionState::new(h.id.clone(), h.cwd.clone());
    for l in lines {
        let l = l.map_err(|e| SessionStorageError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        if l.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<SessionTreeEntry>(&l) {
            state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
            state.entries.push(entry);
        }
    }
    state.seq = state.entries.len() as u64;
    Ok((state, h))
}

fn restore_agent_runtime_state(state: &mut SessionState, manifest: &SessionManifest) {
    let specs = crate::adapters::prompts::agent_loader::load_agents(&state.cwd);
    for agent in manifest.agents.values() {
        let spec = specs.get(&agent.identity.agent_spec_id);
        let unread_report_count = manifest
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

    state.active_agent_instance_id = state
        .active_agents
        .values()
        .find(|agent| agent.parent_agent_instance_id.is_none())
        .map(|agent| agent.agent_instance_id.clone())
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
            let (Some(agent_instance_id), Some(agent_id)) = (&tool.task_id, &tool.agent_id) else {
                return Vec::new();
            };
            vec![(
                agent_instance_id.clone(),
                agent_id.clone(),
                ServerMessage::ToolExecution(piko_protocol::ToolExecutionEvent::Started {
                    task_id: agent_instance_id.clone(),
                    agent_id: agent_id.clone(),
                    tool_call_id: tool.tool_call_id.clone(),
                    tool_name: tool.tool_name.clone(),
                    args: tool.arguments.clone(),
                    parent_message_id: tool.parent_message_id.clone(),
                }),
            )]
        }
        _ => Vec::new(),
    }
}
