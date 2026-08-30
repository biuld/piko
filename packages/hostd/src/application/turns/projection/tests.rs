#[cfg(test)]
use piko_protocol::Message;

use crate::domain::sessions::HostState;
use crate::infra::storage::SessionStore;
use crate::ports::SessionStoreFactory;

use super::*;

#[path = "projection_stream_tests.rs"]
mod stream_tests;

#[tokio::test]
async fn project_committed_message_reads_session_store() {
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
                tree_parent_entry_id: None,
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
    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    let projection = project_committed_message(
        &state,
        Some(async_store.as_ref()),
        "session-1",
        &root.agent_instance_id,
        "msg-followup",
    )
    .await
    .expect("projection should load from store");
    assert_eq!(projection.message_id, "msg-followup");
    assert_eq!(projection.transcript_seq, 1);
}

#[tokio::test]
async fn reconciliation_rebuilds_missing_committed_projection_from_journal() {
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
                tree_parent_entry_id: None,
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

    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    reconcile_committed_messages(&mut state, async_store.as_ref(), "session-1")
        .await
        .unwrap();

    assert!(state.session("session-1").unwrap().entries.iter().any(
        |entry| matches!(entry, SessionTreeEntry::Message(message) if message.id == "message-rebuild")
    ));
}

#[tokio::test]
async fn reconciliation_does_not_graft_existing_root_message_under_current_leaf() {
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
                message_id: "root-msg".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::User {
                    content: MessageContent::String("first".into()),
                    timestamp: Some(1),
                },
                committed_at: 1,
            },
            "main",
        )
        .unwrap();
    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "followup-msg".into(),
                parent_message_id: Some("root-msg".into()),
                tree_parent_entry_id: Some("root-msg".into()),
                message: Message::Assistant {
                    content: vec![piko_protocol::ContentBlock::Text {
                        text: "reply".into(),
                    }],
                    checkpoint: None,
                    provider: "test".into(),
                    model: "test".into(),
                    usage: None,
                    stop_reason: Some("stop".into()),
                    error_message: None,
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();

    // Simulate a session open: entries already projected from the journal with
    // the correct tree parents, leaf pointing at the latest message.
    let mut state = HostState::default();
    let mut session =
        crate::domain::sessions::SessionState::new("session-1".into(), "/project".into());
    session
        .entries
        .push(SessionTreeEntry::Message(piko_protocol::MessageEntry {
            id: "root-msg".into(),
            parent_id: None,
            timestamp: "1".into(),
            agent_id: "main".into(),
            agent_instance_id: root.agent_instance_id.clone(),
            source_turn_id: "turn-1".into(),
            transcript_seq: 1,
            message: Message::User {
                content: MessageContent::String("first".into()),
                timestamp: Some(1),
            },
        }));
    session
        .entries
        .push(SessionTreeEntry::Message(piko_protocol::MessageEntry {
            id: "followup-msg".into(),
            parent_id: Some("root-msg".into()),
            timestamp: "2".into(),
            agent_id: "main".into(),
            agent_instance_id: root.agent_instance_id.clone(),
            source_turn_id: "turn-1".into(),
            transcript_seq: 2,
            message: Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "reply".into(),
                }],
                checkpoint: None,
                provider: "test".into(),
                model: "test".into(),
                usage: None,
                stop_reason: Some("stop".into()),
                error_message: None,
                timestamp: Some(2),
            },
        }));
    session.current_leaf_id = Some("followup-msg".into());
    state.insert_session(session);

    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    reconcile_committed_messages(&mut state, async_store.as_ref(), "session-1")
        .await
        .unwrap();

    let entries = &state.session("session-1").unwrap().entries;
    let root = entries
        .iter()
        .find_map(|entry| match entry {
            SessionTreeEntry::Message(message) if message.id == "root-msg" => Some(message),
            _ => None,
        })
        .expect("root message entry");
    assert_eq!(
        root.parent_id, None,
        "existing root message must not be grafted under the current leaf"
    );
}

#[tokio::test]
async fn record_committed_message_projects_into_host_state() {
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
                tree_parent_entry_id: None,
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
    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    let projection = record_committed_message(
        &mut state,
        Some(async_store.as_ref()),
        "session-1",
        &root.agent_instance_id,
        "msg-followup",
    )
    .await
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
    .await
    .expect("projection should load from barrier-updated host state");
    assert_eq!(again.message_id, "msg-followup");
    assert_eq!(again.transcript_seq, 1);
}

#[tokio::test]
async fn durable_tool_change_rebuilds_turn_diff_without_workspace_read() {
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
                tree_parent_entry_id: None,
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
    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    record_committed_message(
        &mut state,
        Some(async_store.as_ref()),
        "session-1",
        &root.agent_instance_id,
        "tool-result-1",
    )
    .await
    .unwrap();

    let diff = state.turn_diff("session-1", "turn-1").unwrap();
    assert_eq!(diff.files[0].path, "src/a.rs");
    assert!(diff.unified_diff.contains("-old"));
    assert!(diff.unified_diff.contains("+new"));
}

