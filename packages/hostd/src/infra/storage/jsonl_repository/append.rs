use std::path::Path;

use piko_session_store::EventData;
use uuid::Uuid;

use crate::api::{Message, SessionInfoEntry, SessionTreeEntry, ThinkingLevelChangeEntry};

use super::super::session_store::{SessionStore, tree_entry_event};
use super::super::types::{JsonlSessionRepository, SessionStorageError};
use super::helpers::{commit_storage_error, timestamp};

impl JsonlSessionRepository {
    pub fn set_selected_agent(
        &self,
        session_dir: &Path,
        agent_instance_id: &str,
        updated_at: i64,
    ) -> Result<(), SessionStorageError> {
        let store = SessionStore::new(session_dir);
        store.with_io(|| {
            let projection = store.load_projection()?;
            if projection.selected_agent_instance_id.as_deref() == Some(agent_instance_id) {
                return Ok(());
            }
            let seed = format!("{agent_instance_id}:{}", projection.journal_revision);
            commit_events(
                &store,
                "agent-selected",
                &seed,
                updated_at,
                vec![EventData::AgentSelected {
                    agent_instance_id: agent_instance_id.to_string(),
                    selected_at: updated_at,
                }],
            )
        })
    }

    pub fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        _agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        let store = SessionStore::new(session_dir);
        match entry {
            SessionTreeEntry::Message(message) => {
                let agent_instance_id = message.agent_instance_id.clone();
                let projection = store.load_projection()?;
                let agent_spec_id = projection
                    .agents
                    .get(&agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .unwrap_or_else(|| message.agent_id.clone());
                store
                    .commit_message(
                        piko_protocol::agent_work::MessageCommit {
                            session_id: projection.session_id,
                            root_input_id: message.root_input_id.clone(),
                            agent_instance_id,
                            message_id: message.id.clone(),
                            parent_message_id: message.parent_id.clone(),
                            tree_parent_entry_id: message.parent_id.clone(),
                            message: message.message.clone(),
                            committed_at: message.timestamp.parse().unwrap_or_default(),
                        },
                        &agent_spec_id,
                    )
                    .map_err(commit_storage_error)?;
                Ok(())
            }
            SessionTreeEntry::ToolCall(tool) => {
                let (Some(agent_instance_id), Some(agent_spec_id)) =
                    (&tool.agent_instance_id, &tool.agent_id)
                else {
                    return Err(SessionStorageError::Invalid {
                        path: session_dir.to_path_buf(),
                        message: "tool entry requires agent_instance_id and agent_id".into(),
                    });
                };
                let projection = store.load_projection()?;
                let root_input_id = piko_orchd_api::stable_internal_id(
                    "projection",
                    &[&projection.session_id, agent_instance_id, &tool.id],
                );
                store
                    .commit_message(
                        piko_protocol::agent_work::MessageCommit {
                            session_id: projection.session_id,
                            root_input_id: root_input_id.clone(),
                            agent_instance_id: agent_instance_id.clone(),
                            message_id: tool.id.clone(),
                            parent_message_id: tool.parent_id.clone(),
                            tree_parent_entry_id: tool.parent_id.clone(),
                            message: Message::ToolCall {
                                id: tool.tool_call_id.clone(),
                                name: tool.tool_name.clone(),
                                arguments: tool.arguments.clone(),
                                model: tool.model.clone(),
                                provider: tool.provider.clone(),
                                timestamp: tool.timestamp.parse().ok(),
                            },
                            committed_at: tool.timestamp.parse().unwrap_or_default(),
                        },
                        agent_spec_id,
                    )
                    .map_err(commit_storage_error)?;
                Ok(())
            }
            _ => store.append_tree_entry(entry),
        }
    }

