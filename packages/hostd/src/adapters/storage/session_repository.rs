//! Async [`SessionRepositoryPort`] adapter for the synchronous JSONL repository.

use std::path::Path;

use async_trait::async_trait;

use super::blocking::StorageBlockingPool;
use crate::api::{SessionSummary, SessionTreeEntry};
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::storage_types::{PersistedSession, SessionStorageError};

#[async_trait]
impl SessionRepositoryPort for JsonlSessionRepository {
    async fn create(&self, cwd: &str) -> Result<PersistedSession, SessionStorageError> {
        let repository = self.clone();
        let cwd = cwd.to_string();
        pool().run(move || repository.create(&cwd)).await
    }

    async fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        let repository = self.clone();
        let path = path.to_path_buf();
        pool().run(move || repository.load_by_path(&path)).await
    }

    async fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError> {
        let repository = self.clone();
        let cwd = cwd.map(str::to_string);
        pool().run(move || repository.list(cwd.as_deref())).await
    }

    async fn summaries(
        &self,
        cwd: Option<&str>,
    ) -> Result<Vec<SessionSummary>, SessionStorageError> {
        let repository = self.clone();
        let cwd = cwd.map(str::to_string);
        pool()
            .run(move || repository.summaries(cwd.as_deref()))
            .await
    }

    async fn fork(
        &self,
        source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        let repository = self.clone();
        let source_id = source_id.to_string();
        let source_dir = source_dir.to_path_buf();
        let entry_id = entry_id.map(str::to_string);
        pool()
            .run(move || repository.fork(&source_id, &source_dir, entry_id.as_deref()))
            .await
    }

    async fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError> {
        let repository = self.clone();
        let input_path = input_path.to_path_buf();
        pool().run(move || repository.import(&input_path)).await
    }

    async fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        pool().run(move || repository.delete(&session_dir)).await
    }

    async fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let entry = entry.clone();
        let agent_id = agent_id.map(str::to_string);
        pool()
            .run(move || repository.append_entry(&session_dir, &entry, agent_id.as_deref()))
            .await
    }

    async fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let parent_id = parent_id.map(str::to_string);
        let name = name.to_string();
        let agent_id = agent_id.map(str::to_string);
        pool()
            .run(move || {
                repository.append_session_info(
                    &session_dir,
                    parent_id.as_deref(),
                    &name,
                    agent_id.as_deref(),
                )
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let parent_id = owned(parent_id);
        let model_id = owned(model_id);
        let provider = owned(provider);
        let thinking_level = owned(thinking_level);
        let agent_id = owned(agent_id);
        pool()
            .run(move || {
                repository.append_config_metadata(
                    &session_dir,
                    parent_id.as_deref(),
                    model_id.as_deref(),
                    provider.as_deref(),
                    thinking_level.as_deref(),
                    agent_id.as_deref(),
                )
            })
            .await
    }

    async fn set_last_model(
        &self,
        session_dir: &Path,
        model: Option<&crate::domain::sessions::SessionModelRef>,
    ) -> Result<(), SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let model = model.cloned();
        pool()
            .run(move || repository.set_last_model(&session_dir, model.as_ref()))
            .await
    }

    async fn set_world_state_baseline(
        &self,
        session_dir: &Path,
        facts: Option<&crate::domain::prompts::WorldStateFacts>,
    ) -> Result<(), SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let facts = facts.cloned();
        pool()
            .run(move || repository.set_world_state_baseline(&session_dir, facts.as_ref()))
            .await
    }

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
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let parent_id = owned(parent_id);
        let summary = summary.to_string();
        let first_kept_entry_id = first_kept_entry_id.to_string();
        let agent_id = owned(agent_id);
        pool()
            .run(move || {
                repository.append_compaction(
                    &session_dir,
                    parent_id.as_deref(),
                    &summary,
                    &first_kept_entry_id,
                    agent_id.as_deref(),
                    tokens_before,
                    details,
                )
            })
            .await
    }

    async fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let parent_id = owned(parent_id);
        let target_id = owned(target_id);
        let agent_id = owned(agent_id);
        pool()
            .run(move || {
                repository.navigate(
                    &session_dir,
                    parent_id.as_deref(),
                    target_id.as_deref(),
                    agent_id.as_deref(),
                )
            })
            .await
    }

    async fn set_selected_agent(
        &self,
        session_dir: &Path,
        agent_instance_id: &str,
        updated_at: i64,
    ) -> Result<(), SessionStorageError> {
        let repository = self.clone();
        let session_dir = session_dir.to_path_buf();
        let agent_instance_id = agent_instance_id.to_string();
        pool()
            .run(move || {
                repository.set_selected_agent(&session_dir, &agent_instance_id, updated_at)
            })
            .await
    }
}

fn pool() -> StorageBlockingPool {
    StorageBlockingPool::default()
}

fn owned(value: Option<&str>) -> Option<String> {
    value.map(str::to_string)
}
