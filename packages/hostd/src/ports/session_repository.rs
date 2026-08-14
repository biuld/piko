//! Outbound port for session CRUD/tree-mutation storage.
//!
//! `application` reaches session directory storage (create/open/list/fork/
//! import, tree-entry append, navigation) only through
//! [`SessionRepositoryPort`]; the concrete JSONL-backed implementation
//! (`JsonlSessionRepository`) lives in `infra::storage` and is wired up as
//! a port implementation in `adapters::storage`.

use std::path::Path;

use async_trait::async_trait;

use super::storage_types::{PersistedSession, SessionStorageError};
use crate::api::{SessionSummary, SessionTreeEntry};
use crate::domain::prompts::WorldStateFacts;
use crate::domain::sessions::SessionModelRef;

#[async_trait]
pub trait SessionRepositoryPort: Send + Sync {
    async fn create(&self, cwd: &str) -> Result<PersistedSession, SessionStorageError>;

    async fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError>;

    async fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError>;

    async fn summaries(
        &self,
        cwd: Option<&str>,
    ) -> Result<Vec<SessionSummary>, SessionStorageError>;

    async fn fork(
        &self,
        source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError>;

    async fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError>;

    /// Remove one resolved session directory.
    async fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError>;

    async fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError>;

    async fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    async fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError>;

    /// Persist the session's last executed model record.
    async fn set_last_model(
        &self,
        session_dir: &Path,
        model: Option<&SessionModelRef>,
    ) -> Result<(), SessionStorageError>;

    /// Persist the session's world-state baseline (F-04 slice 2). `None`
    /// clears it, forcing full re-injection on the next run.
    async fn set_world_state_baseline(
        &self,
        session_dir: &Path,
        facts: Option<&WorldStateFacts>,
    ) -> Result<(), SessionStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
        tokens_before: u64,
        details: Option<serde_json::Value>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    async fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    async fn set_selected_agent(
        &self,
        session_dir: &Path,
        agent_instance_id: &str,
        updated_at: i64,
    ) -> Result<(), SessionStorageError>;
}