    pub fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        _agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::SessionInfo(SessionInfoEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            name: Some(name.to_string()),
        });
        let store = SessionStore::new(session_dir);
        let committed_at = entry.timestamp().parse().unwrap_or_default();
        let events = vec![
            EventData::SessionMetadataChanged {
                name: Some(name.to_string()),
            },
            tree_entry_event(&entry)?,
        ];
        store
            .with_io(|| commit_events(&store, "session-info", entry.id(), committed_at, events))?;
        Ok(entry)
    }

    pub fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        _agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
        let mut entries = Vec::new();
        let mut cur = parent_id.map(str::to_string);
        if let (Some(m), Some(p)) = (model_id, provider) {
            let e = SessionTreeEntry::ModelChange(crate::api::ModelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: cur.clone(),
                timestamp: timestamp(),
                provider: p.to_string(),
                model_id: m.to_string(),
            });
            cur = Some(e.id().to_string());
            entries.push(e);
        }
        if let Some(tl) = thinking_level {
            let e = SessionTreeEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: cur,
                timestamp: timestamp(),
                thinking_level: tl.to_string(),
            });
            entries.push(e);
        }
        if let Some(last) = entries.last() {
            let store = SessionStore::new(session_dir);
            let mut events = entries
                .iter()
                .map(tree_entry_event)
                .collect::<Result<Vec<_>, _>>()?;
            if let (Some(model_id), Some(provider)) = (model_id, provider) {
                events.insert(
                    0,
                    EventData::ModelContinuityChanged {
                        provider: Some(provider.to_string()),
                        model_id: Some(model_id.to_string()),
                    },
                );
            }
            store.with_io(|| {
                commit_events(
                    &store,
                    "config-metadata",
                    last.id(),
                    last.timestamp().parse().unwrap_or_default(),
                    events,
                )
            })?;
        }
        Ok(entries)
    }

    pub fn set_last_model(
        &self,
        session_dir: &Path,
        model: Option<&crate::domain::sessions::SessionModelRef>,
    ) -> Result<(), SessionStorageError> {
        let store = SessionStore::new(session_dir);
        store.with_io(|| {
            let projection = store.load_projection()?;
            if projection.last_model.as_ref() == model {
                return Ok(());
            }
            let value_seed = model
                .map(|value| format!("{}:{}", value.provider, value.model_id))
                .unwrap_or_else(|| "cleared".into());
            let seed = format!("{value_seed}:{}", projection.journal_revision);
            commit_events(
                &store,
                "model-continuity",
                &seed,
                now_millis(),
                vec![EventData::ModelContinuityChanged {
                    provider: model.map(|value| value.provider.clone()),
                    model_id: model.map(|value| value.model_id.clone()),
                }],
            )
        })
    }

    /// Persist the session's world-state baseline (F-04 slice 2). `None`
    /// clears it so the next run re-injects the full snapshot.
    pub fn set_world_state_baseline(
        &self,
        session_dir: &Path,
        facts: Option<&crate::domain::prompts::WorldStateFacts>,
    ) -> Result<(), SessionStorageError> {
        let store = SessionStore::new(session_dir);
        store.with_io(|| {
            let projection = store.load_projection()?;
            if projection.world_state_baseline.as_ref() == facts {
                return Ok(());
            }
            let value = facts
                .map(serde_json::to_value)
                .transpose()
                .map_err(|source| SessionStorageError::Json {
                    path: session_dir.to_path_buf(),
                    source,
                })?;
            let value_seed =
                serde_json::to_string(&value).map_err(|source| SessionStorageError::Json {
                    path: session_dir.to_path_buf(),
                    source,
                })?;
            let seed = format!("{value_seed}:{}", projection.journal_revision);
            commit_events(
                &store,
                "world-state",
                &seed,
                now_millis(),
                vec![EventData::WorldStateAdvanced { facts: value }],
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
        tokens_before: u64,
        details: Option<serde_json::Value>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::Compaction(crate::api::CompactionEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            summary: summary.to_string(),
            first_kept_entry_id: first_kept_entry_id.to_string(),
            tokens_before,
            details,
            from_hook: None,
        });
        self.append_entry(session_dir, &entry, agent_id)?;
        Ok(entry)
    }

    pub fn navigate(
        &self,
        session_dir: &Path,
        target_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        let store = SessionStore::new(session_dir);
        store.select_branch(target_id)
    }
}

fn commit_events(
    store: &SessionStore,
    purpose: &str,
    seed: &str,
    committed_at: i64,
    events: Vec<EventData>,
) -> Result<(), SessionStorageError> {
    let session_id = store.load_projection()?.session_id;
    let commit_id = piko_orchd_api::stable_internal_id(purpose, &[&session_id, seed]);
    store
        .commit_events(&commit_id, committed_at, events)
        .map(|_| ())
}

fn now_millis() -> i64 {
    timestamp().parse().unwrap_or_default()
}
