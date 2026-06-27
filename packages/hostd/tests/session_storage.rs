use hostd::api::{Command, Event};
use hostd::server::HostServer;
use hostd::session::JsonlSessionRepository;
use std::fs;

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::SessionCreated { session_id, .. } => Some(session_id.clone()),
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
    let Some(hostd::api::SessionTreeEntry::Message(entry)) = reopened.state.entries.first() else {
        panic!("expected message entry");
    };
    let hostd::api::Message::User { content, .. } = &entry.message else {
        panic!("expected user message");
    };
    let hostd::api::MessageContent::Blocks(blocks) = content else {
        panic!("expected content blocks");
    };
    assert_eq!(blocks.len(), 2);
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
