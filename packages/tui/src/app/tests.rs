use std::path::PathBuf;

use piko_protocol::{
    CommandCatalogAction, CommandCatalogItem, ContentBlock, Message, ServerMessage as Event,
};
use serde_json::json;

use crate::app::{
    AppState, InitialOptions, ToolStatus, command::EditorAction, effect::Effect,
    get_active_branch_entries,
};
use crate::features::timeline::TimelineKind;

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

    app.apply_event(Event::Tool(piko_protocol::ToolEvent::Start {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        args: json!({ "path": "Cargo.toml" }),
        parent_message_id: Some("message-1".into()),
    }));
    app.apply_event(Event::Tool(piko_protocol::ToolEvent::End {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        result: json!({ "ok": true }),
        is_error: false,
    }));

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(app.timeline.tool_call_count(), 1);
}

#[test]
fn committed_tool_result_updates_existing_tool_call() {
    let mut app = app();

    app.apply_event(Event::Tool(piko_protocol::ToolEvent::Start {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        tool_call_id: "call-1".into(),
        tool_name: "run".into(),
        args: json!({ "cmd": "true" }),
        parent_message_id: None,
    }));
    app.apply_event(Event::Message(
        piko_protocol::MessageEvent::ToolResultCommitted {
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
    ));

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Failed);
    assert_eq!(app.timeline.tool_calls[0].result.as_deref(), Some("done"));
}

#[test]
fn assistant_streaming_updates_one_component() {
    let mut app = app();

    app.apply_event(Event::Message(piko_protocol::MessageEvent::Start {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        message_id: "message-1".into(),
        role: piko_protocol::MessageRole::Assistant,
    }));
    app.apply_event(Event::Message(piko_protocol::MessageEvent::TextDelta {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        message_id: "message-1".into(),
        content_index: 0,
        delta: "hello".into(),
    }));
    app.apply_event(Event::Message(piko_protocol::MessageEvent::ThinkingDelta {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        message_id: "message-1".into(),
        content_index: 0,
        delta: "thought".into(),
    }));
    app.apply_event(Event::Message(piko_protocol::MessageEvent::TextDelta {
        task_id: "task-1".into(),
        agent_id: "agent-1".into(),
        message_id: "message-1".into(),
        content_index: 0,
        delta: " world".into(),
    }));

    assert_eq!(
        app.timeline.component_kinds(),
        vec![TimelineKind::Assistant]
    );
}

