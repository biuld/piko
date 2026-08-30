#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::api::{Command, Message, ServerMessage as Event, SessionTreeEntry};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::HostServer;
use piko_orchd_api::AgentCommitPort;
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{AgentDurableCommand, ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, execution_running, execution_succeeded};

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session id event")
}

fn snapshot_from_refresh(events: &[Event]) -> &piko_hostd::api::SessionSnapshot {
    events
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("session reconciled snapshot")
}

#[derive(Clone, Default)]
struct AgentPersistRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

#[async_trait]
impl AgentRunRunner for AgentPersistRunner {
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
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let (completion_tx, completion_rx) = support::test_oneshot();
        let publisher_task = Arc::clone(&publisher);
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let prompt = support::content_text(&input.content);
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher,
        );
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let publish = |agent_instance_id: String,
                           agent_id: String,
                           task_seq: u64,
                           event: SessionEvent| {
                publisher_task.publish(agent_instance_id, agent_id, task_seq, event);
            };

            for (agent_instance_id, agent_id, parent_agent_instance_id) in [
                (
                    "task-main",
                    "main",
                    Some(format!("agent_{session_id}_root")),
                ),
                ("task-child", "hello-agent", Some("task-main".into())),
            ] {
                let is_main = agent_instance_id == "task-main";
                let _ = store
                    .commit_agent_command(
                        &session_id,
                        AgentDurableCommand::Create {
                            identity: piko_protocol::AgentInstanceIdentity {
                                session_id: session_id.clone(),
                                agent_instance_id: agent_instance_id.to_string(),
                                agent_spec_id: agent_id.to_string(),
                                parent_agent_instance_id,
                            },
                            spec: test_agent_spec(agent_id),
                        },
                    )
                    .await;
                if is_main {
                    publish(
                        agent_instance_id.into(),
                        agent_id.into(),
                        0,
                        execution_running(),
                    );
                }
            }

            let _ = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    root_input_id: "input-1".into(),
                    agent_instance_id: "task-main".into(),
                    message_id: "user-main".into(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt.clone()),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "main",
            );
            publish(
                "task-main".into(),
                "main".into(),
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: "user-main".into(),
                    root_input_id: input_id.clone(),
                    role: MessageRole::User,
                },
            );

            let _ = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    root_input_id: "input-1".into(),
                    agent_instance_id: "task-child".into(),
                    message_id: "user-child".into(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String("say hello".into()),
                        timestamp: Some(2),
                    },
                    committed_at: 2,
                },
                "hello-agent",
            );
            publish(
                "task-child".into(),
                "hello-agent".into(),
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: "user-child".into(),
                    root_input_id: "child-work".into(),
                    role: MessageRole::User,
                },
            );

            let message = Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: "hello from child".into(),
                }],
                checkpoint: Some(Box::new(
                    serde_json::from_value(serde_json::json!("opaque-session-checkpoint")).unwrap(),
                )),
                provider: "test".into(),
                model: "test".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: Some(2),
            };
            let _ = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    root_input_id: "input-1".into(),
                    agent_instance_id: "task-child".into(),
                    message_id: "assistant-child".into(),
                    parent_message_id: Some("user-child".into()),
                    tree_parent_entry_id: None,
                    message: message.clone(),
                    committed_at: 2,
                },
                "hello-agent",
            );
            publish(
                "task-child".into(),
                "hello-agent".into(),
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 3,
                    message_id: "assistant-child".into(),
                    root_input_id: "child-work".into(),
                    role: MessageRole::Assistant,
                },
            );

            publish("task-main".into(), "main".into(), 2, execution_succeeded());
            let barrier = publisher_task.cursor();
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                input_id,
                result: Ok(support::success_report("task-main")),
                observation_barrier: barrier,
            });
        });
        Ok(piko_protocol::AgentInputReceipt {
            input_id: input.input_id,
            request_id: input.request_id,
            session_id: input.session_id,
            agent_instance_id: input.agent_instance_id,
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        })
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

fn test_agent_spec(id: &str) -> piko_protocol::AgentSpec {
    piko_protocol::AgentSpec {
        id: id.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", id),
        name: id.into(),
        role: "test".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "test".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    }
}

mod persistence_tests;
