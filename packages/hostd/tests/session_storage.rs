use std::sync::Arc;

use async_trait::async_trait;
use hostd::api::{Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;
use hostd::turn_runner::{TurnRunInput, TurnRunner};
use orchd::runtime::dispatch::{LifecycleEvent, PersistEvent, SessionChannels};

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
    async fn run_turn_channels(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionChannels, hostd::api::ProtocolError> {
        let channels = SessionChannels::new(Default::default());
        let senders = channels.senders();
        tokio::spawn(async move {
            let root = piko_protocol::TaskEvent::Created {
                session_id: input.session_id.clone(),
                task_id: "task-main".into(),
                agent_id: "main".into(),
                parent_task_id: None,
                source_agent_id: None,
                prompt: input.prompt.clone(),
                turn_id: input.turn_id.clone(),
                timestamp: 1,
            };
            let child = piko_protocol::TaskEvent::Created {
                session_id: input.session_id.clone(),
                task_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                parent_task_id: Some("task-main".into()),
                source_agent_id: Some("main".into()),
                prompt: "say hello".into(),
                turn_id: input.turn_id,
                timestamp: 2,
            };
            let _ = senders
                .lifecycle
                .send(Arc::new(LifecycleEvent::Task(root.clone())))
                .await;
            let _ = senders
                .persist
                .send(Arc::new(PersistEvent::TaskLifecycle(root)))
                .await;
            let _ = senders
                .lifecycle
                .send(Arc::new(LifecycleEvent::Task(child.clone())))
                .await;
            let _ = senders
                .persist
                .send(Arc::new(PersistEvent::TaskLifecycle(child)))
                .await;

            let message = Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text {
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
            let _ = senders
                .persist
                .send(Arc::new(PersistEvent::Finalized {
                    session_id: input.session_id,
                    message_id: "assistant-child".into(),
                    task_id: "task-child".into(),
                    agent_id: "hello-agent".into(),
                    message,
                }))
                .await;
        });
        Ok(channels)
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
async fn persistent_turn_writes_agent_messages_to_agent_jsonl() {
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
    let main_jsonl = std::fs::read_to_string(session_dir.join("main.jsonl")).unwrap();
    let child_jsonl = std::fs::read_to_string(session_dir.join("hello-agent.jsonl")).unwrap();
    let tasks_json = std::fs::read_to_string(session_dir.join("tasks.json")).unwrap();

    assert!(main_jsonl.contains("spawn child"));
    assert!(!main_jsonl.contains("hello from child"));
    assert!(child_jsonl.contains("hello from child"));
    assert!(child_jsonl.contains("\"agentId\":\"hello-agent\""));
    assert!(child_jsonl.contains("\"taskId\":\"task-child\""));
    assert!(tasks_json.contains("\"task-main\""));
    assert!(tasks_json.contains("\"task-child\""));
    assert!(tasks_json.contains("\"parent_task_id\": \"task-main\""));
    assert!(tasks_json.contains("\"source_agent_id\": \"main\""));

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
