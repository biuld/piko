use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::unbounded_channel;

use orchd_api::{AgentCommitPort, ExecutionCommitPort};
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::api::ServerMessage;

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};
use super::{ActiveTurnRuntime, remove_active_turn_if_matches};

struct FailingAgentCommitPort;

struct NoopExecutionCommitPort;

#[async_trait]
impl ExecutionCommitPort for NoopExecutionCommitPort {
    async fn commit_message(
        &self,
        _commit: piko_protocol::execution::MessageCommit,
    ) -> Result<piko_protocol::CommitAck, CommitError> {
        Ok(piko_protocol::CommitAck {
            session_id: "session".into(),
            execution_id: "exec".into(),
            agent_instance_id: "root".into(),
            message_id: None,
            revision: 1,
        })
    }
}

#[async_trait]
impl AgentCommitPort for FailingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        _session_id: &str,
        _command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        Err(CommitError::Unavailable)
    }
}

fn create_command() -> AgentDurableCommand {
    AgentDurableCommand::Create {
        identity: AgentInstanceIdentity {
            session_id: "session".into(),
            agent_instance_id: "child".into(),
            agent_spec_id: "worker".into(),
            parent_agent_instance_id: Some("root".into()),
        },
        spec: AgentSpec {
            id: "worker".into(),
            name: "Worker".into(),
            role: "worker".into(),
            description: None,
            system_prompt: "work".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
    }
}

#[tokio::test]
async fn agent_projection_is_emitted_only_after_durable_ack() {
    let (event_tx, mut event_rx) = unbounded_channel();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));
    let committing = ProjectingAgentCommitPort::new(
        Arc::new(EphemeralAgentCommitPort::default()),
        &[],
        Arc::clone(&event_tx),
    );
    committing
        .commit_agent_command("session", create_command())
        .await
        .unwrap();
    assert!(matches!(
        event_rx.try_recv(),
        Ok(ServerMessage::AgentChanged(info)) if info.agent_instance_id == "child"
    ));

    let failing = ProjectingAgentCommitPort::new(
        Arc::new(FailingAgentCommitPort),
        &[],
        Arc::clone(&event_tx),
    );
    assert!(
        failing
            .commit_agent_command("session", create_command())
            .await
            .is_err()
    );
    assert!(event_rx.try_recv().is_err());
}

#[test]
fn stale_turn_acknowledgement_cannot_remove_newer_runtime_scope() {
    let mut active = std::collections::HashMap::from([(
        "session".into(),
        ActiveTurnRuntime {
            turn_id: "turn-new".into(),
            observation: Arc::new(orchd::testing::SessionOutputHub::new(
                "session".into(),
                "epoch".into(),
                4,
            )),
            durable_commit: Arc::new(NoopExecutionCommitPort),
        },
    )]);

    assert!(remove_active_turn_if_matches(&mut active, "session", "turn-old").is_none());
    assert_eq!(active["session"].turn_id, "turn-new");
    assert!(remove_active_turn_if_matches(&mut active, "session", "turn-new").is_some());
    assert!(active.is_empty());
}
