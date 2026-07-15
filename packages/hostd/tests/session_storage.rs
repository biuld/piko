#[path = "support/mock_turn_runner.rs"]
mod mock_turn_runner;
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use hostd::api::{Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use hostd::protocol::HostServer;
use mock_turn_runner::MockTurnRunner;
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, execution_running, execution_succeeded, successful_turn_run};

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session id event")
}

fn snapshot_from_refresh(events: &[Event]) -> &hostd::api::SessionSnapshot {
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
impl TurnRunner for AgentPersistRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let session_dir = input.session_dir.clone();

        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let store = SessionStore::new(session_dir);
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let prompt = input.prompt.clone();
        let publisher_task = Arc::clone(&publisher);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let publish = |agent_instance_id: String,
                           agent_id: String,
                           task_seq: u64,
                           event: SessionEvent| {
                publisher_task.publish(agent_instance_id, agent_id, task_seq, event);
            };

            let created_at = 1;
            for (agent_instance_id, agent_id, parent_agent_instance_id) in [
                ("task-main", "main", None),
                ("task-child", "hello-agent", Some("task-main")),
            ] {
                let is_root = parent_agent_instance_id.is_none();
                let _ =
                    store.ensure_agent_shard(&session_id, agent_instance_id, agent_id, created_at);
                let _ = store.update_manifest(|manifest| {
                    if is_root {
                        // Replace the default root AgentInstance created by
                        // `create_session` with this test's own root agent
                        // instance id so the manifest only ever tracks the
                        // two agent instances under test.
                        manifest.agents.clear();
                        manifest.root_agent_instance_id = Some(agent_instance_id.to_string());
                    }
                    manifest.agents.insert(
                        agent_instance_id.to_string(),
                        hostd::infra::storage::AgentManifestEntry {
                            identity: piko_protocol::AgentInstanceIdentity {
                                session_id: session_id.clone(),
                                agent_instance_id: agent_instance_id.to_string(),
                                agent_spec_id: agent_id.to_string(),
                                parent_agent_instance_id: parent_agent_instance_id
                                    .map(str::to_string),
                            },
                            spec: None,
                            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                            latest_report: None,
                            created_at,
                            updated_at: created_at,
                        },
                    );
                });
                if is_root {
                    publish(
                        agent_instance_id.into(),
                        agent_id.into(),
                        0,
                        execution_running(
                            session_id.clone(),
                            turn_id.clone(),
                            agent_instance_id,
                            agent_id,
                        ),
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
                    message_id: "user-child".into(),
                    source_turn_id: "child-work".into(),
                    role: MessageRole::User,
                },
            );

            let message = Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: "hello from child".into(),
                }],
                api: "test".into(),
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
                    message_id: "assistant-child".into(),
                    source_turn_id: "child-work".into(),
                    role: MessageRole::Assistant,
                },
            );

            publish(
                "task-main".into(),
                "main".into(),
                2,
                execution_succeeded(session_id.clone(), turn_id.clone(), "task-main", "main"),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "task-main",
            5,
            std::time::Duration::ZERO,
        ))
    }
}

#[tokio::test]
async fn persistent_server_reopens_with_session() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage(repo);

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let listed = server
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    assert!(matches!(
        &listed[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::SessionListed { sessions, .. }), .. }
            if sessions.iter().any(|session| session.session_id == session_id)
    ));

    let renamed = server
        .handle_command(Command::SessionRename {
            command_id: "rename".into(),
            session_id: session_id.clone(),
            name: "Renamed".into(),
        })
        .await;
    assert!(matches!(
        &renamed[0],
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::Empty),
            ..
        }
    ));
    assert!(matches!(
        &renamed[1],
        Event::SessionReconciled(reconciled)
            if reconciled.snapshot.name.as_deref() == Some("Renamed")
    ));

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert_eq!(snapshot_from_refresh(&snapshot).session_id, session_id);
}

#[tokio::test]
async fn persistent_session_navigate_to_root_user_writes_leaf_target_none() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockTurnRunner));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let _ = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "hello".into(),
        })
        .await;

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let root_user_id = snapshot_from_refresh(&snapshot).entries[0].id().to_string();

    let navigated = server
        .handle_command(Command::SessionNavigate {
            command_id: "navigate".into(),
            session_id: session_id.clone(),
            entry_id: root_user_id.clone(),
            summarize: false,
            custom_instructions: None,
        })
        .await;

    assert!(matches!(
        &navigated[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::SessionNavigated {
            new_leaf_id: None,
            selected_entry_id,
            editor_text: Some(text),
            ..
        }), .. } if selected_entry_id == &root_user_id && text == "hello"
    ));
    let Event::SessionReconciled(reconciled) = &navigated[1] else {
        panic!("expected session reconciled");
    };
    assert_eq!(reconciled.snapshot.current_leaf_id, None);
    assert!(matches!(
        reconciled.snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
    ));
}

