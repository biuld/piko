use std::collections::BTreeMap;

use piko_protocol::AgentInstanceIdentity;
use piko_session_store::{
    NewSession, OpenOptions, ProposedCommit, RawEvent, SessionStore, StoreError,
};
use tempfile::tempdir;

fn new_session() -> NewSession {
    NewSession {
        session_id: "float-checksum".into(),
        cwd: "/project".into(),
        created_at: 1,
        root: AgentInstanceIdentity {
            session_id: "float-checksum".into(),
            agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        },
    }
}

#[test]
fn checksum_verifies_original_float_spelling_without_json_round_trip() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session()).unwrap();
    let event = RawEvent::optional(
        "float-event",
        "float_annotation",
        serde_json::json!({"amount": 0.000045600000000000004_f64}),
    );
    opened
        .store
        .append(
            1,
            ProposedCommit {
                commit_id: "float-commit".into(),
                committed_at: 2,
                causation_id: None,
                correlation_id: None,
                events: vec![event],
                extensions: BTreeMap::new(),
            },
        )
        .unwrap();
    drop(opened);

    let segment = path.join("events/00000000000000000001-open.jsonl");
    let journal = std::fs::read_to_string(&segment).unwrap();
    assert!(journal.contains("0.000045600000000000004"));

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.aggregate.revision, 2);
    drop(reopened);

    let changed = journal.replacen("0.000045600000000000004", "0.0000456", 1);
    std::fs::write(&segment, changed).unwrap();
    let error = SessionStore::open(&path, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(error, StoreError::Corruption { message, .. } if message == "checksum mismatch")
    );
}
