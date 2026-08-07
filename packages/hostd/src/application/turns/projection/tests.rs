#[cfg(test)]
use piko_protocol::Message;
use piko_protocol::agent_runtime::{RealtimeDelta, RealtimeDeltaEnvelope};

use crate::domain::sessions::HostState;
use crate::infra::storage::SessionStore;

use super::*;

#[test]
fn stream_projection_preserves_message_identity_and_delta_seq() {
    let events = stream_items_from_delta(
        "session-1",
        &RealtimeDeltaEnvelope {
            agent_instance_id: "root".into(),
            execution_id: "exec-1".into(),
            agent_id: "main".into(),
            message_id: Some("message-1".into()),
            delta_seq: 7,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: "hello".into(),
            },
        },
    );
    assert_eq!(events.len(), 1);
    let crate::api::ServerMessage::StreamItem(patch) = &events[0] else {
        panic!("expected StreamItem");
    };
    assert_eq!(patch.session_id.as_deref(), Some("session-1"));
    assert_eq!(patch.item_id, "message-1");
    assert_eq!(patch.delta_seq, Some(7));
    assert_eq!(patch.text.as_deref(), Some("hello"));
}

#[test]
fn stream_projection_rejects_missing_message_identity() {
    assert!(
        stream_items_from_delta(
            "session-1",
            &RealtimeDeltaEnvelope {
                agent_instance_id: "root".into(),
                execution_id: "exec-1".into(),
                agent_id: "main".into(),
                message_id: None,
                delta_seq: 0,
                delta: RealtimeDelta::MessageStarted {
                    role: piko_protocol::MessageRole::Assistant,
                },
            },
        )
        .is_empty()
    );
}

#[test]
fn project_committed_message_reads_session_store() {
    use piko_protocol::MessageContent;
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "msg-followup".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("second turn".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();

    let state = HostState::default();
    let projection = project_committed_message(
        &state,
        Some(&store),
        "session-1",
        &root.agent_instance_id,
        "msg-followup",
    )
    .expect("projection should load from store");
    assert_eq!(projection.message_id, "msg-followup");
    assert_eq!(projection.transcript_seq, 1);
}

#[test]
fn reconciliation_rebuilds_missing_committed_projection_from_agent_shard() {
    use piko_protocol::MessageContent;
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "message-rebuild".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("durable".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();
    let mut state = HostState::default();
    state.insert_session(crate::domain::sessions::SessionState::new(
        "session-1".into(),
        "/project".into(),
    ));

    reconcile_committed_messages(&mut state, &store, "session-1").unwrap();

    assert!(state.session("session-1").unwrap().entries.iter().any(
        |entry| matches!(entry, SessionTreeEntry::Message(message) if message.id == "message-rebuild")
    ));
}

#[test]
fn record_committed_message_projects_into_host_state() {
    use piko_protocol::MessageContent;
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "msg-followup".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("second turn".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();

    let mut state = HostState::default();
    state.insert_session(crate::domain::sessions::SessionState::new(
        "session-1".into(),
        "/project".into(),
    ));
    let projection = record_committed_message(
        &mut state,
        Some(&store),
        "session-1",
        &root.agent_instance_id,
        "msg-followup",
    )
    .unwrap()
    .expect("projection should load from store");
    assert_eq!(projection.message_id, "msg-followup");

    // Second call is idempotent and now hits the HostState barrier path.
    let again = project_committed_message(
        &state,
        None,
        "session-1",
        &root.agent_instance_id,
        "msg-followup",
    )
    .expect("projection should load from barrier-updated host state");
    assert_eq!(again.message_id, "msg-followup");
    assert_eq!(again.transcript_seq, 1);
}

#[test]
fn durable_tool_change_rebuilds_turn_diff_without_workspace_read() {
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "tool-result-1".into(),
                parent_message_id: None,
                message: Message::ToolResult {
                    tool_call_id: "call-1".into(),
                    tool_name: Some("edit".into()),
                    content: vec![piko_protocol::ContentBlock::Text {
                        text: "edited".into(),
                    }],
                    details: Some(serde_json::json!({
                        "edited": true,
                        "_pikoFileChange": {
                            "path": "src/a.rs",
                            "before": "old",
                            "after": "new"
                        }
                    })),
                    is_error: Some(false),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();

    let mut state = HostState::default();
    state.insert_session(crate::domain::sessions::SessionState::new(
        "session-1".into(),
        "/project".into(),
    ));
    state
        .restore_turn(
            "session-1",
            "turn-1",
            &root.agent_instance_id,
            "edit",
            crate::api::TurnStatus::Running,
        )
        .unwrap();
    record_committed_message(
        &mut state,
        Some(&store),
        "session-1",
        &root.agent_instance_id,
        "tool-result-1",
    )
    .unwrap();

    let diff = state.turn_diff("session-1", "turn-1").unwrap();
    assert_eq!(diff.files[0].path, "src/a.rs");
    assert!(diff.unified_diff.contains("-old"));
    assert!(diff.unified_diff.contains("+new"));
}
