use std::path::PathBuf;

use piko_protocol::{CommandCatalogAction, CommandCatalogItem, Message, ServerMessage as Event};
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

fn realtime(
    message_id: &str,
    seq: u64,
    delta: piko_protocol::agent_runtime::RealtimeDelta,
) -> Event {
    Event::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
        session_id: "session-1".into(),
        agent_instance_id: "task-1".into(),
        agent_id: "agent-1".into(),
        message_id: message_id.into(),
        delta_seq: seq,
        delta,
    })
}

fn committed(message_id: &str, task_seq: u64, message: Message) -> Event {
    Event::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
        session_id: "session-1".into(),
        agent_instance_id: "task-1".into(),
        agent_id: "agent-1".into(),
        source_turn_id: "work-1".into(),
        message_id: message_id.into(),
        transcript_seq: task_seq,
        message,
    })
}

fn assistant(text: &str) -> Message {
    Message::Assistant {
        content: vec![piko_protocol::ContentBlock::Text { text: text.into() }],
        api: "test".into(),
        provider: "test".into(),
        model: "test".into(),
        usage: None,
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: None,
    }
}

#[test]
fn committed_message_replaces_draft_and_rejects_late_delta() {
    let mut app = app();
    app.apply_event(realtime(
        "assistant-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "assistant-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "partial".into(),
        },
    ));
    app.apply_event(committed("assistant-1", 2, assistant("complete")));
    app.apply_event(realtime(
        "assistant-1",
        2,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: " stale".into(),
        },
    ));

    assert_eq!(app.timeline.message_ids(), vec!["assistant-1"]);
    assert_eq!(
        app.timeline.assistant_text("assistant-1").as_deref(),
        Some("complete")
    );
}

#[test]
fn committed_messages_use_task_seq_not_arrival_order() {
    let mut app = app();
    app.apply_event(committed("assistant-1", 2, assistant("answer")));
    app.apply_event(committed(
        "user-1",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("question".into()),
            timestamp: None,
        },
    ));

    assert_eq!(app.timeline.message_ids(), vec!["user-1", "assistant-1"]);
}

#[test]
fn commit_before_realtime_never_creates_a_second_draft() {
    let mut app = app();
    app.apply_event(committed("assistant-1", 2, assistant("complete")));
    app.apply_event(realtime(
        "assistant-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "assistant-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "late".into(),
        },
    ));

    assert_eq!(app.timeline.message_ids(), vec!["assistant-1"]);
    assert_eq!(
        app.timeline.assistant_text("assistant-1").as_deref(),
        Some("complete")
    );
}

#[test]
fn conflicting_duplicate_commit_requests_authoritative_snapshot() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.apply_event(committed("assistant-1", 2, assistant("first")));

    let effects = app.apply_event(committed("assistant-1", 2, assistant("conflict")));

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::StateSnapshot { session_id, .. })]
            if session_id == "session-1"
    ));
    assert_eq!(
        app.timeline.assistant_text("assistant-1").as_deref(),
        Some("first")
    );
}

#[test]
fn tool_start_and_end_update_one_timeline_item() {
    let mut app = app();

    app.apply_event(Event::ToolExecution(
        piko_protocol::ToolExecutionEvent::Started {
            agent_instance_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            args: json!({ "path": "Cargo.toml" }),
            parent_message_id: Some("message-1".into()),
        },
    ));
    app.apply_event(Event::ToolExecution(
        piko_protocol::ToolExecutionEvent::Ended {
            agent_instance_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            result: json!({ "ok": true }),
            is_error: false,
        },
    ));

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(app.timeline.tool_call_count(), 1);
}

#[test]
fn committed_tool_result_updates_existing_tool_call() {
    let mut app = app();

    app.apply_event(Event::ToolExecution(
        piko_protocol::ToolExecutionEvent::Started {
            agent_instance_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "run".into(),
            args: json!({ "cmd": "true" }),
            parent_message_id: None,
        },
    ));
    app.apply_event(Event::ToolExecution(
        piko_protocol::ToolExecutionEvent::Ended {
            agent_instance_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "run".into(),
            result: json!({"done": true}),
            is_error: true,
        },
    ));

    assert_eq!(app.timeline.tool_calls.len(), 1);
    assert_eq!(app.timeline.tool_calls[0].status, ToolStatus::Failed);
    assert_eq!(
        app.timeline.tool_calls[0].result.as_deref(),
        Some("{\"done\":true}")
    );
}

