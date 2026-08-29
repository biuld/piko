#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::api::{Command, Message, ServerMessage as Event, SessionTreeEntry};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use piko_orchd_api::AgentCommitPort;
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{AgentDurableCommand, ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, execution_running, execution_succeeded, successful_turn_run};

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

struct AgentPersistRunner;

#[async_trait]
impl AgentRunRunner for AgentPersistRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let session_dir = input.session_dir.clone();

        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let store = SessionStore::new(session_dir);
        let session_id = input.session_id.clone();
        let turn_id = input.operation_id.clone();
        let prompt = input.text_projection();
        let publisher_task = Arc::clone(&publisher);

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
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: "task-main".into(),
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
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );

            let _ = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some("child-work".into()),
                    execution_id: "task-child".into(),
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
                    source_turn_id: "child-work".into(),
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
                    source_turn_id: Some("child-work".into()),
                    execution_id: "task-child".into(),
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
                    source_turn_id: "child-work".into(),
                    role: MessageRole::Assistant,
                },
            );

            publish("task-main".into(), "main".into(), 2, execution_succeeded());
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            5,
            std::time::Duration::ZERO,
        ))
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
