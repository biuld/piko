use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_orchd_api::AgentCommitPort;
use piko_protocol::execution::{CommitError, MessageCommit};
use piko_protocol::{
    AgentDurableCommand, AgentInstanceIdentity, AgentInstanceLifecycle, AgentWorkReport, Message,
    MessageContent,
};
use tempfile::tempdir;

fn test_agent_spec(id: &str) -> piko_protocol::AgentSpec {
    piko_protocol::AgentSpec {
        id: id.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", id),
        name: id.into(),
        role: "test".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "test".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    }
}

fn message_commit(id: &str, parent: Option<&str>) -> MessageCommit {
    MessageCommit {
        session_id: "session-1".into(),
        root_input_id: "input-1".into(),
        agent_instance_id: "agent_session-1_root".into(),
        message_id: id.into(),
        parent_message_id: parent.map(str::to_string),
        tree_parent_entry_id: None,
        message: Message::User {
            content: MessageContent::String("hello".into()),
            timestamp: Some(2),
        },
        committed_at: 2,
    }
}

#[test]
fn repository_create_returns_the_persisted_root_agent_selected() {
    let temp = tempdir().unwrap();
    let created = JsonlSessionRepository::new(temp.path())
        .create("/project")
        .unwrap();
    let root_agent_instance_id = format!("agent_{}_root", created.state.session_id);

    assert_eq!(
        created.state.active_agent_instance_id.as_deref(),
        Some(root_agent_instance_id.as_str())
    );
    assert!(
        created
            .state
            .active_agents
            .contains_key(&root_agent_instance_id)
    );
}

#[test]
fn import_validates_then_atomically_publishes_without_merging_existing_destination() {
    let source_root = tempdir().unwrap();
    let destination_root = tempdir().unwrap();
    let source = JsonlSessionRepository::new(source_root.path())
        .create("/project")
        .unwrap();
    let destination_repo = JsonlSessionRepository::new(destination_root.path());

    let imported = destination_repo.import(&source.path).unwrap();
    assert_eq!(imported.state.session_id, source.state.session_id);
    assert_eq!(imported.state.cwd, source.state.cwd);
    assert!(imported.path.join("session.json").is_file());

    let error = destination_repo.import(&source.path).unwrap_err();
    assert!(error.to_string().contains("destination already exists"));
    let reopened = destination_repo.load_by_path(&imported.path).unwrap();
    assert_eq!(reopened.state.session_id, source.state.session_id);
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
                report: AgentWorkReport {
                    agent_instance_id: child.agent_instance_id.clone(),
                    root_input_id: "input-child-1".into(),
                    report_id: "report-child-1".into(),
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
    let projection = reopened.load_projection().unwrap();
    assert_eq!(
        projection.root_agent_instance_id.as_deref(),
        Some(root.agent_instance_id.as_str())
    );
    let recovered_child = projection.agents.get("agent-coder-1").unwrap();
    assert_eq!(recovered_child.identity, child);
    assert_eq!(recovered_child.lifecycle, AgentInstanceLifecycle::Closed);
    let inbox = reopened.agent_inbox(&root.agent_instance_id).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].report.report_id, "report-child-1");
}