#[test]
fn assistant_streaming_updates_one_component() {
    let mut app = app();

    app.apply_event(realtime(
        "message-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "message-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "hello".into(),
        },
    ));
    app.apply_event(realtime(
        "message-1",
        2,
        piko_protocol::agent_runtime::RealtimeDelta::Thinking {
            content_index: 0,
            delta: "thought".into(),
        },
    ));
    app.apply_event(realtime(
        "message-1",
        3,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: " world".into(),
        },
    ));

    assert_eq!(
        app.timeline.component_kinds(),
        vec![TimelineKind::Assistant]
    );
}

#[test]
fn agent_disconnected_preserves_parent_task_relationship() {
    let mut app = app();

    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        agent_instance_id: "task-main".into(),
        agent_id: "main".into(),
        parent_agent_instance_id: None,
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Running,
        unread_report_count: 0,
        name: "main".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Running,
    }));
    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        agent_instance_id: "task-child".into(),
        agent_id: "hello-agent".into(),
        parent_agent_instance_id: Some("task-main".into()),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Running,
        unread_report_count: 0,
        name: "hello-agent".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Running,
    }));
    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        agent_instance_id: "task-child".into(),
        agent_id: "hello-agent".into(),
        parent_agent_instance_id: Some("task-main".into()),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Idle,
        unread_report_count: 0,
        name: "hello-agent".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Completed,
    }));

    let child = app
        .agent_panel
        .agents
        .iter()
        .find(|agent| agent.agent_instance_id == "task-child")
        .expect("child agent should remain visible");
    assert_eq!(child.parent_agent_instance_id.as_deref(), Some("task-main"));
    assert_eq!(child.status, piko_protocol::AgentStatus::Completed);
}

#[test]
fn agent_subscribe_replaces_timeline_with_agent_replay() {
    let mut app = app();
    app.apply_event(committed(
        "root-user",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("root prompt".into()),
            timestamp: None,
        },
    ));

    app.apply_event(Event::CommandResponse {
        command_id: "subscribe-1".into(),
        result: Ok(piko_protocol::CommandResult::AgentSubscribed {
            agent_instance_id: "task-child".into(),
            agent_id: "hello-agent".into(),
            snapshot: piko_protocol::AgentViewSnapshot {
                agent_instance_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                parent_agent_instance_id: Some("task-main".into()),
                status: Some(piko_protocol::AgentStatus::Running),
                next_seq: 3,
                events: vec![
                    piko_protocol::SequencedServerMessage {
                        seq: 1,
                        message: Box::new(Event::RealtimeMessage(
                            piko_protocol::RealtimeMessageEvent {
                                session_id: "session-1".into(),
                                agent_instance_id: "task-child".into(),
                                agent_id: "hello-agent".into(),
                                message_id: "message-child".into(),
                                delta_seq: 0,
                                delta:
                                    piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
                                        role: piko_protocol::MessageRole::Assistant,
                                    },
                            },
                        )),
                    },
                    piko_protocol::SequencedServerMessage {
                        seq: 2,
                        message: Box::new(Event::RealtimeMessage(
                            piko_protocol::RealtimeMessageEvent {
                                session_id: "session-1".into(),
                                agent_instance_id: "task-child".into(),
                                agent_id: "hello-agent".into(),
                                message_id: "message-child".into(),
                                delta_seq: 1,
                                delta: piko_protocol::agent_runtime::RealtimeDelta::Text {
                                    content_index: 0,
                                    delta: "Hello".into(),
                                },
                            },
                        )),
                    },
                ],
            },
            replay: Vec::new(),
            next_seq: 3,
        }),
    });

    assert_eq!(
        app.timeline.component_kinds(),
        vec![TimelineKind::Assistant]
    );
    assert_eq!(
        app.agent_panel.active_agent_instance_id.as_deref(),
        Some("task-child")
    );
}

