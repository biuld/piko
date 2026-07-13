use std::sync::Arc;

use async_trait::async_trait;

use orchd_api::{ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::{CommitAck, CommitError};

use crate::infra::storage::session_store::SessionStore;

pub(super) struct ExecutionCommitRouter {
    fallback: Option<Arc<dyn ExecutionCommitPort>>,
}

#[derive(Default)]
pub(super) struct RealtimeDeltaRouter {
    default_hub: std::sync::Mutex<Option<Arc<orchd::testing::SessionOutputHub>>>,
}

impl RealtimeDeltaRouter {
    pub(super) fn set_default(&self, hub: Arc<orchd::testing::SessionOutputHub>) {
        *self.default_hub.lock().unwrap() = Some(hub);
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope) {
        let hub = self.default_hub.lock().unwrap().clone();
        if let Some(hub) = hub {
            hub.try_publish_delta(delta);
        }
    }
}

impl ExecutionCommitRouter {
    pub(super) fn new(fallback: Option<Arc<dyn ExecutionCommitPort>>) -> Self {
        Self { fallback }
    }

    fn port(&self) -> Option<Arc<dyn ExecutionCommitPort>> {
        self.fallback.clone()
    }
}

#[async_trait]
impl ExecutionCommitPort for ExecutionCommitRouter {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        self.port()
            .ok_or(CommitError::Unavailable)?
            .commit_message(commit)
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
}