#[tokio::test]
async fn deleting_visible_session_returns_empty_then_authoritative_clear() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockTurnRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create-delete".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let deleted = server
        .handle_command(Command::SessionDelete {
            command_id: "delete".into(),
            session_id: session_id.clone(),
        })
        .await;

    assert!(matches!(
        deleted.as_slice(),
        [
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::Empty),
                ..
            },
            Event::SessionCleared(piko_protocol::SessionClearedEvent {
                previous_session_id
            })
        ] if previous_session_id == &session_id
    ));
    let listed = server
        .handle_command(Command::SessionList {
            command_id: "list-after-delete".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    assert!(matches!(
        &listed[0],
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionListed { sessions, .. }),
            ..
        } if sessions.iter().all(|session| session.session_id != session_id)
    ));
}

#[tokio::test]
async fn persistent_turn_writes_each_task_to_its_own_shard() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(AgentPersistRunner));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let _ = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "spawn child".into(),
        })
        .await;

    let listed = server
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::SessionListed { sessions, .. }),
        ..
    } = &listed[0]
    else {
        panic!("expected session list");
    };
    let session_path = sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .and_then(|session| session.session_path.as_ref())
        .expect("session path should be listed");
    let session_dir = std::path::PathBuf::from(session_path);
    let main_jsonl = std::fs::read_to_string(session_dir.join("agents/task-main.jsonl")).unwrap();
    let child_jsonl = std::fs::read_to_string(session_dir.join("agents/task-child.jsonl")).unwrap();
    let manifest = std::fs::read_to_string(session_dir.join("session.json")).unwrap();

    assert!(main_jsonl.contains("spawn child"));
    assert!(!main_jsonl.contains("hello from child"));
    assert!(child_jsonl.contains("hello from child"));
    assert!(
        child_jsonl.contains("\"agentSpecId\": \"hello-agent\"")
            || child_jsonl.contains("\"agentSpecId\":\"hello-agent\"")
    );
    assert!(manifest.contains("task-main"));
    assert!(manifest.contains("task-child"));
    assert!(!session_dir.join("main.jsonl").exists());
    assert!(!session_dir.join("hello-agent.jsonl").exists());
    assert!(!session_dir.join("tasks.json").exists());
    assert!(!session_dir.join("tasks").exists());
    assert!(child_jsonl.contains("\"agentInstanceId\":\"task-child\""));
    assert!(
        manifest.contains("\"parentAgentInstanceId\": \"task-main\"")
            || manifest.contains("\"parentAgentInstanceId\":\"task-main\"")
    );

    let reopened_server = HostServer::with_storage(JsonlSessionRepository::new(temp.path()));
    let opened = reopened_server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path.clone()),
        })
        .await;
    assert!(matches!(
        &opened[0],
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionOpened { .. }),
            ..
        }
    ));
    assert!(matches!(
        &opened[1],
        Event::SessionReconciled(reconciled)
            if reconciled.reason == piko_protocol::ReconcileReason::InitialHydration
                && reconciled.agents.len() == 2
                && reconciled.agents[0].agent_instance_id == "task-main"
                && reconciled.agents[1].agent_instance_id == "task-child"
    ));

    let listed_agents = reopened_server
        .handle_command(Command::AgentList {
            command_id: "agents".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert!(matches!(
        &listed_agents[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::AgentListed { agents, .. }), .. }
            if agents.len() == 2
                && agents[0].agent_instance_id == "task-main"
                && agents[1].agent_instance_id == "task-child"
                && agents[1].parent_agent_instance_id.as_deref() == Some("task-main")
    ));

    let subscribed = reopened_server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id,
            agent_instance_id: "task-child".into(),
            after_seq: None,
        })
        .await;
    assert!(matches!(
        &subscribed[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::AgentSubscribed { agent_instance_id, agent_id, snapshot, .. }), .. }
            if agent_instance_id == "task-child"
                && agent_id == "hello-agent"
                && snapshot.agent_instance_id == "task-child"
                && snapshot.agent_id == "hello-agent"
                && !snapshot.events.is_empty()
    ));
}
