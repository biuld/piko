//! [`SessionRepositoryPort`] implementation wrapping the JSONL-backed
//! [`JsonlSessionRepository`].

use std::path::Path;

use crate::api::{SessionSummary, SessionTreeEntry};
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::storage_types::{PersistedSession, SessionStorageError};

impl SessionRepositoryPort for JsonlSessionRepository {
    fn create(&self, cwd: &str) -> Result<PersistedSession, SessionStorageError> {
        JsonlSessionRepository::create(self, cwd)
    }

    fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        JsonlSessionRepository::load_by_path(self, path)
    }

    fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError> {
        JsonlSessionRepository::list(self, cwd)
    }

    fn summaries(&self, cwd: Option<&str>) -> Result<Vec<SessionSummary>, SessionStorageError> {
        JsonlSessionRepository::summaries(self, cwd)
    }

    fn fork(
        &self,
        source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        JsonlSessionRepository::fork(self, source_id, source_dir, entry_id)
    }

    fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError> {
        JsonlSessionRepository::import(self, input_path)
    }

    fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        JsonlSessionRepository::append_entry(self, session_dir, entry, agent_id)
    }

    fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        JsonlSessionRepository::append_session_info(self, session_dir, parent_id, name, agent_id)
    }

    fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
        JsonlSessionRepository::append_config_metadata(
            self,
            session_dir,
            parent_id,
            model_id,
            provider,
            thinking_level,
            agent_id,
        )
    }

    fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        JsonlSessionRepository::append_compaction(
            self,
            session_dir,
            parent_id,
            summary,
            first_kept_entry_id,
            agent_id,
        )
    }

    fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        JsonlSessionRepository::navigate(self, session_dir, parent_id, target_id, agent_id)
    }
}
