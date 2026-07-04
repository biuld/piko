use hostd::api::{Command, ServerMessage as Event, SessionTreeEntry};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResult(hostd::api::CommandResult::SessionCreated {
                session_id, ..
            }) => Some(session_id.clone()),
            _ => None,
        })
        .expect("session id event")
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
        Event::CommandResult(hostd::api::CommandResult::SessionListed { sessions, .. })
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
        Event::CommandResult(hostd::api::CommandResult::SessionOpened { snapshot, .. })
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
        Event::CommandResult(hostd::api::CommandResult::StateSnapshot { snapshot, .. })
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
    let Event::CommandResult(hostd::api::CommandResult::StateSnapshot { snapshot, .. }) =
        &snapshot[0]
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
        Event::CommandResult(hostd::api::CommandResult::SessionNavigated {
            new_leaf_id: None,
            selected_entry_id,
            editor_text: Some(text),
            ..
        }) if selected_entry_id == &root_user_id && text == "hello"
    ));
    let Event::CommandResult(hostd::api::CommandResult::SessionOpened { snapshot, .. }) =
        &navigated[1]
    else {
        panic!("expected session opened");
    };
    assert_eq!(snapshot.current_leaf_id, None);
    assert!(matches!(
        snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
    ));
}