#[test]
fn snapshot_tool_result_updates_assistant_tool_call_component() {
    use piko_protocol::{
        AssistantContentBlock, MessageEntry, SessionSnapshot, SessionTreeEntry, ToolCallEntry,
    };

    let assistant = SessionTreeEntry::Message(MessageEntry {
        id: "msg-assistant".into(),
        parent_id: None,
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: Some("agent-1".into()),
        message: Message::Assistant {
            content: vec![AssistantContentBlock::Text {
                text: "I'll read it.".into(),
            }],
            api: "test".into(),
            provider: "test".into(),
            model: "test".into(),
            usage: None,
            stop_reason: Some("tool_use".into()),
            error_message: None,
            timestamp: None,
        },
    });
    let tool_call = SessionTreeEntry::ToolCall(ToolCallEntry {
        id: "msg-tool-call".into(),
        parent_id: Some("msg-assistant".into()),
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: Some("agent-1".into()),
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        arguments: json!({ "path": "Cargo.toml" }),
        parent_message_id: Some("msg-assistant".into()),
        model: Some("test".into()),
        provider: Some("test".into()),
    });
    let tool_result = SessionTreeEntry::Message(MessageEntry {
        id: "msg-tool".into(),
        parent_id: Some("msg-tool-call".into()),
        timestamp: "2026-06-29T12:00:01Z".into(),
        agent_id: Some("agent-1".into()),
        message: Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("read".into()),
            content: vec![ContentBlock::Text {
                text: "done".into(),
            }],
            details: None,
            is_error: Some(false),
            timestamp: None,
        },
    });

    let mut app = app();
    app.apply_event(Event::CommandResult(
        piko_protocol::CommandResult::StateSnapshot {
            session_id: "session-1".into(),
            snapshot: SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 2,
                entries: vec![assistant, tool_call, tool_result],
                tasks: std::collections::HashMap::new(),
                current_leaf_id: Some("msg-tool".into()),
                active_turn: None,
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
            },
            timestamp: 0,
        },
    ));

    assert_eq!(
        app.timeline.component_kinds(),
        vec![TimelineKind::Assistant, TimelineKind::Tool]
    );
    assert_eq!(app.timeline.tool_call_count(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(app.timeline.tool_calls[0].args, "{\"path\":\"Cargo.toml\"}");
    assert_eq!(app.timeline.tool_calls[0].result.as_deref(), Some("done"));
}

#[test]
fn queue_update_populates_status_data() {
    let mut app = app();

    app.apply_event(Event::Queue(piko_protocol::QueueEvent::Updated {
        session_id: "session-1".into(),
        steer_count: 1,
        follow_up_count: 2,
        next_turn_count: 3,
        steer_preview: Some("steer".into()),
        follow_up_preview: Some("follow".into()),
    }));

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

fn user_tree_entry(
    id: &str,
    parent_id: Option<&str>,
    text: &str,
) -> piko_protocol::SessionTreeEntry {
    piko_protocol::SessionTreeEntry::Message(piko_protocol::MessageEntry {
        id: id.into(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: None,
        message: Message::User {
            content: piko_protocol::MessageContent::String(text.into()),
            timestamp: None,
        },
    })
}

#[test]
fn tree_summary_prompt_does_not_trigger_when_selected_user_targets_current_leaf() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("current", Some("root"), "current"),
        user_tree_entry("future-branch-user", Some("current"), "future branch"),
    ];
    app.tree.load(&entries, Some("current"));

    assert!(!app.tree_navigation_needs_summary("future-branch-user"));
}

#[test]
fn tree_summary_prompt_triggers_when_selected_user_targets_sibling_branch_parent() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("fork", Some("root"), "fork"),
        user_tree_entry("active-leaf", Some("fork"), "active"),
        user_tree_entry("sibling-user", Some("fork"), "sibling"),
    ];
    app.tree.load(&entries, Some("active-leaf"));

    assert!(app.tree_navigation_needs_summary("sibling-user"));
}

#[test]
fn tree_summary_prompt_triggers_when_root_user_abandons_current_branch() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("active-leaf", Some("root"), "active"),
    ];
    app.tree.load(&entries, Some("active-leaf"));

    assert!(app.tree_navigation_needs_summary("root"));
}

#[test]
fn submit_without_session_returns_session_create_effect() {
    let mut app = app();
    app.editor.restore_text("hello");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(app.session.initializing);
    assert_eq!(app.session.pending_turn_text.as_deref(), Some("hello"));
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::SessionCreate { cwd, .. })
            if cwd == "/tmp/piko-test"
    ));
}

#[test]
fn session_created_event_returns_snapshot_effect() {
    let mut app = app();

    let effects = app.apply_event(Event::CommandResult(
        piko_protocol::CommandResult::SessionCreated {
            session_id: "session-1".into(),
            cwd: "/tmp/piko-test".into(),
            timestamp: 0,
        },
    ));

    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(piko_protocol::Command::StateSnapshot { session_id, .. })
            if session_id == "session-1"
    )));
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

    app.dispatch(EditorAction::Submit.into());

    // Because it's an unknown slash command, it should block submission,
    // so the editor should NOT be cleared (normal submits clear the editor).
    assert_eq!(app.editor.text(), "/unknown");
    assert!(app.status.contains("Unknown slash command"));
}

#[test]
fn completion_acceptance_replaces_range() {
    let mut app = app();
    app.editor.restore_text("/he");
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/he");

    app.apply_event(Event::CommandResult(
        piko_protocol::CommandResult::CommandCatalogListed {
            commands: test_command_catalog(),
            timestamp: 0,
        },
    ));
    app.refresh_suggestions();
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/help ");
}

