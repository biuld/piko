use std::fs::OpenOptions;
use std::io::Write;

use hostd::infra::storage::SessionStore;
use orchd_api::AgentCommitPort;
use piko_protocol::execution::{CommitError, MessageCommit};
use piko_protocol::{
    AgentDurableCommand, AgentExecutionReport, AgentInstanceIdentity, AgentInstanceLifecycle,
    Message, MessageContent,
};
use tempfile::tempdir;

fn test_agent_spec(id: &str) -> piko_protocol::AgentSpec {
    piko_protocol::AgentSpec {
        id: id.into(),
        name: id.into(),
        role: "test".into(),
        description: None,
        system_prompt: "test".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    }
}

fn message_commit(id: &str, parent: Option<&str>) -> MessageCommit {
    MessageCommit {
        session_id: "session-1".into(),
        source_turn_id: Some("turn-1".into()),
        execution_id: "exec-1".into(),
        agent_instance_id: "agent_session-1_root".into(),
        message_id: id.into(),
        parent_message_id: parent.map(str::to_string),
        message: Message::User {
            content: MessageContent::String("hello".into()),
            timestamp: Some(2),
        },
        committed_at: 2,
    }
}

#[tokio::test]
async fn agent_tree_lifecycle_and_inbox_survive_repository_reopen() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child = AgentInstanceIdentity {
        session_id: "session-1".into(),
        agent_instance_id: "agent-coder-1".into(),
        agent_spec_id: "coder".into(),
        parent_agent_instance_id: Some(root.agent_instance_id.clone()),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::Create {
                identity: child.clone(),
                spec: test_agent_spec("coder"),
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::SetLifecycle {
                agent_instance_id: child.agent_instance_id.clone(),
                lifecycle: AgentInstanceLifecycle::Closed,
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id: root.agent_instance_id.clone(),
                report: AgentExecutionReport {
                    agent_instance_id: child.agent_instance_id.clone(),
                    execution_id: "exec-child-1".into(),
                    outcome: piko_protocol::ExecutionOutcome::Succeeded {
                        usage: Default::default(),
                    },
                    summary: "done".into(),
                    usage: Default::default(),
                    artifacts: Vec::new(),
                },
            },
        )
        .await
        .unwrap();

    let reopened = SessionStore::new(temp.path());
    let manifest = reopened.load_manifest().unwrap();
    assert_eq!(
        manifest.root_agent_instance_id.as_deref(),
        Some(root.agent_instance_id.as_str())
    );
    let recovered_child = manifest.agents.get("agent-coder-1").unwrap();
    assert_eq!(recovered_child.identity, child);
    assert_eq!(recovered_child.lifecycle, AgentInstanceLifecycle::Closed);
    let inbox = reopened.agent_inbox(&root.agent_instance_id).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].report.execution_id, "exec-child-1");
}

#[test]
fn private_transcripts_are_recovered_per_agent_shard() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store.ensure_root_agent("main").unwrap();
    for child_id in ["coder-a", "coder-b"] {
        store
            .ensure_agent_shard("session-1", child_id, "coder", 1)
            .unwrap();
        store
            .commit_message(
                MessageCommit {
                    session_id: "session-1".into(),
                    source_turn_id: Some("turn-1".into()),
                    execution_id: format!("exec-{child_id}"),
                    agent_instance_id: child_id.into(),
                    message_id: format!("message-{child_id}"),
                    parent_message_id: None,
                    message: Message::User {
                        content: MessageContent::String(format!("private-{child_id}")),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "coder",
            )
            .unwrap();
    }

    let a = store.agent_transcript("session-1", "coder-a").unwrap();
    let b = store.agent_transcript("session-1", "coder-b").unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert!(matches!(
        &a[0],
        Message::User { content: MessageContent::String(text), .. }
            if text == "private-coder-a"
    ));
    assert!(matches!(
        &b[0],
        Message::User { content: MessageContent::String(text), .. }
            if text == "private-coder-b"
    ));
}

#[test]
fn ensure_root_agent_is_idempotent_after_manifest_clear() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .update_manifest(|manifest| {
            manifest.root_agent_instance_id = None;
            manifest.agents.clear();
            manifest.agent_revision = 0;
        })
        .unwrap();

    let first = store.ensure_root_agent("main").unwrap();
    let second = SessionStore::new(temp.path())
        .ensure_root_agent("main")
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(store.agent_instances().unwrap().len(), 1);
}

#[test]
fn stores_and_recovers_agent_shard_transcript() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("message-1", None), "main")
        .unwrap();
    store
        .commit_message(message_commit("message-2", Some("message-1")), "main")
        .unwrap();

    let recovered = store
        .load_agent("session-1", "agent_session-1_root")
        .unwrap();
    assert_eq!(recovered.transcript.len(), 2);
    assert_eq!(recovered.head_message_id.as_deref(), Some("message-2"));
    assert_eq!(recovered.last_transcript_seq, 2);
    assert!(
        temp.path()
            .join("agents/agent_session-1_root.jsonl")
            .exists()
    );
    assert!(!temp.path().join("tasks").exists());
    assert!(!temp.path().join("tasks.json").exists());
}

#[test]
fn rejects_wrong_parent_and_duplicate_payload_conflict() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("message-1", None), "main")
        .unwrap();
    let wrong_parent = message_commit("message-2", Some("other-message"));
    assert_eq!(
        store.commit_message(wrong_parent, "main"),
        Err(CommitError::IdentityMismatch)
    );

    let mut conflict = message_commit("message-1", None);
    conflict.execution_id = "different-exec".into();
    assert_eq!(
        store.commit_message(conflict, "main"),
        Err(CommitError::IdempotencyConflict)
    );
}

#[test]
fn find_committed_message_lenient_tolerates_trailing_partial_line() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("msg-followup", None), "main")
        .unwrap();

    let shard = temp.path().join("agents/agent_session-1_root.jsonl");
    let mut file = OpenOptions::new().append(true).open(&shard).unwrap();
    write!(file, "{{\"type\":\"message\",\"id\":\"partial").unwrap();

    assert!(
        store
            .load_agent("session-1", "agent_session-1_root")
            .is_err()
    );
    let found = store
        .find_committed_message_lenient("agent_session-1_root", "msg-followup")
        .expect("lenient scan should still find committed message");
    assert_eq!(found.id, "msg-followup");
}

#[tokio::test]
async fn fork_to_copies_agent_shards_with_rewritten_session_id() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("message-1", None), "main")
        .unwrap();

    let dest_dir = temp.path().join("forked");
    let forked = store
        .fork_to(dest_dir.clone(), "session-2".into(), 5)
        .unwrap();
    let manifest = forked.load_manifest().unwrap();
    assert_eq!(manifest.session_id, "session-2");
    let recovered = forked
        .load_agent("session-2", "agent_session-1_root")
        .unwrap();
    assert_eq!(recovered.transcript.len(), 1);
}

include!("session_store_cases/durable_agent.rs");
