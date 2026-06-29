use std::path::PathBuf;

use piko_protocol::{ContentBlock, Event, Message};
use serde_json::json;

use crate::{
    action::Action,
    app::{AppState, InitialOptions, ToolStatus, get_active_branch_entries},
    panels::timeline::TimelineEntry,
};

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

#[test]
fn tool_start_and_end_update_one_timeline_item() {
    let mut app = app();

    app.apply_event(
        None,
        Event::ToolStart {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            args: json!({ "path": "Cargo.toml" }),
            parent_message_id: Some("message-1".into()),
        },
    );
    app.apply_event(
        None,
        Event::ToolEnd {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            result: json!({ "ok": true }),
            is_error: false,
        },
    );

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(
        app.timeline
            .entries
            .iter()
            .filter(|entry| matches!(entry, TimelineEntry::Tool(_)))
            .count(),
        1
    );
}

#[test]
fn committed_tool_result_updates_existing_tool_call() {
    let mut app = app();

    app.apply_event(
        None,
        Event::ToolStart {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "run".into(),
            args: json!({ "cmd": "true" }),
            parent_message_id: None,
        },
    );
    app.apply_event(
        None,
        Event::ToolResultCommitted {
            session_id: "session-1".into(),
            message_id: "message-1".into(),
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            message: Message::ToolResult {
                tool_call_id: "call-1".into(),
                tool_name: Some("run".into()),
                content: vec![ContentBlock::Text {
                    text: "done".into(),
                }],
                details: None,
                is_error: Some(true),
                timestamp: None,
            },
        },
    );

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Failed);
    assert_eq!(app.timeline.tool_calls[0].result.as_deref(), Some("done"));
}

#[test]
fn queue_update_populates_status_data() {
    let mut app = app();

    app.apply_event(
        None,
        Event::QueueUpdate {
            session_id: "session-1".into(),
            steer_count: 1,
            follow_up_count: 2,
            next_turn_count: 3,
            steer_preview: Some("steer".into()),
            follow_up_preview: Some("follow".into()),
        },
    );

    assert_eq!(app.queue_status.steer_count, 1);
    assert_eq!(app.queue_status.follow_up_count, 2);
    assert_eq!(app.queue_status.next_turn_count, 3);
    assert_eq!(app.queue_status.steer_preview.as_deref(), Some("steer"));
    assert_eq!(
        app.queue_status.follow_up_preview.as_deref(),
        Some("follow")
    );
}

#[test]
fn test_active_branch_entries_filtering() {
    use piko_protocol::{MessageEntry, SessionTreeEntry};

    let msg_a = SessionTreeEntry::Message(MessageEntry {
        id: "msg-a".into(),
        parent_id: None,
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: None,
        message: Message::User {
            content: piko_protocol::MessageContent::String("A".into()),
            timestamp: None,
        },
    });
    let msg_b = SessionTreeEntry::Message(MessageEntry {
        id: "msg-b".into(),
        parent_id: Some("msg-a".into()),
        timestamp: "2026-06-29T12:01:00Z".into(),
        agent_id: None,
        message: Message::User {
            content: piko_protocol::MessageContent::String("B".into()),
            timestamp: None,
        },
    });
    let msg_c = SessionTreeEntry::Message(MessageEntry {
        id: "msg-c".into(),
        parent_id: Some("msg-b".into()),
        timestamp: "2026-06-29T12:02:00Z".into(),
        agent_id: None,
        message: Message::User {
            content: piko_protocol::MessageContent::String("C".into()),
            timestamp: None,
        },
    });
    let msg_d = SessionTreeEntry::Message(MessageEntry {
        id: "msg-d".into(),
        parent_id: Some("msg-b".into()),
        timestamp: "2026-06-29T12:03:00Z".into(),
        agent_id: None,
        message: Message::User {
            content: piko_protocol::MessageContent::String("D".into()),
            timestamp: None,
        },
    });

    let entries = vec![msg_a.clone(), msg_b.clone(), msg_c.clone(), msg_d.clone()];

    let active_c = get_active_branch_entries(&entries, Some("msg-c"));
    assert_eq!(active_c.len(), 3);
    assert_eq!(active_c[0].id(), "msg-a");
    assert_eq!(active_c[1].id(), "msg-b");
    assert_eq!(active_c[2].id(), "msg-c");

    let active_d = get_active_branch_entries(&entries, Some("msg-d"));
    assert_eq!(active_d.len(), 3);
    assert_eq!(active_d[0].id(), "msg-a");
    assert_eq!(active_d[1].id(), "msg-b");
    assert_eq!(active_d[2].id(), "msg-d");
}

#[test]
fn test_unknown_slash_command_blocks_submit() {
    let mut app = app();
    app.editor.insert_char('/');
    app.editor.insert_char('u');
    app.editor.insert_char('n');
    app.editor.insert_char('k');
    app.editor.insert_char('n');
    app.editor.insert_char('o');
    app.editor.insert_char('w');
    app.editor.insert_char('n');

    let mut host = crate::host::HostdClient::spawn(
        "true".to_string(), // dummy command
        vec![],
    )
    .unwrap();

    app.dispatch(&mut host, Action::Submit);

    // Because it's an unknown slash command, it should block submission,
    // so the editor should NOT be cleared (normal submits clear the editor).
    assert_eq!(app.editor.text(), "/unknown");
    assert!(app.status.contains("Unknown slash command"));

    host.shutdown();
}