#[tokio::test]
async fn private_transcripts_are_recovered_per_agent() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store.ensure_root_agent("main").unwrap();
    store
        .commit_message(message_commit("root-message", None), "main")
        .unwrap();
    for child_id in ["coder-a", "coder-b"] {
        store
            .commit_agent_command(
                "session-1",
                AgentDurableCommand::Create {
                    identity: AgentInstanceIdentity {
                        session_id: "session-1".into(),
                        agent_instance_id: child_id.into(),
                        agent_spec_id: "coder".into(),
                        parent_agent_instance_id: Some("agent_session-1_root".into()),
                    },
                    spec: test_agent_spec("coder"),
                },
            )
            .await
            .unwrap();
        store
            .commit_message(
                MessageCommit {
                    session_id: "session-1".into(),
                    root_input_id: "input-1".into(),
                    agent_instance_id: child_id.into(),
                    message_id: format!("message-{child_id}"),
                    parent_message_id: None,
                    tree_parent_entry_id: Some("root-message".into()),
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
    let durable_a = store
        .find_committed_message("session-1", "coder-a", "message-coder-a")
        .unwrap()
        .unwrap();
    assert_eq!(durable_a.parent_id, None);
    assert_eq!(durable_a.tree_parent_id.as_deref(), Some("root-message"));
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
fn stores_and_recovers_agent_transcript_from_v4_journal() {
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
            .join("events/00000000000000000001-open.jsonl")
            .exists()
    );
    assert!(temp.path().join("readmodels").is_dir());
    assert!(!temp.path().join("agents").exists());
}

#[test]
fn corrupt_v4_session_remains_listable_with_integrity_error() {
    use std::io::Write;

    let temp = tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let persisted = repo.create("/project").unwrap();
    let session_id = persisted.state.session_id.clone();
    let session_path = persisted.path.clone();
    drop(persisted);

    let open_segment = std::fs::read_dir(session_path.join("events"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().contains("-open.jsonl"))
        .unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(open_segment)
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();

    let summaries = repo.summaries(None).unwrap();
    let summary = summaries
        .iter()
        .find(|summary| summary.session_id == session_id)
        .expect("corrupt v4 session remains discoverable");
    assert!(summary.integrity_error.is_some());
}

#[test]
fn root_transcript_advances_persisted_leaf_across_reopen() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("message-1", None), "main")
        .unwrap();
    store
        .commit_message(message_commit("message-2", Some("message-1")), "main")
        .unwrap();

    let reopened = SessionStore::new(temp.path());
    assert_eq!(
        reopened
            .load_projection()
            .unwrap()
            .current_leaf_id
            .as_deref(),
        Some("message-2")
    );
    JsonlSessionRepository::new(temp.path())
        .navigate(temp.path(), Some("message-1"))
        .unwrap();
    let explicitly_navigated = JsonlSessionRepository::new(temp.path())
        .load_by_path(temp.path())
        .unwrap();
    assert_eq!(
        explicitly_navigated.state.current_leaf_id.as_deref(),
        Some("message-1")
    );

    JsonlSessionRepository::new(temp.path())
        .append_entry(
            temp.path(),
            &piko_protocol::SessionTreeEntry::Label(piko_protocol::LabelEntry {
                id: "label-1".into(),
                parent_id: Some("message-1".into()),
                timestamp: "3".into(),
                target_id: "message-1".into(),
                label: Some("keep".into()),
            }),
            None,
        )
        .unwrap();
    assert_eq!(
        SessionStore::new(temp.path())
            .load_projection()
            .unwrap()
            .current_leaf_id
            .as_deref(),
        Some("message-1")
    );

    reopened
        .commit_message(message_commit("message-3", Some("message-2")), "main")
        .unwrap();
    reopened
        .commit_message(message_commit("message-1", None), "main")
        .unwrap();
    assert_eq!(
        SessionStore::new(temp.path())
            .load_projection()
            .unwrap()
            .current_leaf_id
            .as_deref(),
        Some("message-3")
    );

    let restored = JsonlSessionRepository::new(temp.path())
        .load_by_path(temp.path())
        .unwrap();
    assert_eq!(restored.state.current_leaf_id.as_deref(), Some("message-3"));
    assert!(
        restored
            .state
            .entries
            .iter()
            .any(|entry| entry.id() == "message-3")
    );
}

#[tokio::test]
async fn child_transcript_does_not_move_persisted_session_leaf() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    store
        .commit_message(message_commit("message-root", None), "main")
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-1".into(),
                    agent_instance_id: "agent-child".into(),
                    agent_spec_id: "coder".into(),
                    parent_agent_instance_id: Some("agent_session-1_root".into()),
                },
                spec: test_agent_spec("coder"),
            },
        )
        .await
        .unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                root_input_id: "input-1".into(),
                agent_instance_id: "agent-child".into(),
                message_id: "message-child".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::User {
                    content: MessageContent::String("private".into()),
                    timestamp: Some(3),
                },
                committed_at: 3,
            },
            "coder",
        )
        .unwrap();

    assert_eq!(
        store.load_projection().unwrap().current_leaf_id.as_deref(),
        Some("message-root")
    );
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
    conflict.root_input_id = "different-exec".into();
    assert_eq!(
        store.commit_message(conflict, "main"),
        Err(CommitError::IdempotencyConflict)
    );
}

#[tokio::test]
async fn fork_to_copies_agent_history_with_rewritten_session_id() {
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
    let projection = forked.load_projection().unwrap();
    assert_eq!(projection.session_id, "session-2");
    let recovered = forked
        .load_agent("session-2", "agent_session-1_root")
        .unwrap();
    assert_eq!(recovered.transcript.len(), 1);
}

#[tokio::test]
async fn durable_commands_serialize_across_independent_store_handles() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();

    let left = SessionStore::new(temp.path());
    let right = SessionStore::new(temp.path());
    let left_cmd = AgentDurableCommand::Create {
        identity: AgentInstanceIdentity {
            session_id: "session-1".into(),
            agent_instance_id: "child-a".into(),
            agent_spec_id: "coder".into(),
            parent_agent_instance_id: Some(root.agent_instance_id.clone()),
        },
        spec: test_agent_spec("coder"),
    };
    let right_cmd = AgentDurableCommand::Create {
        identity: AgentInstanceIdentity {
            session_id: "session-1".into(),
            agent_instance_id: "child-b".into(),
            agent_spec_id: "reviewer".into(),
            parent_agent_instance_id: Some(root.agent_instance_id),
        },
        spec: test_agent_spec("reviewer"),
    };

    let (left_ack, right_ack) = tokio::join!(
        left.commit_agent_command("session-1", left_cmd),
        right.commit_agent_command("session-1", right_cmd),
    );
    left_ack.expect("left create");
    right_ack.expect("right create");

    let projection = SessionStore::new(temp.path()).load_projection().unwrap();
    assert!(projection.agents.contains_key("child-a"));
    assert!(projection.agents.contains_key("child-b"));
    assert!(projection.journal_revision >= 3);
}

include!("session_store_cases/durable_agent.rs");
include!("session_store_cases/canonical_agent.rs");
include!("session_store_cases/branch_point_fork.rs");
