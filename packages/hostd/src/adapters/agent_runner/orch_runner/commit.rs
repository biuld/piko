use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{AgentCommitPort, ExecutionCommitPort, RealtimeDeltaSink};
use piko_protocol::AgentInputDispositionChange;
use piko_protocol::MessageRole;
use piko_protocol::agent_runtime::{RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope};
use piko_protocol::agent_work::{CommitAck, CommitError, MessageCommit, ModelStepCommit};

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

    pub(super) async fn commit_agent_command(
        &self,
        command: piko_protocol::AgentDurableCommand,
    ) -> Result<piko_protocol::AgentCommitAck, piko_protocol::CommitError> {
        self.store
            .commit_agent_command(&self.session_id, command)
            .await
    }
}

impl super::OrchAgentRunRunner {
    pub(super) async fn commit_agent_work_fact(
        &self,
        session_id: &str,
        command: piko_protocol::AgentDurableCommand,
    ) -> Result<piko_protocol::AgentCommitAck, piko_protocol::CommitError> {
        let router = self.commit_routers.lock().unwrap().get(session_id).cloned();
        let Some(router) = router else {
            return Err(piko_protocol::CommitError::Failed(format!(
                "no durable route for session {session_id}"
            )));
        };
        router.commit_agent_command(command).await
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
                        root_input_id: commit.root_input_id,
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

    async fn commit_steer(
        &self,
        commit: MessageCommit,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError> {
        let role = MessageRole::User;
        let ack = self.durable.commit_steer(commit.clone(), change).await?;
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
                        root_input_id: commit.root_input_id,
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

    async fn commit_steer(
        &self,
        commit: MessageCommit,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError> {
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
                store.commit_steer_under_lock(commit, &agent_spec_id, change)
            })
            .await
    }
}
