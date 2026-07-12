use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use orchd_api::{ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::{CommitAck, CommitError, ExecutionOutcomeCommit};

use crate::infra::storage::session_store::SessionStore;

pub(super) struct ExecutionCommitRouter {
    routes: std::sync::Mutex<HashMap<String, Arc<dyn ExecutionCommitPort>>>,
    fallback: Option<Arc<dyn ExecutionCommitPort>>,
}

#[derive(Default)]
pub(super) struct RealtimeDeltaRouter {
    hubs: std::sync::Mutex<HashMap<String, Arc<orchd::testing::SessionOutputHub>>>,
    default_hub: std::sync::Mutex<Option<Arc<orchd::testing::SessionOutputHub>>>,
}

impl RealtimeDeltaRouter {
    pub(super) fn register(
        &self,
        execution_id: String,
        hub: Arc<orchd::testing::SessionOutputHub>,
    ) {
        self.hubs
            .lock()
            .unwrap()
            .insert(execution_id, Arc::clone(&hub));
        *self.default_hub.lock().unwrap() = Some(hub);
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope) {
        let hub = self
            .hubs
            .lock()
            .unwrap()
            .get(&delta.work_id)
            .cloned()
            .or_else(|| self.default_hub.lock().unwrap().clone());
        if let Some(hub) = hub {
            hub.try_publish_delta(delta);
        }
    }
}

impl ExecutionCommitRouter {
    pub(super) fn new(fallback: Option<Arc<dyn ExecutionCommitPort>>) -> Self {
        Self {
            routes: std::sync::Mutex::new(HashMap::new()),
            fallback,
        }
    }

    pub(super) fn register(&self, execution_id: String, port: Arc<dyn ExecutionCommitPort>) {
        self.routes.lock().unwrap().insert(execution_id, port);
    }

    fn route(&self, execution_id: &str) -> Option<Arc<dyn ExecutionCommitPort>> {
        self.routes
            .lock()
            .unwrap()
            .get(execution_id)
            .cloned()
            .or_else(|| self.fallback.clone())
    }
}

#[async_trait]
impl ExecutionCommitPort for ExecutionCommitRouter {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        self.route(&commit.execution_id)
            .ok_or(CommitError::Unavailable)?
            .commit_message(commit)
            .await
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        self.route(&commit.execution_id)
            .ok_or(CommitError::Unavailable)?
            .commit_execution_outcome(commit)
            .await
    }
}

pub(super) struct RepositoryExecutionCommitPort {
    pub(super) store: SessionStore,
}

#[async_trait]
impl ExecutionCommitPort for RepositoryExecutionCommitPort {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        let manifest = self
            .store
            .load_manifest()
            .map_err(|error| CommitError::Failed(error.to_string()))?;
        let agent_spec_id = manifest
            .agents
            .get(&commit.agent_instance_id)
            .map(|agent| agent.identity.agent_spec_id.clone())
            .ok_or(CommitError::IdentityMismatch)?;
        self.store.commit_message(commit, &agent_spec_id)
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        let revision = self
            .store
            .load_agent(&commit.session_id, &commit.agent_instance_id)
            .map(|agent| agent.last_transcript_seq)
            .unwrap_or_default();
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: None,
            revision,
        })
    }
}
