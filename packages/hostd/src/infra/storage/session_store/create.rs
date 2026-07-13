use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use piko_protocol::{AgentInstanceIdentity, AgentInstanceLifecycle};

use super::super::SessionStorageError;
use super::SessionStore;
use super::io::atomic_create_jsonl;
use super::types::*;

impl SessionStore {
    pub fn create_session(
        session_dir: impl Into<PathBuf>,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let store = Self::new(session_dir);
        store.with_io(|| {
            fs::create_dir_all(store.agents_dir()).map_err(|source| SessionStorageError::Io {
                path: store.agents_dir(),
                source,
            })?;
            let root_agent_instance_id = format!("agent_{session_id}_root");
            let root_identity = AgentInstanceIdentity {
                session_id: session_id.clone(),
                agent_instance_id: root_agent_instance_id.clone(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            };
            let mut agents = BTreeMap::new();
            agents.insert(
                root_agent_instance_id.clone(),
                AgentManifestEntry {
                    identity: root_identity,
                    spec: None,
                    lifecycle: AgentInstanceLifecycle::Open,
                    latest_report: None,
                    created_at,
                    updated_at: created_at,
                },
            );
            store.store_manifest(&SessionManifest {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id,
                cwd,
                name: None,
                created_at,
                updated_at: created_at,
                current_leaf_id: None,
                root_agent_instance_id: Some(root_agent_instance_id),
                agent_revision: 1,
                agents,
                agent_inbox: Vec::new(),
                agent_executions: BTreeMap::new(),
                agent_input_queue: Vec::new(),
                entries: Vec::new(),
            })?;
            Ok(store.clone())
        })
    }

    /// Return the durable root AgentInstance, migrating a pre-AgentInstance
    /// manifest exactly once when necessary.
    pub fn ensure_root_agent(
        &self,
        agent_spec_id: &str,
    ) -> Result<AgentInstanceIdentity, SessionStorageError> {
        self.with_io(|| self.ensure_root_agent_under_lock(agent_spec_id))
    }

    fn ensure_root_agent_under_lock(
        &self,
        agent_spec_id: &str,
    ) -> Result<AgentInstanceIdentity, SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        if let Some(root_id) = &manifest.root_agent_instance_id
            && let Some(root) = manifest.agents.get(root_id)
        {
            return Ok(root.identity.clone());
        }

        let root_id = format!("agent_{}_root", manifest.session_id);
        let identity = AgentInstanceIdentity {
            session_id: manifest.session_id.clone(),
            agent_instance_id: root_id.clone(),
            agent_spec_id: agent_spec_id.to_string(),
            parent_agent_instance_id: None,
        };
        manifest.agent_revision = manifest.agent_revision.saturating_add(1);
        manifest.root_agent_instance_id = Some(root_id.clone());
        manifest.agents.insert(
            root_id,
            AgentManifestEntry {
                identity: identity.clone(),
                spec: None,
                lifecycle: AgentInstanceLifecycle::Open,
                latest_report: None,
                created_at: manifest.created_at,
                updated_at: manifest.updated_at,
            },
        );
        self.store_manifest(&manifest)?;
        Ok(identity)
    }

    /// Ensure the durable shard for `agent_instance_id` exists (header-only).
    /// Idempotent: a matching existing header is a no-op.
    pub fn ensure_agent_shard(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        agent_spec_id: &str,
        created_at: i64,
    ) -> Result<(), SessionStorageError> {
        self.with_io(|| {
            self.ensure_agent_shard_under_lock(
                session_id,
                agent_instance_id,
                agent_spec_id,
                created_at,
            )
        })
    }

    pub(super) fn ensure_agent_shard_under_lock(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        agent_spec_id: &str,
        created_at: i64,
    ) -> Result<(), SessionStorageError> {
        let path = self.agent_path(agent_instance_id);
        if path.exists() {
            let existing = self.read_header(&path)?;
            if existing.session_id == session_id && existing.agent_instance_id == agent_instance_id
            {
                return Ok(());
            }
            return Err(SessionStorageError::Invalid {
                path,
                message: "agent shard identity mismatch".into(),
            });
        }
        atomic_create_jsonl(
            &path,
            &AgentShardRecord::Header(AgentShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                agent_spec_id: agent_spec_id.to_string(),
                created_at,
            }),
        )
    }
}
