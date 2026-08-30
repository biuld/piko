//! Async [`SessionStorePort`] adapter for the synchronous session store.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::blocking::StorageBlockingPool;
use crate::infra::storage::SessionStore;
use crate::ports::session_store::{SessionStoreFactory, SessionStorePort};
use crate::ports::storage_types::{
    AgentProjection, CommittedMessage, RecoveredAgent, SessionProjection, SessionStorageError,
};

#[derive(Debug, Clone)]
struct BlockingSessionStore {
    inner: SessionStore,
    pool: StorageBlockingPool,
}

impl BlockingSessionStore {
    fn new(inner: SessionStore) -> Self {
        Self {
            inner,
            pool: StorageBlockingPool::default(),
        }
    }
}

#[async_trait]
impl SessionStorePort for BlockingSessionStore {
    async fn load_projection(&self) -> Result<SessionProjection, SessionStorageError> {
        let store = self.inner.clone();
        self.pool.run(move || store.load_projection()).await
    }

    async fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError> {
        let store = self.inner.clone();
        let session_id = session_id.to_string();
        let agent_instance_id = agent_instance_id.to_string();
        self.pool
            .run(move || store.load_agent(&session_id, &agent_instance_id))
            .await
    }

    async fn agent_instances(&self) -> Result<Vec<AgentProjection>, SessionStorageError> {
        let store = self.inner.clone();
        self.pool.run(move || store.agent_instances()).await
    }

    async fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        let store = self.inner.clone();
        let session_id = session_id.to_string();
        let agent_instance_id = agent_instance_id.to_string();
        let message_id = message_id.to_string();
        self.pool
            .run(move || store.find_committed_message(&session_id, &agent_instance_id, &message_id))
            .await
    }

    async fn agent_report_for_input(
        &self,
        root_input_id: &str,
    ) -> Result<Option<piko_protocol::AgentWorkReport>, SessionStorageError> {
        let store = self.inner.clone();
        let root_input_id = root_input_id.to_string();
        self.pool
            .run(move || store.agent_report_for_input(&root_input_id))
            .await
    }

    async fn interrupt_incomplete_agent_work(&self) -> Result<usize, SessionStorageError> {
        let store = self.inner.clone();
        self.pool
            .run(move || store.interrupt_incomplete_agent_work())
            .await
    }

    async fn trajectory(
        &self,
    ) -> Result<piko_session_store::TrajectoryProjection, SessionStorageError> {
        let store = self.inner.clone();
        self.pool.run(move || store.trajectory()).await
    }
}

/// Default factory backed by the real filesystem and the shared blocking pool.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsSessionStoreFactory;

#[async_trait]
impl SessionStoreFactory for FsSessionStoreFactory {
    fn open(&self, session_dir: &Path) -> Arc<dyn SessionStorePort> {
        Arc::new(BlockingSessionStore::new(SessionStore::new(session_dir)))
    }

    async fn create(
        &self,
        session_dir: &Path,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Arc<dyn SessionStorePort>, SessionStorageError> {
        let session_dir = session_dir.to_path_buf();
        let store = StorageBlockingPool::default()
            .run(move || {
                if let Some(parent) = session_dir.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| SessionStorageError::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                SessionStore::create_session(session_dir, session_id, cwd, created_at)
            })
            .await?;
        Ok(Arc::new(BlockingSessionStore::new(store)))
    }

    async fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError> {
        let session_dir = session_dir.to_path_buf();
        StorageBlockingPool::default()
            .run(move || {
                std::fs::remove_dir_all(&session_dir).map_err(|source| SessionStorageError::Io {
                    path: session_dir,
                    source,
                })
            })
            .await
    }
}
