use hostd::api::{Command, Event};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::SessionCreated { session_id, .. } => Some(session_id.clone()),
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
        })
        .await;
    assert!(matches!(
        &listed[0],
        Event::SessionListed { sessions, .. } if sessions.iter().any(|session| session.session_id == session_id)
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
        Event::SessionOpened { snapshot, .. } if snapshot.name.as_deref() == Some("Renamed")
    ));

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert!(matches!(
        &snapshot[0],
        Event::StateSnapshot { snapshot, .. } if snapshot.session_id == session_id
    ));
}
