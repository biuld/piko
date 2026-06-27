use hostd::api::{HostCommand, HostEvent};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;
use std::fs;

fn session_id_from(events: &[HostEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            HostEvent::SessionCreated { session_id, .. } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session id event")
}

#[test]
fn repository_reads_pi_style_content_blocks() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let session_dir = temp.path().join("--tmp-project--");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("1_session-blocks.jsonl"),
        r#"{"type":"session","version":3,"id":"session-blocks","timestamp":"1","cwd":"/tmp/project"}
{"type":"message","id":"entry-1","parentId":null,"timestamp":"2","message":{"role":"user","content":[{"type":"text","text":"hello "},{"type":"text","text":"blocks"}]}}
"#,
    )
    .unwrap();

    let reopened = repo.open("/tmp/project", "session-blocks").unwrap();
    assert_eq!(reopened.state.messages[0].text, "hello blocks");
}

#[tokio::test]
async fn persistent_server_reopens_with_session() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage(repo);

    let created = server
        .handle_command(HostCommand::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let listed = server
        .handle_command(HostCommand::SessionList {
            command_id: "list".into(),
        })
        .await;
    assert!(matches!(
        &listed[0],
        HostEvent::SessionListed { sessions, .. } if sessions.iter().any(|session| session.session_id == session_id)
    ));

    let renamed = server
        .handle_command(HostCommand::SessionRename {
            command_id: "rename".into(),
            session_id: session_id.clone(),
            name: "Renamed".into(),
        })
        .await;
    assert!(matches!(
        &renamed[0],
        HostEvent::SessionOpened { snapshot, .. } if snapshot.name.as_deref() == Some("Renamed")
    ));

    let snapshot = server
        .handle_command(HostCommand::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    assert!(matches!(
        &snapshot[0],
        HostEvent::StateSnapshot { snapshot, .. } if snapshot.session_id == session_id
    ));
}
