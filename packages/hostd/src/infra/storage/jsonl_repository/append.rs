use std::path::Path;

use uuid::Uuid;

use crate::api::{Message, SessionInfoEntry, SessionTreeEntry, ThinkingLevelChangeEntry};

use super::super::session_store::SessionStore;
use super::super::types::{JsonlSessionRepository, SessionStorageError};
use super::helpers::{commit_storage_error, timestamp};

impl JsonlSessionRepository {
    pub fn set_selected_agent(
        &self,
        session_dir: &Path,
        agent_instance_id: &str,
        updated_at: i64,
    ) -> Result<(), SessionStorageError> {
        SessionStore::new(session_dir).update_manifest(|manifest| {
            manifest.selected_agent_instance_id = Some(agent_instance_id.to_string());
            manifest.updated_at = manifest.updated_at.max(updated_at);
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
                let manifest = store.load_manifest()?;
                let agent_spec_id = manifest
                    .agents
                    .get(&agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .unwrap_or_else(|| message.agent_id.clone());
                let execution_id = orchd_api::stable_internal_id(
                    "projection",
                    &[&manifest.session_id, &agent_instance_id, &message.id],
                );
                store
                    .commit_message(
                        piko_protocol::execution::MessageCommit {
                            session_id: manifest.session_id,
                            source_turn_id: Some(message.source_turn_id.clone()),
                            execution_id,
                            agent_instance_id,
                            message_id: message.id.clone(),
                            parent_message_id: message.parent_id.clone(),
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
                let manifest = store.load_manifest()?;
                let execution_id = orchd_api::stable_internal_id(
                    "projection",
                    &[&manifest.session_id, agent_instance_id, &tool.id],
                );
                store
                    .commit_message(
                        piko_protocol::execution::MessageCommit {
                            session_id: manifest.session_id,
                            source_turn_id: Some(agent_instance_id.clone()),
                            execution_id,
                            agent_instance_id: agent_instance_id.clone(),
                            message_id: tool.id.clone(),
                            parent_message_id: None,
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
            _ => store.update_manifest(|manifest| {
                manifest.current_leaf_id = entry.leaf_target_id().map(str::to_string);
                manifest.entries.push(entry.clone());
            }),
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
        store.update_manifest(|manifest| {
            manifest.name = Some(name.to_string());
            manifest.entries.push(entry.clone());
        })?;
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
        let store = SessionStore::new(session_dir);
        store.update_manifest(|manifest| manifest.entries.extend(entries.clone()))?;
        Ok(entries)
    }

    pub fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::Compaction(crate::api::CompactionEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            summary: summary.to_string(),
            first_kept_entry_id: first_kept_entry_id.to_string(),
            tokens_before: 0,
            details: None,
            from_hook: None,
        });
        self.append_entry(session_dir, &entry, agent_id)?;
        Ok(entry)
    }

    pub fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::Leaf(crate::api::LeafEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            target_id: target_id.map(str::to_string),
        });
        self.append_entry(session_dir, &entry, agent_id)?;
        Ok(entry)
    }
}
