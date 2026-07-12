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
        let mut manifest = self.load_manifest()?;
        update(&mut manifest);
        self.store_manifest(&manifest)
    }

    pub fn store_manifest(&self, manifest: &SessionManifest) -> Result<(), SessionStorageError> {
        atomic_write_json(&self.manifest_path(), manifest)
    }
    pub fn fork_to(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let source_manifest = self.load_manifest()?;
        let destination = Self::new(destination);
        fs::create_dir_all(destination.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: destination.agents_dir(),
            source,
        })?;
        let mut manifest = source_manifest.clone();
        manifest.session_id = new_session_id.clone();
        manifest.created_at = created_at;
        manifest.updated_at = created_at;
        destination.store_manifest(&manifest)?;

        for agent_instance_id in self.list_agents(&source_manifest.session_id)? {
            let records = read_records(&self.agent_path(&agent_instance_id))?;
            let Some(AgentShardRecord::Header(mut header)) = records.first().cloned() else {
                return Err(SessionStorageError::Invalid {
                    path: self.agent_path(&agent_instance_id),
                    message: "missing agent shard header".into(),
                });
            };
            header.session_id = new_session_id.clone();
            atomic_create_jsonl(
                &destination.agent_path(&agent_instance_id),
                &AgentShardRecord::Header(header),
            )?;
            for record in records.into_iter().skip(1) {
                destination
                    .append_record(&agent_instance_id, &record)
                    .map_err(|error| SessionStorageError::Invalid {
                        path: destination.agent_path(&agent_instance_id),
                        message: error.to_string(),
                    })?;
            }
        }
        Ok(destination)
    }
}
