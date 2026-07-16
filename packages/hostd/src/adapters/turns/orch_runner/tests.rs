use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

use orchd_api::AgentCommitPort;
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::api::ServerMessage;
use crate::infra::storage::SessionStore;
use crate::ports::{AgentOperationAddress, AgentRunInput, AgentRunRunner};

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};
use super::run::{ensure_root_tool_sets, resolve_recovered_agent_spec};

struct FailingAgentCommitPort;

struct DirectInputGateway;

#[async_trait]
impl LlmGateway for DirectInputGateway {
    async fn chat_stream(
        &self,
        _: GatewayRequest,
        _: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Ok(Box::pin(iter(vec![
            GatewayEvent::ContentDelta("child reply".into()),
            GatewayEvent::Usage(piko_protocol::Usage::empty()),
            GatewayEvent::Done("stop".into()),
        ])))
    }

    async fn llm_call(
        &self,
        _: piko_protocol::Model,
        _: Option<String>,
        _: Vec<piko_protocol::Message>,
        _: piko_protocol::ModelRunSettings,
    ) -> Result<String, String> {
        Ok("child reply".into())
    }

    fn capabilities(&self) -> piko_protocol::ModelCapabilities {
        piko_protocol::ModelCapabilities::default()
    }
}

#[async_trait]
impl AgentCommitPort for FailingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        _: &str,
        _: AgentDurableCommand,
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
            base_system_prompt: "work".into(),
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
    let event_router = Arc::new(super::ui_router::UiEventRouter::default());
    event_router.register("session-1", "operation", "child", true, event_tx);
    let committing = ProjectingAgentCommitPort::new(
        Arc::new(EphemeralAgentCommitPort::default()),
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
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
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
    );
    assert!(
        failing
            .commit_agent_command("session", create_command())
            .await
            .is_err()
    );
    assert!(event_rx.try_recv().is_err());
}

#[tokio::test]
async fn direct_input_runs_the_addressed_recovered_child_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        SessionStore::create_session(temp.path(), "session-direct".into(), "/project".into(), 1)
            .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child_id = "agent-child";
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: child_id.into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    name: "Worker".into(),
                    role: "worker".into(),
                    description: None,
                    base_system_prompt: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: "agent-child-two".into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    name: "Worker".into(),
                    role: "worker".into(),
                    description: None,
                    base_system_prompt: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();

    let runner = super::OrchAgentRunRunner::new(
        Arc::new(DirectInputGateway),
        "test",
        "test-key",
        "test-model",
    )
    .await;
    let (ui_event_tx, _ui_event_rx) = unbounded_channel();
    let run = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-direct".into(),
            agent_instance_id: child_id.into(),
            prompt: "follow up".into(),
            source_turn_id: Some("run-direct".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            ui_event_tx,
            resume_agent: None,
        })
        .await
        .unwrap();
    AgentRunRunner::finish_agent_run(
        &runner,
        &AgentOperationAddress {
            session_id: "session-direct".into(),
            operation_id: "stale-run-id".into(),
            agent_instance_id: child_id.into(),
        },
        &piko_protocol::agent_runtime::SessionCursor {
            epoch: "stale".into(),
            seq: 0,
        },
    )
    .await;
    let (duplicate_ui_tx, _duplicate_ui_rx) = unbounded_channel();
    let duplicate = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-duplicate".into(),
            agent_instance_id: child_id.into(),
            prompt: "duplicate".into(),
            source_turn_id: Some("run-duplicate".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            ui_event_tx: duplicate_ui_tx,
            resume_agent: None,
        })
        .await;
    assert!(matches!(
        duplicate,
        Err(crate::api::ProtocolError::InvalidCommand(_))
    ));
    let (second_ui_tx, _second_ui_rx) = unbounded_channel();
    let second = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-second-child".into(),
            agent_instance_id: "agent-child-two".into(),
            prompt: "parallel".into(),
            source_turn_id: Some("run-second-child".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            ui_event_tx: second_ui_tx,
            resume_agent: None,
        })
        .await
        .expect("different AgentInstances may run concurrently");
    let completed = run.completion.await.unwrap();
    let second_completed = second.completion.await.unwrap();
    assert_eq!(completed.address.agent_instance_id, child_id);
    assert!(completed.result.is_ok());
    assert!(second_completed.result.is_ok());

    let recovered = store.load_agent("session-direct", child_id).unwrap();
    assert_eq!(recovered.transcript.len(), 2);
    assert!(matches!(
        &recovered.transcript[0].message,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String(text),
            ..
        } if text == "follow up"
    ));
}

#[test]
fn ensure_root_tool_sets_adds_user_interaction_and_multi_agent() {
    let mut spec = AgentSpec {
        id: "main".into(),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        base_system_prompt: "hi".into(),
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
fn resolve_recovered_agent_spec_prefers_durable_snapshot_then_registry_fallback() {
    let root_agent_spec = AgentSpec {
        id: "main".into(),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        base_system_prompt: "stable root prompt".into(),
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
            base_system_prompt: "raw toml".into(),
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
            base_system_prompt: "code".into(),
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
    assert_eq!(root.base_system_prompt, "stable root prompt");
    assert!(root.tool_set_ids.iter().any(|id| id == "multi_agent"));
    assert!(root.tool_set_ids.iter().any(|id| id == "user_interaction"));

    let durable_root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        Some(resolved_specs["main"].clone()),
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(durable_root.base_system_prompt, "raw toml");
    assert!(
        !durable_root
            .tool_set_ids
            .iter()
            .any(|id| id == "multi_agent")
    );

    let child = resolve_recovered_agent_spec(
        "agent_coder_1",
        "agent_session_root",
        None,
        "coder",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(child.base_system_prompt, "code");
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
