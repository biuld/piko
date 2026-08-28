use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::MessageRole;
use piko_protocol::agent_runtime::{RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope};
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit, ModelStepCommit};

use crate::infra::storage::session_store::SessionStore;

/// Stable per-Session commit port. Operation routes are registered by concrete
/// AgentInstance, so concurrent Agents never replace each other's sink.
pub(super) struct ExecutionCommitRouter {
    durable: Arc<dyn ExecutionCommitPort>,
    store: SessionStore,
    session_id: String,
    routes: Arc<super::observation_router::SessionObservationRouter>,
}

impl ExecutionCommitRouter {
    pub(super) fn new(
        durable: Arc<dyn ExecutionCommitPort>,
        store: SessionStore,
        session_id: String,
        routes: Arc<super::observation_router::SessionObservationRouter>,
    ) -> Self {
        Self {
            durable,
            store,
            session_id,
            routes,
        }
    }
}

#[async_trait]
impl ExecutionCommitPort for ExecutionCommitRouter {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let role = match &commit.message {
            piko_protocol::Message::Context { .. } => MessageRole::Context,
            piko_protocol::Message::User { .. } => MessageRole::User,
            piko_protocol::Message::Assistant { .. } => MessageRole::Assistant,
            piko_protocol::Message::ToolCall { .. } | piko_protocol::Message::ToolResult { .. } => {
                MessageRole::ToolResult
            }
        };
        let ack = self.durable.commit_message(commit.clone()).await?;
        let hub = self
            .routes
            .hub_for(&self.session_id, &commit.agent_instance_id);
        if let Some(hub) = hub {
            let agent_id = self
                .store
                .load_projection()
                .ok()
                .and_then(|projection| {
                    projection
                        .agents
                        .get(&commit.agent_instance_id)
                        .map(|agent| agent.identity.agent_spec_id.clone())
                })
                .unwrap_or_else(|| "unknown".into());
            let _ = hub
                .publish_event(SessionEventEnvelope {
                    agent_instance_id: commit.agent_instance_id,
                    agent_id,
                    cursor: hub.cursor(),
                    event: SessionEvent::MessageCommitted {
                        transcript_seq: ack.revision,
                        message_id: commit.message_id,
                        source_turn_id: commit.source_turn_id.unwrap_or_default(),
                        role,
                    },
                })
                .await;
        }
        Ok(ack)
    }

    async fn commit_model_step(&self, commit: ModelStepCommit) -> Result<CommitAck, CommitError> {
        let boundary = commit.boundary();
        let ack = self.durable.commit_model_step(commit).await?;
        let hub = self
            .routes
            .hub_for(&self.session_id, &boundary.agent_instance_id);
        if let Some(hub) = hub {
            let agent_id = self
                .store
                .load_projection()
                .ok()
                .and_then(|projection| {
                    projection
                        .agents
                        .get(&boundary.agent_instance_id)
                        .map(|agent| agent.identity.agent_spec_id.clone())
                })
                .unwrap_or_else(|| "unknown".into());
            let _ = hub
                .publish_event(SessionEventEnvelope {
                    agent_instance_id: boundary.agent_instance_id.clone(),
                    agent_id,
                    cursor: hub.cursor(),
                    event: SessionEvent::ModelStepCommitted { boundary },
                })
                .await;
        }
        Ok(ack)
    }
}

/// Stable per-Session realtime port with the same Agent routing semantics as
/// committed observation.
pub(super) struct RealtimeDeltaRouter {
    session_id: String,
    routes: Arc<super::observation_router::SessionObservationRouter>,
}

impl RealtimeDeltaRouter {
    pub(super) fn new(
        session_id: String,
        routes: Arc<super::observation_router::SessionObservationRouter>,
    ) -> Self {
        Self { session_id, routes }
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: RealtimeDeltaEnvelope) {
        let hub = self
            .routes
            .hub_for(&self.session_id, &delta.agent_instance_id);
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
                let projection = store
                    .load_projection()
                    .map_err(|error| CommitError::Failed(error.to_string()))?;
                let agent_spec_id = projection
                    .agents
                    .get(&commit.agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .ok_or(CommitError::IdentityMismatch)?;
                store.commit_message_under_lock(commit, &agent_spec_id)
            })
            .await
    }

    async fn commit_model_step(&self, commit: ModelStepCommit) -> Result<CommitAck, CommitError> {
        self.store
            .run_durable(move |store| {
                let projection = store
                    .load_projection()
                    .map_err(|error| CommitError::Failed(error.to_string()))?;
                let agent_spec_id = projection
                    .agents
                    .get(&commit.agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .ok_or(CommitError::IdentityMismatch)?;
                store.commit_model_step_under_lock(commit, &agent_spec_id)
            })
            .await
    }
}
