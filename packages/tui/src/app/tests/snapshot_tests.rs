use super::*;

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
            session_id: "session-1".into(),
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
    app.session.opening_id = Some("session-1".into());
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
                selected_agent_instance_id: None,
                active_turns: Vec::new(),
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
    let mut app = live_app();

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
fn stale_session_events_do_not_mutate_live_view() {
    let mut app = live_app();

    app.apply_event(Event::Queue(piko_protocol::QueueEvent::Updated {
        session_id: "session-2".into(),
        steer_count: 9,
        follow_up_count: 9,
        next_turn_count: 9,
        steer_preview: None,
        follow_up_preview: None,
    }));
    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Started {
        session_id: "session-2".into(),
        turn_id: "foreign-turn".into(),
        agent_instance_id: "foreign-agent".into(),
        timestamp: 0,
    }));

    assert_eq!(app.queue_status.steer_count, 0);
    assert!(app.session.active_turns.is_empty());
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
