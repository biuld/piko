#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::api::{
    ApprovalDecision, Command, Message, ServerMessage as Event, SessionTreeEntry,
};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::{HostServer, run_jsonl_server};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{
    MockSessionPublisher, execution_running, execution_succeeded, success_report,
    successful_turn_run, test_agent_run_process,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

struct SlowRunner;

#[async_trait]
impl AgentRunRunner for SlowRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let agent_instance_id = input.agent_instance_id.clone();
        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 0, execution_running());
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            1,
            Duration::from_millis(200),
        ))
    }
}

#[tokio::test]
async fn command_catalog_get_returns_neutral_commands() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::CommandCatalogGet {
            command_id: "catalog-1".into(),
        })
        .await;

    let [
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::CommandCatalogListed { commands, .. }),
            ..
        },
    ] = events.as_slice()
    else {
        panic!("expected command catalog event, got {events:?}");
    };
    assert!(commands.iter().any(|command| command.id == "session.new"));
}

struct PromptDebugRunner;

#[async_trait]
impl AgentRunRunner for PromptDebugRunner {
    async fn prompt_debug_snapshot(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Option<piko_protocol::PromptDebugSnapshot> {
        (session_id == "s1" && agent_instance_id == "a1").then(|| {
            piko_protocol::PromptDebugSnapshot {
                session_id: session_id.into(),
                agent_instance_id: agent_instance_id.into(),
                run_prompt: piko_protocol::SemanticRunPrompt::default(),
                resource_messages: Vec::new(),
                tool_catalog: piko_protocol::ResolvedToolCatalog::new(Vec::new(), "tools"),
                model_inputs: Vec::new(),
            }
        })
    }
}

#[tokio::test]
async fn prompt_debug_get_returns_latest_runner_snapshot() {
    let server = HostServer::with_turn_runner(Arc::new(PromptDebugRunner));
    let events = server
        .handle_command(Command::PromptDebugGet {
            command_id: "debug-1".into(),
            session_id: "s1".into(),
            agent_instance_id: "a1".into(),
        })
        .await;

    let [
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::PromptDebugged { snapshot, .. }),
            ..
        },
    ] = events.as_slice()
    else {
        panic!("expected prompt debug snapshot, got {events:?}");
    };
    assert_eq!(snapshot.session_id, "s1");
    assert_eq!(snapshot.agent_instance_id, "a1");
    assert_eq!(snapshot.tool_catalog.digest, "tools");
}

#[tokio::test]
async fn prompt_debug_get_is_explicit_when_snapshot_is_unavailable() {
    let events = HostServer::new()
        .handle_command(Command::PromptDebugGet {
            command_id: "debug-missing".into(),
            session_id: "s1".into(),
            agent_instance_id: "a1".into(),
        })
        .await;

    let [
        Event::CommandResponse {
            result: Err(error), ..
        },
    ] = events.as_slice()
    else {
        panic!("expected unavailable error, got {events:?}");
    };
    assert!(error.contains("prompt debug snapshot unavailable"));
}

struct AssistantRunner;

#[async_trait]
impl AgentRunRunner for AssistantRunner {
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
        let assistant_message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "world".into(),
            }],
            continuation: None,
            provider: "test-provider".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(3),
        };
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    message: assistant_message,
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
                2,
                execution_succeeded(),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            4,
            Duration::ZERO,
        ))
    }
}

#[derive(Default)]
struct ReuseRootAgentRunRunner {
    turn_count: std::sync::atomic::AtomicU32,
    root_agent_instance_id: std::sync::Mutex<Option<String>>,
}

#[async_trait]
impl AgentRunRunner for ReuseRootAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let store = SessionStore::new(&input.session_dir);

        let turn = self
            .turn_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = if turn == 0 {
            let id = input.operation_id.clone();
            *self.root_agent_instance_id.lock().unwrap() = Some(id.clone());
            id
        } else {
            self.root_agent_instance_id
                .lock()
                .unwrap()
                .clone()
                .expect("root agent instance id")
        };
        let turn_id = input.operation_id.clone();
        let prompt = input.prompt.clone();

        let user_message_id: String = if turn == 0 {
            "user-1".into()
        } else {
            "user-2".into()
        };
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: user_message_id.clone(),
                    parent_message_id: if turn == 0 {
                        None
                    } else {
                        Some("assistant-1".into())
                    },
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "agent-1",
            )
            .unwrap();

        let assistant_message_id: String = if turn == 0 {
            "assistant-1".into()
        } else {
            "assistant-2".into()
        };
        let assistant_message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: if turn == 0 {
                    "world".into()
                } else {
                    "again".into()
                },
            }],
            continuation: None,
            provider: "test-provider".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(3),
        };
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: assistant_message_id.clone(),
                    parent_message_id: Some(user_message_id.clone()),
                    message: assistant_message,
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let user_task_seq: u64 = if turn == 0 { 1 } else { 3 };
        let assistant_task_seq: u64 = if turn == 0 { 2 } else { 4 };
        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            if turn == 0 {
                publisher_task.publish(
                    agent_instance_id.clone(),
                    "agent-1",
                    1,
                    execution_running(),
                );
            }

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                user_task_seq,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: user_message_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                assistant_task_seq,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: assistant_message_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                assistant_task_seq + 1,
                execution_succeeded(),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            if turn == 0 { 4 } else { 3 },
            Duration::ZERO,
        ))
    }
}

struct WaitingApprovalRunner {
    started: Arc<Notify>,
    finish: Arc<Notify>,
}

#[async_trait]
impl AgentRunRunner for WaitingApprovalRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let started = self.started.clone();
        let finish = self.finish.clone();
        let agent_instance_id = input.operation_id.clone();
        let publisher_task = Arc::clone(&publisher);
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: subscription.cursor.epoch.clone(),
            seq: 2,
        };
        let (completion_tx, completion) = support::test_oneshot();
        let completion_session_id = input.session_id.clone();
        let completion_turn_id = input.operation_id.clone();
        let completion_agent_instance_id = input.agent_instance_id.clone();

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 0, execution_running());
            started.notify_one();
            finish.notified().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 1, execution_succeeded());
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                address: piko_hostd::ports::AgentOperationAddress {
                    session_id: completion_session_id,
                    operation_id: completion_turn_id,
                    agent_instance_id: completion_agent_instance_id.clone(),
                },
                result: Ok(success_report(completion_agent_instance_id)),
                observation_barrier: barrier,
            });
        });

        let (started_tx, started_rx) = support::test_oneshot();
        let _ = started_tx.send(subscription);
        Ok(AgentRunHandle {
            address: piko_hostd::ports::AgentOperationAddress {
                session_id: input.session_id.clone(),
                operation_id: input.operation_id.clone(),
                agent_instance_id: input.agent_instance_id.clone(),
            },
            receipt: piko_protocol::AgentInputReceipt {
                request_id: input.operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition: piko_protocol::InputDisposition::Accepted,
            },
            process: test_agent_run_process(started_rx, completion),
        })
    }

    async fn respond_approval(
        &self,
        _approval_id: &str,
        _decision: ApprovalDecision,
    ) -> Result<bool, piko_hostd::api::ProtocolError> {
        Ok(true)
    }
}

mod session_tests;
mod turn_tests;
