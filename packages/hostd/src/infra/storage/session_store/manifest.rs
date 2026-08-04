use std::fs;
use std::path::PathBuf;

use super::super::SessionStorageError;
use super::SessionStore;
use super::io::{atomic_create_jsonl, atomic_write_json, read_records};
use super::types::*;

impl SessionStore {
    pub fn load_manifest(&self) -> Result<SessionManifest, SessionStorageError> {
        let path = self.manifest_path();
        let data = fs::read_to_string(&path).map_err(|source| SessionStorageError::Io {
            path: path.clone(),
            source,
        })?;
        let manifest: SessionManifest =
            serde_json::from_str(&data).map_err(|source| SessionStorageError::Json {
                path: path.clone(),
                source,
            })?;
        if manifest.schema_version != SESSION_SCHEMA_VERSION {
            return Err(SessionStorageError::Invalid {
                path,
                message: "unsupported session manifest schema".into(),
            });
        }
        Ok(manifest)
    }

    pub fn update_manifest(
        &self,
        update: impl FnOnce(&mut SessionManifest),
    ) -> Result<(), SessionStorageError> {
        self.with_io(|| {
            let mut manifest = self.load_manifest()?;
            update(&mut manifest);
            self.store_manifest(&manifest)
        })
    }

    pub fn store_manifest(&self, manifest: &SessionManifest) -> Result<(), SessionStorageError> {
        atomic_write_json(&self.manifest_path(), manifest)
    }

