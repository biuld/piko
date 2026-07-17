use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use orchd::events::SessionOutputHub;
use orchd_api::{ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::MessageRole;
use piko_protocol::agent_runtime::{RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope};
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};

use crate::infra::storage::session_store::SessionStore;

struct ObservationRoute {
    operation_id: String,
    hub: Arc<SessionOutputHub>,
    fallback: bool,
}

#[derive(Default)]
struct ObservationRoutes {
    by_agent: HashMap<String, ObservationRoute>,
}

impl ObservationRoutes {
    fn register(
        &mut self,
        agent_instance_id: String,
        operation_id: String,
        hub: Arc<SessionOutputHub>,
        fallback: bool,
    ) {
        self.by_agent.insert(
            agent_instance_id,
            ObservationRoute {
                operation_id,
                hub,
                fallback,
            },
        );
    }

    fn unregister(&mut self, agent_instance_id: &str, operation_id: &str) {
        if self
            .by_agent
            .get(agent_instance_id)
            .is_some_and(|route| route.operation_id == operation_id)
        {
            self.by_agent.remove(agent_instance_id);
        }
    }

    fn hub_for(&self, agent_instance_id: &str) -> Option<Arc<SessionOutputHub>> {
        self.by_agent
            .get(agent_instance_id)
            .map(|route| Arc::clone(&route.hub))
            .or_else(|| {
                self.by_agent
                    .values()
                    .find(|route| route.fallback)
                    .map(|route| Arc::clone(&route.hub))
            })
    }
}

/// Stable per-Session commit port. Operation routes are registered by concrete
/// AgentInstance, so concurrent Agents never replace each other's sink.
pub(super) struct ExecutionCommitRouter {
    durable: Arc<dyn ExecutionCommitPort>,
    store: SessionStore,
    routes: std::sync::Mutex<ObservationRoutes>,
}

impl ExecutionCommitRouter {
    pub(super) fn new(durable: Arc<dyn ExecutionCommitPort>, store: SessionStore) -> Self {
        Self {
            durable,
            store,
            routes: std::sync::Mutex::new(ObservationRoutes::default()),
        }
    }

    pub(super) fn register(
        &self,
        agent_instance_id: String,
        operation_id: String,
        hub: Arc<SessionOutputHub>,
        fallback: bool,
    ) {
        self.routes
            .lock()
            .unwrap()
            .register(agent_instance_id, operation_id, hub, fallback);
    }

    pub(super) fn unregister(&self, agent_instance_id: &str, operation_id: &str) {
        self.routes
            .lock()
            .unwrap()
            .unregister(agent_instance_id, operation_id);
    }
}

#[async_trait]
impl ExecutionCommitPort for ExecutionCommitRouter {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let role = match &commit.message {
            piko_protocol::Message::User { .. } => MessageRole::User,
            piko_protocol::Message::Assistant { .. } => MessageRole::Assistant,
            piko_protocol::Message::ToolCall { .. } | piko_protocol::Message::ToolResult { .. } => {
                MessageRole::ToolResult
            }
        };
        let ack = self.durable.commit_message(commit.clone()).await?;
        let hub = self
            .routes
            .lock()
            .unwrap()
            .hub_for(&commit.agent_instance_id);
        if let Some(hub) = hub {
            let agent_id = self
                .store
                .load_manifest()
                .ok()
                .and_then(|manifest| {
                    manifest
                        .agents
                        .get(&commit.agent_instance_id)
                        .map(|agent| agent.identity.agent_spec_id.clone())
                })
                .unwrap_or_else(|| "unknown".into());
            let _ = hub
                .publish_event(SessionEventEnvelope {
                    agent_instance_id: commit.agent_instance_id,
                    agent_id,
                    transcript_seq: ack.revision,
                    cursor: hub.cursor(),
                    event: SessionEvent::MessageCommitted {
                        message_id: commit.message_id,
                        source_turn_id: commit.source_turn_id.unwrap_or_default(),
                        role,
                    },
                })
                .await;
        }
        Ok(ack)
    }
}

/// Stable per-Session realtime port with the same Agent routing semantics as
/// committed observation.
#[derive(Default)]
pub(super) struct RealtimeDeltaRouter {
    routes: std::sync::Mutex<ObservationRoutes>,
}

impl RealtimeDeltaRouter {
    pub(super) fn register(
        &self,
        agent_instance_id: String,
        operation_id: String,
        hub: Arc<SessionOutputHub>,
        fallback: bool,
    ) {
        self.routes
            .lock()
            .unwrap()
            .register(agent_instance_id, operation_id, hub, fallback);
    }

    pub(super) fn unregister(&self, agent_instance_id: &str, operation_id: &str) {
        self.routes
            .lock()
            .unwrap()
            .unregister(agent_instance_id, operation_id);
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: RealtimeDeltaEnvelope) {
        let hub = self
            .routes
            .lock()
            .unwrap()
            .hub_for(&delta.agent_instance_id);
        if let Some(hub) = hub {
            hub.try_publish_delta(delta);
        }
    }
}

pub(super) struct RepositoryExecutionCommitPort {
    pub(super) store: SessionStore,
}

#[async_trait]
impl ExecutionCommitPort for RepositoryExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        self.store
            .run_durable(move |store| {
                let manifest = store
                    .load_manifest()
                    .map_err(|error| CommitError::Failed(error.to_string()))?;
                let agent_spec_id = manifest
                    .agents
                    .get(&commit.agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .ok_or(CommitError::IdentityMismatch)?;
                store.commit_message_under_lock(commit, &agent_spec_id)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_agent_route_wins_over_fallback() {
        let root = Arc::new(SessionOutputHub::new("s".into(), "root".into(), 4));
        let child = Arc::new(SessionOutputHub::new("s".into(), "child".into(), 4));
        let mut routes = ObservationRoutes::default();
        routes.register("root".into(), "turn".into(), root, true);
        routes.register("child".into(), "run".into(), Arc::clone(&child), false);
        assert_eq!(routes.hub_for("child").unwrap().cursor().epoch, "child");
        assert_eq!(routes.hub_for("other").unwrap().cursor().epoch, "root");
    }
}
