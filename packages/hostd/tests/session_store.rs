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

#[tokio::test]
async fn recovery_marks_accepted_execution_interrupted() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                run_id: "exec-interrupted".into(),
                internal_execution_id: "exec-interrupted".into(),
                request_id: "request-interrupted".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: None,
                started_at: 1,
            },
        )
        .await
        .unwrap();

    assert_eq!(store.interrupt_incomplete_agent_executions().unwrap(), 1);
    assert_eq!(store.interrupt_incomplete_agent_executions().unwrap(), 0);
    let manifest = store.load_manifest().unwrap();
    let execution = manifest.agent_executions.get("exec-interrupted").unwrap();
    assert_eq!(execution.status, piko_protocol::ExecutionStatus::Cancelled);
    assert!(matches!(
        execution.report.as_ref().map(|report| &report.outcome),
        Some(piko_protocol::ExecutionOutcome::Cancelled { .. })
    ));
}

#[tokio::test]
async fn detached_delivery_recovery_is_pending_until_idempotent_inbox_commit() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child = AgentInstanceIdentity {
        session_id: "session-1".into(),
        agent_instance_id: "child".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: Some(root.agent_instance_id.clone()),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::Create {
                identity: child.clone(),
                spec: test_agent_spec("main"),
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: child.agent_instance_id.clone(),
                run_id: "run-detached".into(),
                internal_execution_id: "exec-detached".into(),
                request_id: "request-detached".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: Some(root.agent_instance_id.clone()),
                started_at: 2,
            },
        )
        .await
        .unwrap();
    let report = AgentExecutionReport {
        agent_instance_id: child.agent_instance_id.clone(),
        execution_id: "exec-detached".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "detached result".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunTerminal {
                run_id: "run-detached".into(),
                report: report.clone(),
                finished_at: 3,
            },
        )
        .await
        .unwrap();

    let pending = store
        .pending_detached_deliveries(&child.agent_instance_id)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].report, report);

    for _ in 0..2 {
        store
            .commit_agent_command(
                "session-1",
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id: root.agent_instance_id.clone(),
                    report: report.clone(),
                },
            )
            .await
            .unwrap();
    }

    assert!(
        store
            .pending_detached_deliveries(&child.agent_instance_id)
            .unwrap()
            .is_empty()
    );
    let inbox = store.agent_inbox(&root.agent_instance_id).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].report, report);
}

#[tokio::test]
async fn duplicate_run_start_and_terminal_are_idempotent() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let start = AgentDurableCommand::RunStarted {
        agent_instance_id: root.agent_instance_id.clone(),
        run_id: "run-idempotent".into(),
        internal_execution_id: "exec-idempotent".into(),
        request_id: "request-idempotent".into(),
        source_turn_id: None,
        detached_recipient_agent_instance_id: None,
        started_at: 1,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", start.clone())
            .await
            .unwrap();
    }
    let report = AgentExecutionReport {
        agent_instance_id: root.agent_instance_id.clone(),
        execution_id: "exec-idempotent".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    let terminal = AgentDurableCommand::RunTerminal {
        run_id: "run-idempotent".into(),
        report: report.clone(),
        finished_at: 2,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", terminal.clone())
            .await
            .unwrap();
    }
    let manifest = store.load_manifest().unwrap();
    assert_eq!(manifest.agent_executions.len(), 1);
    assert_eq!(
        manifest
            .agent_executions
            .get("run-idempotent")
            .unwrap()
            .report
            .as_ref(),
        Some(&report)
    );
}

#[tokio::test]
async fn follow_up_queue_is_durable_and_advances_atomically_into_a_run() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let queued = piko_protocol::DurableAgentInput {
        queued_input_id: "queued-1".into(),
        request: piko_protocol::SendAgentInputRequest {
            request_id: "queued-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: root.agent_instance_id.clone(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-queued".into()),
            source_turn_id: None,
            message_id: "message-queued".into(),
            content: MessageContent::String("follow up".into()),
            delivery: piko_protocol::AgentInputDelivery::FollowUp,
        },
        detached_recipient_agent_instance_id: None,
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::InputQueued {
                agent_instance_id: root.agent_instance_id.clone(),
                queued_input: queued.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.agent_queued_inputs(&root.agent_instance_id).unwrap(),
        vec![queued]
    );

    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::QueuedInputStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                queued_input_id: "queued-1".into(),
                run_id: "run-queued".into(),
                internal_execution_id: "exec-queued".into(),
                request_id: "queued-1".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: None,
                started_at: 2,
            },
        )
        .await
        .unwrap();
    let manifest = store.load_manifest().unwrap();
    assert!(manifest.agent_input_queue.is_empty());
    assert_eq!(
        manifest
            .agent_executions
            .get("run-queued")
            .unwrap()
            .execution_id,
        "exec-queued"
    );
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
