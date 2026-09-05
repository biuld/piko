use piko_protocol::AgentInstanceIdentity;
use piko_session_store::{NewSession, SessionStore, query_inspection};

fn create(path: &std::path::Path) -> piko_session_store::OpenedSession {
    SessionStore::create(
        path,
        NewSession {
            session_id: "inspection".into(),
            cwd: "/project".into(),
            created_at: 1,
            root: AgentInstanceIdentity {
                session_id: "inspection".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
        },
    )
    .unwrap()
}

#[test]
fn aligned_inspection_does_not_access_journal_segments() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = create(&path);
    let expected = query_inspection(&path).unwrap();
    drop(opened);
    // The published snapshot remains readable even when segments are inaccessible.
    std::fs::rename(path.join("events"), path.join("hidden-events")).unwrap();
    assert_eq!(query_inspection(&path).unwrap(), expected);
}

#[test]
fn each_damaged_inspection_model_rebuilds_to_the_same_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session");
    let _opened = create(&path);
    let expected = query_inspection(&path).unwrap();
    for file in [
        "current.json",
        "history.json",
        "trajectory.json",
        "head.json",
    ] {
        std::fs::write(path.join("readmodels").join(file), b"broken").unwrap();
        assert_eq!(query_inspection(&path).unwrap(), expected, "{file}");
    }
}
