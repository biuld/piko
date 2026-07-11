use std::sync::Arc;

use async_trait::async_trait;
use hostd::api::{Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;
use hostd::turn_runner::{TurnRunInput, TurnRunner};
use orchd::SessionSubscription;
use orchd::host::{SessionOutputHub, merged_output_stream};
use piko_protocol::agent_runtime::{SessionEvent, SessionEventEnvelope, TaskSnapshot, TaskStatus};
use piko_protocol::{ContentBlock, MessageContent, MessageRole};

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

struct AgentPersistRunner;

#[async_trait]
impl TurnRunner for AgentPersistRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        use hostd::infra::storage::task_repository::SESSION_SCHEMA_VERSION;
        use hostd::infra::storage::{TaskRepository, TaskShardHeader};

        let Some(session_dir) = input.session_dir.clone() else {
            return Err(hostd::api::ProtocolError::InvalidCommand(
                "session_dir required for AgentPersistRunner".into(),
            ));
        };

        let hub = Arc::new(SessionOutputHub::new(
            input.session_id.clone(),
            format!("test_{}", uuid::Uuid::new_v4()),
            64,
        ));
        let cursor = hub.cursor();
        let subscription = merged_output_stream(hub.subscribe(), cursor.clone());
        let repository = TaskRepository::new(session_dir);
        let session_id = input.session_id.clone();
        let turn_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let hub_task = Arc::clone(&hub);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let publish =
                async |task_id: String, agent_id: String, task_seq: u64, event: SessionEvent| {
                    let _ = hub_task
                        .publish_event(SessionEventEnvelope {
                            task_id,
                            agent_id,
                            task_seq,
                            cursor: hub_task.cursor(),
                            event,
                        })
                        .await;
                };

            let created_at = 1;
            for (task_id, agent_id, parent_task_id) in [
                ("task-main", "main", None),
                ("task-child", "hello-agent", Some("task-main")),
            ] {
                let _ = repository.create_task(TaskShardHeader {
                    schema_version: SESSION_SCHEMA_VERSION,
                    session_id: session_id.clone(),
                    task_id: task_id.into(),
                    agent_id: agent_id.into(),
                    parent_task_id: parent_task_id.map(str::to_string),
                    created_at,
                });
                publish(
                    task_id.into(),
                    agent_id.into(),
                    0,
                    SessionEvent::TaskChanged {
                        snapshot: TaskSnapshot {
                            session_id: session_id.clone(),
                            task_id: task_id.into(),
                            agent_id: agent_id.into(),
                            parent_task_id: parent_task_id.map(str::to_string),
                            status: TaskStatus::Created,
                            active_work: None,
                        },
                    },
                )
                .await;
            }

            let _ = repository.commit_message(orchd::integration::MessageCommit {
                session_id: session_id.clone(),
                task_id: "task-main".into(),
                agent_id: "main".into(),
                work_id: turn_id.clone(),
                task_seq: 1,
                message_id: "user-main".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String(prompt.clone()),
                    timestamp: Some(1),
                },
                committed_at: 1,
            });
            publish(
                "task-main".into(),
                "main".into(),
                1,
                SessionEvent::MessageCommitted {
                    message_id: "user-main".into(),
                    work_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            )
            .await;

            let _ = repository.commit_message(orchd::integration::MessageCommit {
                session_id: session_id.clone(),
                task_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                work_id: "child-work".into(),
                task_seq: 1,
                message_id: "user-child".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("say hello".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            });
            publish(
                "task-child".into(),
                "hello-agent".into(),
                1,
                SessionEvent::MessageCommitted {
                    message_id: "user-child".into(),
                    work_id: "child-work".into(),
                    role: MessageRole::User,
                },
            )
            .await;

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
            let _ = repository.commit_message(orchd::integration::MessageCommit {
                session_id: session_id.clone(),
                task_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                work_id: "child-work".into(),
                task_seq: 2,
                message_id: "assistant-child".into(),
                parent_message_id: Some("user-child".into()),
                message: message.clone(),
                committed_at: 2,
            });
            publish(
                "task-child".into(),
                "hello-agent".into(),
                2,
                SessionEvent::MessageCommitted {
                    message_id: "assistant-child".into(),
                    work_id: "child-work".into(),
                    role: MessageRole::Assistant,
                },
            )
            .await;

            publish(
                "task-main".into(),
                "main".into(),
                2,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: "task-main".into(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Idle,
                        active_work: None,
                    },
                },
            )
            .await;
        });
        std::mem::forget(hub);

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor,
            output: subscription,
        })
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
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::SessionOpened { snapshot, .. }), .. }
            if snapshot.name.as_deref() == Some("Renamed")
    ));

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert!(matches!(
        &snapshot[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }), .. }
            if snapshot.session_id == session_id
    ));
}

#[tokio::test]
async fn persistent_session_navigate_to_root_user_writes_leaf_target_none() {
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

    let _ = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }),
        ..
    } = &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    let root_user_id = snapshot.entries[0].id().to_string();

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
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::SessionOpened { snapshot, .. }),
        ..
    } = &navigated[1]
    else {
        panic!("expected session opened");
    };
    assert_eq!(snapshot.current_leaf_id, None);
    assert!(matches!(
        snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
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
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
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
    let main_jsonl = std::fs::read_to_string(session_dir.join("tasks/task-main.jsonl")).unwrap();
    let child_jsonl = std::fs::read_to_string(session_dir.join("tasks/task-child.jsonl")).unwrap();
    let manifest = std::fs::read_to_string(session_dir.join("session.json")).unwrap();

    assert!(main_jsonl.contains("spawn child"));
    assert!(!main_jsonl.contains("hello from child"));
    assert!(child_jsonl.contains("hello from child"));
    assert!(
        child_jsonl.contains("\"agentId\": \"hello-agent\"")
            || child_jsonl.contains("\"agentId\":\"hello-agent\"")
    );
    assert!(manifest.contains("task-main"));
    assert!(manifest.contains("task-child"));
    assert!(!session_dir.join("main.jsonl").exists());
    assert!(!session_dir.join("hello-agent.jsonl").exists());
    assert!(!session_dir.join("tasks.json").exists());
    assert!(child_jsonl.contains("\"taskId\":\"task-child\""));
    assert!(manifest.contains("\"parentTaskId\": \"task-main\""));

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
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::SessionOpened { snapshot, .. }), .. }
            if snapshot.tasks.contains_key("task-main") && snapshot.tasks.contains_key("task-child")
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
                && agents[0].task_id == "task-main"
                && agents[1].task_id == "task-child"
                && agents[1].parent_task_id.as_deref() == Some("task-main")
    ));

    let subscribed = reopened_server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id,
            task_id: "task-child".into(),
            after_seq: None,
        })
        .await;
    assert!(matches!(
        &subscribed[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::AgentSubscribed { task_id, agent_id, snapshot, .. }), .. }
            if task_id == "task-child"
                && agent_id == "hello-agent"
                && snapshot.task_id == "task-child"
                && snapshot.agent_id == "hello-agent"
                && !snapshot.events.is_empty()
    ));
}
