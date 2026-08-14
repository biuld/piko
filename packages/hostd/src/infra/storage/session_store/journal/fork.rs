use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use piko_session_store::{EventData, NewSession, SessionForkedV1, SessionStore as Journal};

use crate::api::SessionTreeEntry;
use crate::ports::storage_types::SessionStorageError;

use super::SessionStore;

impl SessionStore {
    pub fn fork_to(
        &self,
        target: impl Into<std::path::PathBuf>,
        session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let target = target.into();
        self.fork_projected(&target, session_id, created_at, None)?;
        Ok(Self::new(target))
    }

    pub fn fork_to_at_entry(
        &self,
        target: &Path,
        session_id: String,
        created_at: i64,
        _entry_id: &str,
        retained: &[SessionTreeEntry],
    ) -> Result<(), SessionStorageError> {
        let ids = retained
            .iter()
            .map(|entry| entry.id().to_string())
            .collect::<BTreeSet<_>>();
        self.fork_projected(target, session_id, created_at, Some(&ids))
    }

    fn fork_projected(
        &self,
        target: &Path,
        session_id: String,
        created_at: i64,
        retained: Option<&BTreeSet<String>>,
    ) -> Result<(), SessionStorageError> {
        let parent = target
            .parent()
            .ok_or_else(|| self.invalid("fork target has no parent"))?;
        fs::create_dir_all(parent).map_err(|source| SessionStorageError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let name = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session");
        let staging_root = parent.join(".staging");
        fs::create_dir_all(&staging_root).map_err(|source| SessionStorageError::Io {
            path: staging_root.clone(),
            source,
        })?;
        let staging = staging_root.join(format!("{name}-{}", uuid::Uuid::new_v4()));
        if let Err(error) = self.fork_projected_into(&staging, session_id, created_at, retained) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(source) = fs::rename(&staging, target) {
            let _ = fs::remove_dir_all(&staging);
            return Err(SessionStorageError::Io {
                path: target.to_path_buf(),
                source,
            });
        }
        fs::File::open(&staging_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| SessionStorageError::Io {
                path: staging_root,
                source,
            })?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| SessionStorageError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        Ok(())
    }

    fn fork_projected_into(
        &self,
        target: &Path,
        session_id: String,
        created_at: i64,
        retained: Option<&BTreeSet<String>>,
    ) -> Result<(), SessionStorageError> {
        let source = self.aggregate()?;
        let source_session_id = source
            .session_id
            .clone()
            .ok_or_else(|| self.invalid("source session has no identity"))?;
        let mut root = source
            .root
            .clone()
            .ok_or_else(|| self.invalid("source session has no root"))?;
        root.session_id = session_id.clone();
        let opened = Journal::create(
            target,
            NewSession {
                session_id: session_id.clone(),
                cwd: source.cwd.clone().unwrap_or_default(),
                created_at,
                root,
            },
        )
        .map_err(|error| self.storage_error(error))?;
        let forked = SessionStore::new(target);
        drop(opened);
        forked.commit_events(
            &piko_orchd_api::stable_internal_id("session-fork", &[&session_id]),
            created_at,
            vec![EventData::SessionForked(SessionForkedV1 {
                source_session_id,
                source_revision: source.revision,
                source_tree_entry_id: retained.and_then(|ids| ids.last().cloned()),
            })],
        )?;

        for agent in source.agents.values() {
            let Some(spec) = agent.spec.clone() else {
                continue;
            };
            let mut identity = agent.identity.clone();
            identity.session_id = session_id.clone();
            forked.commit_events(
                &piko_orchd_api::stable_internal_id(
                    "fork-agent",
                    &[&session_id, &identity.agent_instance_id],
                ),
                created_at,
                vec![EventData::AgentCreated {
                    identity,
                    spec,
                    created_at: agent.created_at,
                }],
            )?;
        }
        let mut messages = source.messages.values().collect::<Vec<_>>();
        messages.sort_by_key(|message| message.revision);
        for message in messages {
            if retained.is_some_and(|ids| !ids.contains(&message.data.message_id)) {
                continue;
            }
            forked.commit_events(
                &piko_orchd_api::stable_internal_id(
                    "fork-message",
                    &[&session_id, &message.data.message_id],
                ),
                message.data.committed_at,
                vec![EventData::MessageCommitted(message.data.clone())],
            )?;
        }
        let mut entries = source.tree_entries.values().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.revision);
        for entry in entries {
            if retained.is_some_and(|ids| !ids.contains(&entry.data.entry_id)) {
                continue;
            }
            forked.commit_events(
                &piko_orchd_api::stable_internal_id(
                    "fork-tree-entry",
                    &[&session_id, &entry.data.entry_id],
                ),
                entry.data.timestamp,
                vec![EventData::TreeEntryRecorded(entry.data.clone())],
            )?;
        }
        let mut final_events = Vec::new();
        if source.name.is_some() {
            final_events.push(EventData::SessionMetadataChanged {
                name: source.name.clone(),
            });
        }
        if let Some(model) = source.model_continuity {
            final_events.push(EventData::ModelContinuityChanged {
                provider: Some(model.provider),
                model_id: Some(model.model_id),
            });
        }
        // Forks intentionally clear the world-state diff baseline so their
        // first run injects a complete snapshot in the new session context.
        for (agent_instance_id, todo_list) in source.todo_lists {
            final_events.push(EventData::TodoListReplaced {
                agent_instance_id,
                todo_list: Some(todo_list),
            });
        }
        if let Some(selected) = source.selected_agent_instance_id {
            final_events.push(EventData::AgentSelected {
                agent_instance_id: selected,
                selected_at: created_at,
            });
        }
        let selected = source
            .selected_tree_entry_id
            .filter(|id| retained.is_none_or(|ids| ids.contains(id)));
        let root_base = source
            .root_base_message_id
            .filter(|id| retained.is_none_or(|ids| ids.contains(id)));
        if selected.is_some() || root_base.is_some() {
            final_events.push(EventData::BranchSelected {
                selected_tree_entry_id: selected,
                root_base_message_id: root_base,
            });
        }
        if !final_events.is_empty() {
            forked.commit_events(
                &piko_orchd_api::stable_internal_id("fork-projection", &[&session_id]),
                created_at,
                final_events,
            )?;
        }
        Ok(())
    }
}