    pub(super) fn advance_root_leaf_under_lock(
        &self,
        agent_instance_id: &str,
        message_id: &str,
        committed_at: i64,
    ) -> Result<(), SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        if manifest.root_agent_instance_id.as_deref() != Some(agent_instance_id) {
            return Ok(());
        }
        manifest.current_leaf_id = Some(message_id.to_string());
        manifest.updated_at = manifest.updated_at.max(committed_at);
        self.store_manifest(&manifest)
    }

    pub fn fork_to(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        self.with_io(|| self.fork_to_under_lock(destination, new_session_id, created_at))
    }

    fn fork_to_under_lock(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let source_manifest = self.load_manifest()?;
        let source_session_id = source_manifest.session_id.clone();
        let destination = Self::new(destination);
        fs::create_dir_all(destination.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: destination.agents_dir(),
            source,
        })?;
        let mut manifest = sanitize_forked_manifest(source_manifest, &new_session_id, created_at);
        // Full clone keeps every agent identity; rewrite session_id only.
        for agent in manifest.agents.values_mut() {
            agent.identity.session_id = new_session_id.clone();
            agent.latest_report = None;
        }
        destination.store_manifest(&manifest)?;

        for agent_instance_id in self.list_agents(&source_session_id)? {
            self.copy_agent_shard_to(&destination, &agent_instance_id, &new_session_id, None)?;
        }
        Ok(destination)
    }

    /// Branch-point fork: write a new session directory that retains only the
    /// ancestor path through `entry_id` (F-09 / D-26).
    pub fn fork_to_at_entry(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
        entry_id: &str,
        retained_entries: &[crate::api::SessionTreeEntry],
    ) -> Result<Self, SessionStorageError> {
        self.with_io(|| {
            self.fork_to_at_entry_under_lock(
                destination,
                new_session_id,
                created_at,
                entry_id,
                retained_entries,
            )
        })
    }

    fn fork_to_at_entry_under_lock(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
        entry_id: &str,
        retained_entries: &[crate::api::SessionTreeEntry],
    ) -> Result<Self, SessionStorageError> {
        use std::collections::{BTreeMap, HashSet};

        let source_manifest = self.load_manifest()?;
        let kept_entry_ids: HashSet<&str> =
            retained_entries.iter().map(|entry| entry.id()).collect();
        if !kept_entry_ids.contains(entry_id) {
            return Err(SessionStorageError::Invalid {
                path: self.session_dir.clone(),
                message: format!("unknown tree entry: {entry_id}"),
            });
        }

        let mut kept_message_ids: HashSet<&str> = HashSet::new();
        let mut kept_agent_ids: HashSet<String> = HashSet::new();
        for entry in retained_entries {
            match entry {
                crate::api::SessionTreeEntry::Message(message) => {
                    kept_message_ids.insert(message.id.as_str());
                    kept_agent_ids.insert(message.agent_instance_id.clone());
                }
                crate::api::SessionTreeEntry::ToolCall(tool) => {
                    kept_message_ids.insert(tool.id.as_str());
                    if let Some(agent_instance_id) = &tool.agent_instance_id {
                        kept_agent_ids.insert(agent_instance_id.clone());
                    }
                }
                _ => {}
            }
        }
        if let Some(root) = &source_manifest.root_agent_instance_id {
            kept_agent_ids.insert(root.clone());
        }

        let destination = Self::new(destination);
        fs::create_dir_all(destination.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: destination.agents_dir(),
            source,
        })?;

        let mut manifest = sanitize_forked_manifest(source_manifest, &new_session_id, created_at);
        manifest
            .entries
            .retain(|entry| kept_entry_ids.contains(entry.id()));
        manifest.current_leaf_id = Some(entry_id.to_string());
        manifest.agents = manifest
            .agents
            .into_iter()
            .filter(|(id, _)| kept_agent_ids.contains(id))
            .map(|(id, mut agent)| {
                agent.identity.session_id = new_session_id.clone();
                agent.latest_report = None;
                (id, agent)
            })
            .collect::<BTreeMap<_, _>>();
        if let Some(selected) = &manifest.selected_agent_instance_id
            && !kept_agent_ids.contains(selected)
        {
            manifest.selected_agent_instance_id = manifest.root_agent_instance_id.clone();
        }
        destination.store_manifest(&manifest)?;

        for agent_instance_id in &kept_agent_ids {
            // Skip agents that never received a durable shard on the source.
            if !self.agent_path(agent_instance_id).exists() {
                continue;
            }
            self.copy_agent_shard_to(
                &destination,
                agent_instance_id,
                &new_session_id,
                Some(&kept_message_ids),
            )?;
        }
        Ok(destination)
    }

    fn copy_agent_shard_to(
        &self,
        destination: &Self,
        agent_instance_id: &str,
        new_session_id: &str,
        message_filter: Option<&std::collections::HashSet<&str>>,
    ) -> Result<(), SessionStorageError> {
        let records = read_records(&self.agent_path(agent_instance_id))?;
        let Some(AgentShardRecord::Header(mut header)) = records.first().cloned() else {
            return Err(SessionStorageError::Invalid {
                path: self.agent_path(agent_instance_id),
                message: "missing agent shard header".into(),
            });
        };
        header.session_id = new_session_id.to_string();
        atomic_create_jsonl(
            &destination.agent_path(agent_instance_id),
            &AgentShardRecord::Header(header),
        )?;
        for record in records.into_iter().skip(1) {
            if let Some(filter) = message_filter {
                match &record {
                    AgentShardRecord::Message(message) if !filter.contains(message.id.as_str()) => {
                        continue;
                    }
                    _ => {}
                }
            }
            destination
                .append_record(agent_instance_id, &record)
                .map_err(|error| SessionStorageError::Invalid {
                    path: destination.agent_path(agent_instance_id),
                    message: error.to_string(),
                })?;
        }
        Ok(())
    }
}

/// Shared post-clone cleanup for full and branch-point forks (F-09 / D-26).
fn sanitize_forked_manifest(
    mut manifest: SessionManifest,
    new_session_id: &str,
    created_at: i64,
) -> SessionManifest {
    manifest.session_id = new_session_id.to_string();
    manifest.created_at = created_at;
    manifest.updated_at = created_at;
    manifest.world_state_baseline = None;
    manifest.agent_inbox.clear();
    manifest.agent_input_queue.clear();
    manifest.agent_executions.clear();
    manifest
}
