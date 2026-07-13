use std::sync::Arc;

use async_trait::async_trait;

use orchd_api::{ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::{CommitAck, CommitError};

use crate::infra::storage::session_store::SessionStore;

pub(super) struct ExecutionCommitRouter {
    port: std::sync::Mutex<Option<Arc<dyn ExecutionCommitPort>>>,
}

#[derive(Default)]
pub(super) struct RealtimeDeltaRouter {
    active: std::sync::Mutex<Option<(String, Arc<orchd::testing::SessionOutputHub>)>>,
}

impl RealtimeDeltaRouter {
    pub(super) fn install(&self, turn_id: String, hub: Arc<orchd::testing::SessionOutputHub>) {
        *self.active.lock().unwrap() = Some((turn_id, hub));
    }

    pub(super) fn clear_if_turn(&self, turn_id: &str) {
        let mut active = self.active.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|(active_turn, _)| active_turn == turn_id)
        {
            active.take();
        }
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope) {
        let hub = self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .map(|(_, hub)| hub.clone());
        if let Some(hub) = hub {
            hub.try_publish_delta(delta);
        }
    }
}

impl ExecutionCommitRouter {
    pub(super) fn new(port: Arc<dyn ExecutionCommitPort>) -> Self {
        Self {
            port: std::sync::Mutex::new(Some(port)),
        }
    }

    pub(super) fn install(&self, port: Arc<dyn ExecutionCommitPort>) {
        *self.port.lock().unwrap() = Some(port);
    }

    fn port(&self) -> Option<Arc<dyn ExecutionCommitPort>> {
        self.port.lock().unwrap().clone()
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct CountingPort(AtomicU64);

    #[async_trait]
    impl ExecutionCommitPort for CountingPort {
        async fn commit_message(
            &self,
            commit: piko_protocol::execution::MessageCommit,
        ) -> Result<CommitAck, CommitError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(CommitAck {
                session_id: commit.session_id,
                execution_id: commit.execution_id,
                agent_instance_id: commit.agent_instance_id,
                message_id: Some(commit.message_id),
                revision: 1,
            })
        }
    }

    #[tokio::test]
    async fn commit_router_switches_away_from_prior_turn_sink() {
        let first = Arc::new(CountingPort(AtomicU64::new(0)));
        let second = Arc::new(CountingPort(AtomicU64::new(0)));
        let router = ExecutionCommitRouter::new(first.clone());
        router.install(second.clone());
        router
            .commit_message(piko_protocol::execution::MessageCommit {
                session_id: "session".into(),
                source_turn_id: Some("turn-2".into()),
                execution_id: "exec".into(),
                agent_instance_id: "root".into(),
                message_id: "message".into(),
                parent_message_id: None,
                message: piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String("hello".into()),
                    timestamp: None,
                },
                committed_at: 1,
            })
            .await
            .unwrap();

        assert_eq!(first.0.load(Ordering::SeqCst), 0);
        assert_eq!(second.0.load(Ordering::SeqCst), 1);
    }
}