#[tokio::test]
async fn todo_write_empty_clear_projects_pending_and_removes_durable_map() {
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let agent = root.agent_instance_id.clone();

    let mut state = HostState::default();
    state.insert_session(crate::domain::sessions::SessionState::new(
        "session-1".into(),
        "/project".into(),
    ));
    // Seed a non-empty list, then clear via empty todo_write.
    {
        let session = state.session_mut("session-1").unwrap();
        session.set_todo_list(piko_protocol::TodoList {
            agent_instance_id: agent.clone(),
            items: vec![piko_protocol::TodoItem {
                id: "1".into(),
                status: piko_protocol::TodoStatus::Pending,
                content: "keep until clear".into(),
                detail: None,
            }],
            updated_at: 1,
            revision: 1,
        });
        // Drain the seed pending so only the clear write is observed.
        let _ = session.take_pending_todo_projection();
        assert!(session.todo_lists.contains_key(&agent));
    }

    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: agent.clone(),
                message_id: "todo-clear".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::ToolResult {
                    tool_call_id: "call-todo".into(),
                    tool_name: Some("todo_write".into()),
                    content: vec![piko_protocol::ContentBlock::Text {
                        text: r#"{"todos":[]}"#.into(),
                    }],
                    details: Some(serde_json::json!({ "todos": [] })),
                    is_error: Some(false),
                    timestamp: Some(3),
                },
                committed_at: 3,
            },
            "main",
        )
        .unwrap();

    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    record_committed_message(
        &mut state,
        Some(async_store.as_ref()),
        "session-1",
        &agent,
        "todo-clear",
    )
    .await
    .unwrap()
    .expect("committed projection");

    let session = state.session_mut("session-1").unwrap();
    // Durable map cleared.
    assert!(
        !session.todo_lists.contains_key(&agent),
        "empty write must remove durable in-memory list"
    );
    // Pending projection still carries empty list for live emit + disk clear.
    let pending = session
        .take_pending_todo_projection()
        .expect("empty clear must queue pending projection");
    assert!(pending.items.is_empty());
    assert_eq!(pending.agent_instance_id, agent);
    assert_eq!(pending.revision, 2); // bumped from previous revision 1
}

#[tokio::test]
async fn todo_write_nonempty_sets_map_and_pending() {
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let agent = root.agent_instance_id.clone();

    store
        .commit_message(
            MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-1".into()),
                execution_id: "exec-1".into(),
                agent_instance_id: agent.clone(),
                message_id: "todo-write-1".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::ToolResult {
                    tool_call_id: "call-todo".into(),
                    tool_name: Some("todo_write".into()),
                    content: vec![piko_protocol::ContentBlock::Text { text: "ok".into() }],
                    details: Some(serde_json::json!({
                        "todos": [
                            { "id": "1", "status": "pending", "content": "Ship it" }
                        ]
                    })),
                    is_error: Some(false),
                    timestamp: Some(2),
                },
                committed_at: 2,
            },
            "main",
        )
        .unwrap();
    assert!(
        store
            .load_projection()
            .unwrap()
            .agents
            .get(&agent)
            .unwrap()
            .todo_list
            .is_some(),
        "todo replacement must share the message commit"
    );

    let mut state = HostState::default();
    state.insert_session(crate::domain::sessions::SessionState::new(
        "session-1".into(),
        "/project".into(),
    ));
    let async_store = crate::adapters::storage::FsSessionStoreFactory.open(temp.path());
    record_committed_message(
        &mut state,
        Some(async_store.as_ref()),
        "session-1",
        &agent,
        "todo-write-1",
    )
    .await
    .unwrap();

    let session = state.session_mut("session-1").unwrap();
    let list = session.todo_lists.get(&agent).expect("list stored");
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].id, "1");
    let pending = session.take_pending_todo_projection().unwrap();
    assert_eq!(pending.items.len(), 1);
}

#[test]
fn empty_clear_pending_drives_durable_none() {
    use piko_protocol::execution::MessageCommit;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let agent = root.agent_instance_id.clone();

    let tool_result = |message_id: &str, parent_message_id: Option<&str>, todos| MessageCommit {
        session_id: "session-1".into(),
        source_turn_id: Some("turn-1".into()),
        execution_id: "exec-1".into(),
        agent_instance_id: agent.clone(),
        message_id: message_id.into(),
        parent_message_id: parent_message_id.map(str::to_string),
        tree_parent_entry_id: None,
        message: Message::ToolResult {
            tool_call_id: format!("call-{message_id}"),
            tool_name: Some("todo_write".into()),
            content: vec![piko_protocol::ContentBlock::Text { text: "ok".into() }],
            details: Some(serde_json::json!({ "todos": todos })),
            is_error: Some(false),
            timestamp: Some(2),
        },
        committed_at: 2,
    };
    store
        .commit_message(
            tool_result(
                "todo-1",
                None,
                serde_json::json!([{
                    "id": "1",
                    "status": "pending",
                    "content": "x"
                }]),
            ),
            "main",
        )
        .unwrap();
    assert!(
        store
            .load_projection()
            .unwrap()
            .agents
            .get(&agent)
            .unwrap()
            .todo_list
            .is_some()
    );

    store
        .commit_message(
            tool_result("todo-2", Some("todo-1"), serde_json::json!([])),
            "main",
        )
        .unwrap();

    let after = store.load_projection().unwrap();
    assert!(
        after.agents.get(&agent).unwrap().todo_list.is_none(),
        "empty clear must drop durable todo_list field"
    );
}
