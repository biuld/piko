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
async fn first_reconciled_snapshot_contains_atomic_interruption_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let created = repo.create("/tmp/project").unwrap();
    let session_id = created.state.session_id.clone();
    let session_path = created.path.to_string_lossy().to_string();
    let store = SessionStore::new(&created.path);
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "request-interrupted".into(),
                request_id: "request-interrupted".into(),
                source_turn_id: Some("turn-interrupted".into()),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "digest".into(),
                started_at: 1,
                input: piko_protocol::AgentInput {
                    input_id: "request-interrupted".into(),
                    request_id: "request-interrupted".into(),
                    session_id: session_id.clone(),
                    agent_instance_id: root.agent_instance_id.clone(),
                    origin: piko_protocol::AgentInputOrigin::User,
                    delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
                    content: piko_protocol::MessageContent::String("hello".into()),
                    submitted_at: 1,
                    caller_agent_instance_id: None,
                    detached_recipient_agent_instance_id: None,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_message(
            piko_protocol::execution::MessageCommit {
                session_id: session_id.clone(),
                source_turn_id: Some("turn-interrupted".into()),
                root_input_id: "request-interrupted".into(),
                agent_instance_id: root.agent_instance_id,
                message_id: "input-interrupted".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(1),
                },
                committed_at: 1,
            },
            "main",
        )
        .unwrap();

    let server = HostServer::with_storage(repo);
    let opened = server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path),
        })
        .await;
    let reconciled = opened
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(event) => Some(event),
            _ => None,
        })
        .expect("first reconciled snapshot");
    let marker_id = piko_protocol::turn_abort_marker_message_id("request-interrupted");
    assert!(
        reconciled
            .snapshot
            .agent_work
            .iter()
            .all(|work| work.active_work.is_none())
    );
    assert!(reconciled.snapshot.entries.iter().any(|entry| {
        matches!(entry, SessionTreeEntry::Message(message) if message.id == marker_id)
    }));
    let projection = store.load_projection().unwrap();
    let execution = projection
        .agent_executions
        .get("request-interrupted")
        .unwrap();
    assert_eq!(execution.status, piko_protocol::ExecutionStatus::Cancelled);
    assert!(execution.report.is_some());
}

#[tokio::test]
async fn persistent_session_navigate_to_root_user_clears_cursor_without_leaf_node() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner::default()));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let _ = server
        .handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("hello".into()),
        ))
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
}

#[tokio::test]
async fn deleting_visible_session_returns_empty_then_authoritative_clear() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner::default()));
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
async fn persistent_turn_recovers_each_agent_private_transcript() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(AgentPersistRunner::default()));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let _ = server
        .handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("spawn child".into()),
        ))
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
    let store = piko_hostd::infra::storage::SessionStore::new(&session_dir);
    let main = store.load_agent(&session_id, "task-main").unwrap();
    let child = store.load_agent(&session_id, "task-child").unwrap();
    let projection = store.load_projection().unwrap();
    let main_json = serde_json::to_string(&main.transcript).unwrap();
    let child_json = serde_json::to_string(&child.transcript).unwrap();

    assert!(main_json.contains("spawn child"));
    assert!(!main_json.contains("hello from child"));
    assert!(child_json.contains("hello from child"));
    assert_eq!(child.agent_spec_id, "hello-agent");
    assert_eq!(
        projection.agents["task-child"]
            .identity
            .parent_agent_instance_id
            .as_deref(),
        Some("task-main")
    );
    assert!(session_dir.join("events").is_dir());
    assert!(session_dir.join("readmodels").is_dir());
    assert!(!session_dir.join("agents").exists());

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
                && reconciled.agents.iter().any(|agent| agent.agent_instance_id == "task-main")
                && reconciled.agents.iter().any(|agent| agent.agent_instance_id == "task-child")
    ));
    let Event::SessionReconciled(reopened) = &opened[1] else {
        unreachable!()
    };
    let restored_checkpoint = reopened
        .snapshot
        .entries
        .iter()
        .find_map(|entry| match entry {
            SessionTreeEntry::Message(message) => match &message.message {
                Message::Assistant {
                    checkpoint: Some(checkpoint),
                    ..
                } => Some(checkpoint.as_ref()),
                _ => None,
            },
            _ => None,
        });
    assert_eq!(
        serde_json::to_value(restored_checkpoint.expect("assistant checkpoint restored")).unwrap(),
        serde_json::json!("opaque-session-checkpoint")
    );

    let listed_agents = reopened_server
        .handle_command(Command::AgentList {
            command_id: "agents".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert!(matches!(
        &listed_agents[0],
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::AgentListed { agents, .. }), .. }
            if agents.iter().any(|agent| agent.agent_instance_id == "task-main")
                && agents.iter().any(|agent| agent.agent_instance_id == "task-child"
                    && agent.parent_agent_instance_id.as_deref() == Some("task-main"))
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
