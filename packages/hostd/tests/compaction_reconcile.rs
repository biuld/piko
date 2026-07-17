mod support;

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use piko_hostd::api::{Command, ServerMessage as Event};
use piko_hostd::domain::config::HostSettings;
use piko_hostd::infra::storage::JsonlSessionRepository;
use piko_hostd::infra::storage::session_store::SessionStore;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::messages::{Message, Model};
use piko_protocol::model::{ModelCapabilities, ModelRunSettings};
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, execution_running, execution_succeeded, successful_turn_run};
use tokio_util::sync::CancellationToken;

struct SummaryGateway;

#[async_trait]
impl LlmGateway for SummaryGateway {
    async fn chat_stream(
        &self,
        _req: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Err("not used".into())
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        Ok("## Goal\n- test compact\n".into())
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
}

struct CompactAgentRunRunner;

#[async_trait]
impl AgentRunRunner for CompactAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let store = SessionStore::new(&input.session_dir);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = input.operation_id.clone();
        let turn_id = input.operation_id.clone();
        let prompt = input.prompt.clone();

        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "user-1".into(),
                    parent_message_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "agent-1",
            )
            .unwrap();
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    message: Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: "world".into(),
                        }],
                        api: "test".into(),
                        provider: "test-provider".into(),
                        model: "test-model".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "agent-1", 0, execution_running());
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: "user-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: "assistant-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                3,
                execution_succeeded(),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            3,
            std::time::Duration::ZERO,
        ))
    }
}

#[tokio::test]
async fn session_compact_emits_session_reconciled_when_history_rewritten() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    // Default compaction thresholds keep short transcripts below the auto-compact
    // waterline so root chat does not consume the opportunity; SessionCompact
    // still forces a rewrite via context_window = 0.
    let server = HostServer::with_storage_runner_settings(
        repo,
        Arc::new(CompactAgentRunRunner),
        HostSettings::default(),
    );
    server.set_model_executor(Arc::new(SummaryGateway)).await;

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created");

    let turn_events = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "hello".into(),
        })
        .await;
    assert!(
        turn_events.iter().any(|event| matches!(
            event,
            Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed { .. })
        )),
        "turn must complete before compact; events={turn_events:?}"
    );
    assert!(
        turn_events
            .iter()
            .all(|event| !matches!(event, Event::SessionReconciled(_))),
        "short transcript must not auto-compact during the turn"
    );

    let compact_events = server
        .handle_command(Command::SessionCompact {
            command_id: "compact".into(),
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
        })
        .await;

    let reconciled = compact_events.iter().find_map(|event| match event {
        Event::SessionReconciled(reconciled) => Some(reconciled),
        _ => None,
    });
    let Some(reconciled) = reconciled else {
        panic!(
            "compact that rewrites the view must emit SessionReconciled; events={compact_events:?}"
        );
    };
    assert_eq!(
        reconciled.reason,
        piko_protocol::ReconcileReason::ExplicitRefresh
    );
    assert!(
        reconciled
            .snapshot
            .entries
            .iter()
            .any(|entry| matches!(entry, piko_hostd::api::SessionTreeEntry::Compaction(_))),
        "reconciled snapshot should include compaction entry; entries={:?}",
        reconciled.snapshot.entries
    );
}
