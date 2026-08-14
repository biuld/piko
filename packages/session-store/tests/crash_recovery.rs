use std::collections::BTreeMap;
use std::fs::{self, OpenOptions as FsOpenOptions};
use std::sync::{Arc, Barrier};

use piko_protocol::AgentInstanceIdentity;
use piko_session_store::{
    EventData, NewSession, OpenOptions, ProposedCommit, RawEvent, SessionStore, StoreError,
};
use tempfile::tempdir;

fn new_session(session_id: &str) -> NewSession {
    NewSession {
        session_id: session_id.into(),
        cwd: "/project".into(),
        created_at: 1,
        root: AgentInstanceIdentity {
            session_id: session_id.into(),
            agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        },
    }
}

#[test]
fn synced_boundary_commit_is_normalized_after_crash_before_rollover() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("boundary")).unwrap();
    for revision in 2..=1_000 {
        let event = RawEvent::new(
            format!("event-{revision}"),
            EventData::SessionMetadataChanged {
                name: Some(format!("revision-{revision}")),
            },
        )
        .unwrap();
        opened
            .store
            .append(
                revision - 1,
                ProposedCommit {
                    commit_id: format!("commit-{revision}"),
                    committed_at: revision as i64,
                    causation_id: None,
                    correlation_id: None,
                    events: vec![event],
                    extensions: BTreeMap::new(),
                },
            )
            .unwrap();
    }
    drop(opened);

    let events = path.join("events");
    let closed = events.join("00000000000000000001-00000000000000001000.jsonl");
    let interrupted_open = events.join("00000000000000000001-open.jsonl");
    let next_open = events.join("00000000000000001001-open.jsonl");
    fs::rename(&closed, &interrupted_open).unwrap();
    fs::remove_file(&next_open).unwrap();

    let reopened = SessionStore::open(&path, OpenOptions::default()).unwrap();
    assert_eq!(reopened.aggregate.revision, 1_000);
    assert!(reopened.recovery.repaired);
    assert!(closed.exists());
    assert!(next_open.exists());
    assert!(!interrupted_open.exists());
}

#[test]
fn published_session_requires_a_genesis_commit() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session");
    let opened = SessionStore::create(&path, new_session("partial")).unwrap();
    drop(opened);
    FsOpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path.join("events/00000000000000000001-open.jsonl"))
        .unwrap();

    let error = SessionStore::open(&path, OpenOptions::default()).unwrap_err();
    assert!(matches!(error, StoreError::InvalidEvent(message) if message.contains("genesis")));
}

#[test]
fn parallel_session_creation_safely_shares_the_staging_container() {
    let temp = tempdir().unwrap();
    let parent = Arc::new(temp.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(8));
    let threads = (0..8)
        .map(|index| {
            let parent = Arc::clone(&parent);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                let session_id = format!("parallel-{index}");
                let opened =
                    SessionStore::create(&parent.join(&session_id), new_session(&session_id))
                        .unwrap();
                assert_eq!(opened.aggregate.revision, 1);
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        thread.join().unwrap();
    }
}
