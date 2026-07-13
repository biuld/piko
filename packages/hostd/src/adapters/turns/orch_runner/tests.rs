use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::unbounded_channel;

use orchd_api::{AgentCommitPort, ExecutionCommitPort};
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::api::ServerMessage;

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};
use super::run::{ensure_root_tool_sets, resolve_recovered_agent_spec};
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

#[test]
fn ensure_root_tool_sets_adds_user_interaction_and_multi_agent() {
    let mut spec = AgentSpec {
        id: "main".into(),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        system_prompt: "hi".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec!["todo".into(), "workspace".into()],
        active_tool_names: None,
    };
    ensure_root_tool_sets(&mut spec);
    assert_eq!(
        spec.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "user_interaction".to_string(),
            "multi_agent".to_string()
        ]
    );
}

#[test]
fn resolve_recovered_agent_spec_prefers_turn_root_and_keeps_child_toml_sets() {
    let root_agent_spec = AgentSpec {
        id: "main".into(),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        system_prompt: "composed turn prompt".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec![
            "todo".into(),
            "workspace".into(),
            "user_interaction".into(),
            "multi_agent".into(),
        ],
        active_tool_names: None,
    };
    let mut resolved_specs = std::collections::HashMap::new();
    resolved_specs.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            name: "Main".into(),
            role: "root".into(),
            description: None,
            system_prompt: "raw toml".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into()],
            active_tool_names: None,
        },
    );
    resolved_specs.insert(
        "coder".into(),
        AgentSpec {
            id: "coder".into(),
            name: "Coder".into(),
            role: "worker".into(),
            description: None,
            system_prompt: "code".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into(), "multi_agent".into()],
            active_tool_names: None,
        },
    );

    let root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        None,
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(root.system_prompt, "composed turn prompt");
    assert!(root.tool_set_ids.iter().any(|id| id == "multi_agent"));
    assert!(root.tool_set_ids.iter().any(|id| id == "user_interaction"));

    let child = resolve_recovered_agent_spec(
        "agent_coder_1",
        "agent_session_root",
        None,
        "coder",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(child.system_prompt, "code");
    assert_eq!(
        child.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "multi_agent".to_string()
        ]
    );
    assert!(!child.tool_set_ids.iter().any(|id| id == "user_interaction"));
}