#[test]
fn test_completion_cycling_fills_editor() {
    let mut app = app();
    app.apply_event(Event::CommandResult(
        piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![
                CommandCatalogItem {
                    id: "help".to_string(),
                    title: "Help".to_string(),
                    detail: "Show help".to_string(),
                    action: CommandCatalogAction::Help,
                    slash_name: "/help".to_string(),
                    visible_in_palette: true,
                },
                CommandCatalogItem {
                    id: "quit".to_string(),
                    title: "Quit".to_string(),
                    detail: "Quit".to_string(),
                    action: CommandCatalogAction::Quit,
                    slash_name: "/quit".to_string(),
                    visible_in_palette: true,
                },
            ],
            timestamp: 0,
        },
    ));

    // Type "/q"
    app.editor.restore_text("/q");
    app.refresh_suggestions();

    // Check suggestions: should match /quit
    assert_eq!(app.editor.auto_complete.items.len(), 1);
    assert_eq!(app.editor.auto_complete.items[0].replacement, "/quit ");

    // Cycle next (Tab equivalent)
    app.dispatch(EditorAction::SuggestionSelectNext.into());
    // Editor should be updated automatically!
    assert_eq!(app.editor.text(), "/quit ");

    // Accept suggestion (Enter equivalent)
    app.dispatch(EditorAction::AcceptSuggestion.into());
    // Editor should remain "/quit "
    assert_eq!(app.editor.text(), "/quit ");
}

#[test]
fn test_file_completion_inserted_as_placeholder_block() {
    let mut app = app();

    // We mock file suggestions by manually updating AutoComplete state
    app.editor.auto_complete.active = true;
    app.editor.auto_complete.items = vec![crate::features::auto_completion::CompletionRow {
        replacement: "@src/main.rs ".to_string(),
        start: 0,
        end: 2,
        cells: vec![],
        keep_active: false,
    }];
    app.editor.auto_complete.selected = 0;

    // Cycle next to preview
    app.dispatch(EditorAction::SuggestionSelectNext.into());
    // Editor should be filled with the placeholder "[@src/main.rs] "
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Accept suggestion
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Deleting the last character (the space)
    app.editor.backspace();
    assert_eq!(app.editor.text(), "[@src/main.rs]");

    // Deleting again should delete the ENTIRE placeholder block!
    app.editor.backspace();
    assert_eq!(app.editor.text(), "");

    // Re-do completion and submit to verify expansion
    app.editor.auto_complete.active = true;
    app.editor.auto_complete.items = vec![crate::features::auto_completion::CompletionRow {
        replacement: "@src/main.rs ".to_string(),
        start: 0,
        end: 0,
        cells: vec![],
        keep_active: false,
    }];
    app.editor.auto_complete.selected = 0;
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "[@src/main.rs] ");

    // Get raw text (which expands references and takes the text)
    let submitted = app.editor.take_trimmed().unwrap();
    assert_eq!(submitted, "@src/main.rs");
}

#[test]
fn ctrl_p_history_works_with_live_draft() {
    let mut app = app();
    app.editor.restore_text("first");
    app.dispatch(EditorAction::Submit.into());
    app.editor.restore_text("draft");

    app.dispatch(EditorAction::HistoryPrev.into());
    assert_eq!(app.editor.text(), "first");
    app.dispatch(EditorAction::HistoryNext.into());
    assert_eq!(app.editor.text(), "draft");
}

#[test]
fn slash_completion_visible_with_empty_results() {
    let mut app = app();
    app.editor.restore_text("/zzz");
    app.refresh_suggestions();
    assert!(app.has_suggestions());
    assert!(app.editor.auto_complete.items.is_empty());
}

fn test_command_catalog() -> Vec<CommandCatalogItem> {
    vec![CommandCatalogItem {
        id: "help".to_string(),
        title: "Help".to_string(),
        detail: "Show help".to_string(),
        action: CommandCatalogAction::Help,
        slash_name: "/help".to_string(),
        visible_in_palette: true,
    }]
}
