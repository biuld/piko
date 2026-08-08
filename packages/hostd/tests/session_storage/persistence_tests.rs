use super::*;

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
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::SessionListed { sessions, .. }), .. }
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
            result: Ok(piko_hostd::api::CommandResult::Empty),
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
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner));

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
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::SessionNavigated {
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
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner));
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
                result: Ok(piko_hostd::api::CommandResult::Empty),
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
            result: Ok(piko_hostd::api::CommandResult::SessionListed { sessions, .. }),
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
        result: Ok(piko_hostd::api::CommandResult::SessionListed { sessions, .. }),
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
            result: Ok(piko_hostd::api::CommandResult::SessionOpened { .. }),
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
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::AgentListed { agents, .. }), .. }
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
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::AgentSubscribed { agent_instance_id, agent_id, snapshot, .. }), .. }
            if agent_instance_id == "task-child"
                && agent_id == "hello-agent"
                && snapshot.agent_instance_id == "task-child"
                && snapshot.agent_id == "hello-agent"
                && !snapshot.events.is_empty()
    ));
}
