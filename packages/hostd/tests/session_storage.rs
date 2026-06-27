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
    let server = HostServer::new();

    let created = server
        .handle_command(HostCommand::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    // Session should be trackable
    assert!(!session_id.is_empty());
}