#[test]
fn agent_subscribe_clears_optimistic_active_without_stale_timeline() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("task-1".into());
    app.apply_event(committed(
        "root-user",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("root prompt".into()),
            timestamp: None,
        },
    ));
    assert!(
        !app.timeline.components.is_empty(),
        "root timeline should have content before switch"
    );
    // Simulate AgentPanel Enter marking the child active before Subscribe returns
    // without swapping timelines.
    app.agent_panel.active_agent_instance_id = Some("task-child".into());

    app.apply_event(Event::CommandResponse {
        command_id: "subscribe-1".into(),
        result: Ok(piko_protocol::CommandResult::AgentSubscribed {
            agent_instance_id: "task-child".into(),
            agent_id: "hello-agent".into(),
            snapshot: piko_protocol::AgentViewSnapshot {
                agent_instance_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                parent_agent_instance_id: Some("task-1".into()),
                status: Some(piko_protocol::AgentStatus::Idle),
                next_seq: 1,
                events: Vec::new(),
            },
            replay: Vec::new(),
            next_seq: 1,
        }),
    });

    assert!(
        app.timeline.components.is_empty(),
        "subscribe must clear stale timeline when active was already set"
    );
    assert_eq!(
        app.agent_panel.active_agent_instance_id.as_deref(),
        Some("task-child")
    );
}

#[test]
fn snapshot_tool_result_updates_assistant_tool_call_component() {
    use piko_protocol::{
        ContentBlock, MessageEntry, SessionSnapshot, SessionTreeEntry, ToolCallEntry,
    };

    let assistant = SessionTreeEntry::Message(MessageEntry {
        id: "msg-assistant".into(),
        parent_id: None,
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: "agent-1".into(),
        agent_instance_id: "task-1".into(),
        source_turn_id: "work-1".into(),
        transcript_seq: 1,
        message: Message::Assistant {
            content: vec![ContentBlock::Text {
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
        agent_instance_id: Some("task-1".into()),
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
        agent_id: "agent-1".into(),
        agent_instance_id: "task-1".into(),
        source_turn_id: "work-1".into(),
        transcript_seq: 3,
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
    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 2,
            },
            snapshot: SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 2,
                entries: vec![assistant, tool_call, tool_result],
                current_leaf_id: Some("msg-tool".into()),
                active_turn: None,
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
            },
            agents: Vec::new(),
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
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-a".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String("A".into()),
            timestamp: None,
        },
    });
    let msg_b = SessionTreeEntry::Message(MessageEntry {
        id: "msg-b".into(),
        parent_id: Some("msg-a".into()),
        timestamp: "2026-06-29T12:01:00Z".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-b".into(),
        transcript_seq: 2,
        message: Message::User {
            content: piko_protocol::MessageContent::String("B".into()),
            timestamp: None,
        },
    });
    let msg_c = SessionTreeEntry::Message(MessageEntry {
        id: "msg-c".into(),
        parent_id: Some("msg-b".into()),
        timestamp: "2026-06-29T12:02:00Z".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-c".into(),
        transcript_seq: 3,
        message: Message::User {
            content: piko_protocol::MessageContent::String("C".into()),
            timestamp: None,
        },
    });
    let msg_d = SessionTreeEntry::Message(MessageEntry {
        id: "msg-d".into(),
        parent_id: Some("msg-b".into()),
        timestamp: "2026-06-29T12:03:00Z".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-d".into(),
        transcript_seq: 4,
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
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-main".into(),
        transcript_seq: 1,
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
fn submit_with_session_waits_for_server_committed_user_message() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.editor.restore_text("hello");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::TurnSubmit { text, .. }) if text == "hello"
    ));
    assert!(app.timeline.message_ids().is_empty());
}

#[test]
fn session_created_waits_for_reconcile_without_local_refresh_effects() {
    let mut app = app();
    app.session.initializing = true;
    app.agent_panel.begin_loading();

    let effects = app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::SessionCreated {
            session_id: "session-1".into(),
            cwd: "/tmp/piko-test".into(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });

    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert!(app.agent_panel.is_loading());
    assert!(app.session.initializing);
    assert!(effects.is_empty());
}

