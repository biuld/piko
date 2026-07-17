//! Outbound port for session CRUD/tree-mutation storage.
//!
//! `application` reaches session directory storage (create/open/list/fork/
//! import, tree-entry append, navigation) only through
//! [`SessionRepositoryPort`]; the concrete JSONL-backed implementation
//! (`JsonlSessionRepository`) lives in `infra::storage` and is wired up as
//! a port implementation in `adapters::storage`.

use std::path::Path;

use super::storage_types::{PersistedSession, SessionStorageError};
use crate::api::{SessionSummary, SessionTreeEntry};

pub trait SessionRepositoryPort: Send + Sync {
    fn create(&self, cwd: &str) -> Result<PersistedSession, SessionStorageError>;

    fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError>;

    fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError>;

    fn summaries(&self, cwd: Option<&str>) -> Result<Vec<SessionSummary>, SessionStorageError>;

    fn fork(
        &self,
        source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError>;

    fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError>;

    fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError>;

    fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError>;

    fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError>;

    fn set_selected_agent(
        &self,
        session_dir: &Path,
        agent_instance_id: &str,
        updated_at: i64,
    ) -> Result<(), SessionStorageError>;
}
