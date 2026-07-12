use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::unbounded_channel;

use orchd_api::AgentCommitPort;
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::api::ServerMessage;

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};

struct FailingAgentCommitPort;

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