#[test]
fn cold_start_idle_no_session_shows_empty_agents_not_loading() {
    let app = app();
    assert!(!app.session.initializing);
    assert!(!app.agent_panel.is_loading());
    assert!(app.agent_panel.agents.is_empty());
}

#[test]
fn open_or_continue_boot_starts_agent_panel_loading() {
    let open = AppState::new(
        PathBuf::from("/tmp/piko-test"),
        Some("session-1".into()),
        false,
        InitialOptions::default(),
    );
    assert!(open.session.initializing);
    assert!(open.agent_panel.is_loading());

    let cont = AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        true,
        InitialOptions::default(),
    );
    assert!(cont.session.initializing);
    assert!(cont.agent_panel.is_loading());
}

#[test]
fn session_reconciled_marks_agents_hydrated_with_host_names() {
    let mut app = app();
    app.agent_panel.begin_loading();
    assert!(app.agent_panel.is_loading());

    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::InitialHydration,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 0,
            },
            snapshot: piko_protocol::SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 0,
                entries: Vec::new(),
                current_leaf_id: None,
                active_turn: None,
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
            },
            agents: vec![piko_protocol::AgentInfo {
                agent_instance_id: "task-main".into(),
                agent_id: "main".into(),
                parent_agent_instance_id: None,
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count: 0,
                name: "Main".into(),
                role: "root".into(),
                status: piko_protocol::AgentStatus::Idle,
            }],
        },
    ));

    assert!(!app.agent_panel.is_loading());
    assert!(!app.session.initializing);
    assert_eq!(app.agent_panel.agents.len(), 1);
    assert_eq!(app.agent_panel.agents[0].name, "Main");
    assert_eq!(app.agent_panel.agents[0].agent_id, "main");
}

#[test]
fn session_opened_keeps_agent_panel_loading_until_reconcile() {
    let mut app = app();
    app.session.initializing = true;
    app.agent_panel.begin_loading();

    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::SessionOpened {
            session_id: "session-1".into(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });

    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert!(app.agent_panel.is_loading());
    assert!(app.session.initializing);
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

    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: test_command_catalog(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });
    app.refresh_suggestions();
    app.dispatch(EditorAction::AcceptSuggestion.into());
    assert_eq!(app.editor.text(), "/help ");
}

#[test]
fn test_completion_cycling_fills_editor() {
    let mut app = app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
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
        }),
        command_id: "test".into(),
    });

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

#[test]
fn delete_current_session_waits_for_listed_before_clearing() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline
        .push(crate::features::timeline::TimelineEntry::System(
            "keep until listed".into(),
        ));

    let effects = app.delete_current_session();
    assert!(app.session.id.as_deref() == Some("session-1"));
    assert!(!app.timeline.components.is_empty());
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::SessionDelete { session_id, .. })]
            if session_id == "session-1"
    ));
    let command_id = match &effects[0] {
        Effect::Send(piko_protocol::Command::SessionDelete { command_id, .. }) => {
            command_id.clone()
        }
        _ => unreachable!(),
    };

    let _ = app.apply_event(Event::CommandResponse {
        command_id: command_id.clone(),
        result: Ok(piko_protocol::CommandResult::SessionListed {
            sessions: Vec::new(),
            timestamp: 0,
        }),
    });
    assert!(app.session.id.is_none());
    assert!(app.timeline.components.is_empty());
}

#[test]
fn tool_execution_scopes_to_non_active_agent_timeline() {
    let mut app = app();
    app.agent_panel.active_agent_instance_id = Some("active".into());

    app.apply_event(Event::ToolExecution(
        piko_protocol::ToolExecutionEvent::Started {
            agent_instance_id: "other".into(),
            agent_id: "agent-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            args: json!({ "path": "Cargo.toml" }),
            parent_message_id: Some("message-1".into()),
        },
    ));

    assert!(app.timeline.tool_calls.is_empty());
    assert_eq!(
        app.agent_timelines
            .get("other")
            .map(|t| t.tool_calls.len())
            .unwrap_or(0),
        1
    );
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
