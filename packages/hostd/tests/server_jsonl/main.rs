#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::api::{ApprovalDecision, Command, Message, ServerMessage as Event};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::{HostServer, run_jsonl_server};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{execution_running, execution_succeeded, success_report};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

#[derive(Clone, Default)]
struct SlowRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

#[async_trait]
impl AgentRunRunner for SlowRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let (receipt, control) = self.harness.alloc_root(
            &session_id,
            &agent_instance_id,
            &input_id,
            piko_protocol::AgentInputDisposition::AppliedAsRoot,
        );
        let publisher_task = Arc::clone(&control.publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 0, execution_running());
            tokio::time::sleep(Duration::from_millis(200)).await;
            let barrier = publisher_task.cursor();
            let _ = control
                .completion_tx
                .send(piko_hostd::ports::AgentRunCompletion {
                    input_id,
                    result: Ok(success_report(&agent_instance_id)),
                    observation_barrier: barrier,
                });
        });
        Ok(receipt)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
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

#[derive(Clone, Default)]
struct AssistantRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

#[async_trait]
impl AgentRunRunner for AssistantRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_dir = self.session_dir.lock().unwrap().clone().unwrap_or_default();
        let store = SessionStore::new(session_dir);
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let prompt = support::content_text(&input.content);
        let agent_spec_id = store
            .load_projection()
            .unwrap()
            .agents
            .get(&agent_instance_id)
            .expect("run agent must be durable")
            .identity
            .agent_spec_id
            .clone();

        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(input_id.clone()),
                    root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "user-1".into(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                &agent_spec_id,
            )
            .unwrap();
        let assistant_message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "world".into(),
            }],
            checkpoint: None,
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
                    source_turn_id: Some(input_id.clone()),
                    root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    tree_parent_entry_id: None,
                    message: assistant_message,
                    committed_at: 3,
                },
                &agent_spec_id,
            )
            .unwrap();

        let (receipt, control) = self.harness.alloc_root(
            &session_id,
            &agent_instance_id,
            &input_id,
            piko_protocol::AgentInputDisposition::AppliedAsRoot,
        );
        let publisher_task = Arc::clone(&control.publisher);
        let publish_input_id = input_id.clone();
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
                    source_turn_id: publish_input_id.clone(),
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
                    source_turn_id: publish_input_id,
                    role: MessageRole::Assistant,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                execution_succeeded(),
            );
            let barrier = publisher_task.cursor();
            let _ = control
                .completion_tx
                .send(piko_hostd::ports::AgentRunCompletion {
                    input_id,
                    result: Ok(success_report(&agent_instance_id)),
                    observation_barrier: barrier,
                });
        });
        Ok(receipt)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }
}

#[derive(Clone, Default)]
struct ReuseRootAgentRunRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    turn_count: Arc<std::sync::atomic::AtomicU32>,
    root_agent_instance_id: Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl AgentRunRunner for ReuseRootAgentRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_dir = self.session_dir.lock().unwrap().clone().unwrap_or_default();
        let store = SessionStore::new(session_dir);

        let turn = self
            .turn_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = if turn == 0 {
            let id = input.agent_instance_id.clone();
            *self.root_agent_instance_id.lock().unwrap() = Some(id.clone());
            id
        } else {
            let recovered = self
                .root_agent_instance_id
                .lock()
                .unwrap()
                .clone()
                .expect("root agent instance id");
            assert_eq!(recovered, input.agent_instance_id);
            recovered
        };
        let prompt = support::content_text(&input.content);
        let agent_spec_id = store
            .load_projection()
            .unwrap()
            .agents
            .get(&agent_instance_id)
            .expect("run agent must be durable")
            .identity
            .agent_spec_id
            .clone();

        let user_message_id: String = if turn == 0 {
            "user-1".into()
        } else {
            "user-2".into()
        };
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(input_id.clone()),
                    root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: user_message_id.clone(),
                    parent_message_id: if turn == 0 {
                        None
                    } else {
                        Some("assistant-1".into())
                    },
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                &agent_spec_id,
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
            checkpoint: None,
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
                    source_turn_id: Some(input_id.clone()),
                    root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: assistant_message_id.clone(),
                    parent_message_id: Some(user_message_id.clone()),
                    tree_parent_entry_id: None,
                    message: assistant_message,
                    committed_at: 3,
                },
                &agent_spec_id,
            )
            .unwrap();

        let user_task_seq: u64 = if turn == 0 { 1 } else { 3 };
        let assistant_task_seq: u64 = if turn == 0 { 2 } else { 4 };
        let (receipt, control) = self.harness.alloc_root(
            &session_id,
            &agent_instance_id,
            &input_id,
            piko_protocol::AgentInputDisposition::AppliedAsRoot,
        );
        let publisher_task = Arc::clone(&control.publisher);
        let final_input_id = input_id.clone();
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
                    source_turn_id: final_input_id.clone(),
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
                    source_turn_id: final_input_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                assistant_task_seq + 1,
                execution_succeeded(),
            );
            let barrier = publisher_task.cursor();
            let _ = control
                .completion_tx
                .send(piko_hostd::ports::AgentRunCompletion {
                    input_id: final_input_id,
                    result: Ok(success_report(&agent_instance_id)),
                    observation_barrier: barrier,
                });
        });
        Ok(receipt)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }
}

#[derive(Clone, Default)]
struct WaitingApprovalRunner {
    harness: crate::support::MockRunHarness,
    started: Arc<Notify>,
    finish: Arc<Notify>,
}

#[async_trait]
impl AgentRunRunner for WaitingApprovalRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        _session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let (receipt, control) = self.harness.alloc_root(
            &session_id,
            &agent_instance_id,
            &input_id,
            piko_protocol::AgentInputDisposition::AppliedAsRoot,
        );
        let started = self.started.clone();
        let finish = self.finish.clone();
        let publisher_task = Arc::clone(&control.publisher);
        let final_input_id = input_id.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 0, execution_running());
            started.notify_one();
            finish.notified().await;
            publisher_task.publish(agent_instance_id.clone(), "main", 1, execution_succeeded());
            let barrier = publisher_task.cursor();
            let _ = control
                .completion_tx
                .send(piko_hostd::ports::AgentRunCompletion {
                    input_id: final_input_id,
                    result: Ok(success_report(&agent_instance_id)),
                    observation_barrier: barrier,
                });
        });
        Ok(receipt)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
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
