use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use hostd::adapters::OrchAgentRunRunner;
use hostd::api::{Command, CommandResult, ServerMessage};
use hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use hostd::protocol::HostServer;
use llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use orchd_api::AgentCommitPort;
use piko_protocol::{
    AgentDurableCommand, AgentInstanceIdentity, AgentSpec, Message, MessageContent,
};
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

struct DirectChatGateway;

#[async_trait]
impl LlmGateway for DirectChatGateway {
    async fn chat_stream(
        &self,
        _request: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Ok(Box::pin(iter(vec![
            GatewayEvent::ContentDelta("direct reply".into()),
            GatewayEvent::Usage(piko_protocol::Usage::empty()),
            GatewayEvent::Done("stop".into()),
        ])))
    }

    async fn llm_call(
        &self,
        _model: piko_protocol::Model,
        _system_prompt: Option<String>,
        _messages: Vec<piko_protocol::Message>,
        _settings: piko_protocol::ModelRunSettings,
    ) -> Result<String, String> {
        Ok("direct reply".into())
    }

    fn capabilities(&self) -> piko_protocol::ModelCapabilities {
        piko_protocol::ModelCapabilities::default()
    }
}

#[tokio::test]
async fn child_transcript_and_selected_view_persist_independently() {
    let temp = tempfile::tempdir().unwrap();
    let initial = HostServer::with_storage(JsonlSessionRepository::new(temp.path()));
    let created = initial
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap();
    let listed = initial
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let session_path = listed
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionListed { sessions, .. }),
                ..
            } => sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .and_then(|session| session.session_path.clone()),
            _ => None,
        })
        .unwrap();
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    let root_agent_instance_id = root.agent_instance_id.clone();
    store
        .commit_message(
            piko_protocol::execution::MessageCommit {
                session_id: session_id.clone(),
                source_turn_id: Some("turn-root".into()),
                execution_id: "exec-root".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "message-root".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("root history".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: session_id.clone(),
                    agent_instance_id: "agent-child".into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root_agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    name: "Worker".into(),
                    role: "worker".into(),
                    description: None,
                    base_system_prompt: "Reply directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();

    let runner = Arc::new(
        OrchAgentRunRunner::new(
            Arc::new(DirectChatGateway),
            "test",
            "test-key",
            "test-model",
        )
        .await,
    );
    let server =
        HostServer::with_storage_and_runner(JsonlSessionRepository::new(temp.path()), runner);
    server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path.clone()),
        })
        .await;
    server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id: session_id.clone(),
            agent_instance_id: "agent-child".into(),
            after_seq: None,
        })
        .await;

    let events = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.handle_command(Command::ChatSubmit {
            command_id: "direct".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: "agent-child".into(),
            text: "follow up".into(),
        }),
    )
    .await
    .expect("direct child run should finish");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::CommandResponse {
            command_id,
            result: Ok(CommandResult::Empty),
        } if command_id == "direct"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::TurnLifecycle(piko_protocol::TurnEvent::Completed {
            agent_instance_id,
            ..
        }) if agent_instance_id == "agent-child"
    )));
    let child_commits: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ServerMessage::TranscriptCommitted(committed)
                if committed.agent_instance_id == "agent-child" =>
            {
                Some(committed)
            }
            _ => None,
        })
        .collect();
    assert_eq!(child_commits.len(), 2);
    server
        .handle_command(Command::AgentSubscribe {
            command_id: "return-root".into(),
            session_id: session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            after_seq: None,
        })
        .await;
    let reopened = HostServer::with_storage(JsonlSessionRepository::new(temp.path()));
    let opened = reopened
        .handle_command(Command::SessionOpen {
            command_id: "reopen".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path),
        })
        .await;
    assert!(opened.iter().any(|event| matches!(
        event,
        ServerMessage::SessionReconciled(reconciled)
            if reconciled.snapshot.selected_agent_instance_id.as_deref()
                == Some(root_agent_instance_id.as_str())
                && reconciled.snapshot.current_leaf_id.as_deref() == Some("message-root")
    )));
    let child_view = reopened
        .handle_command(Command::AgentSubscribe {
            command_id: "reopen-child".into(),
            session_id,
            agent_instance_id: "agent-child".into(),
            after_seq: None,
        })
        .await;
    assert!(child_view.iter().any(|event| matches!(
        event,
        ServerMessage::CommandResponse {
            result: Ok(CommandResult::AgentSubscribed { snapshot, .. }),
            ..
        } if snapshot.events.iter().filter(|event| matches!(
            event.message.as_ref(),
            ServerMessage::TranscriptCommitted(_)
        )).count() == 2
    )));
}
