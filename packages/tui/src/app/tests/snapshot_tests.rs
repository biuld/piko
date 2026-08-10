use super::*;

#[test]
fn approval_notice_resolves_with_authoritative_event() {
    let mut app = live_app();
    app.apply_event(Event::Approval(piko_protocol::ApprovalEvent::Requested {
        session_id: "session-1".into(),
        agent_instance_id: "task-1".into(),
        agent_id: "main".into(),
        approval_id: "approval-1".into(),
        tool_name: "exec".into(),
        tool_args: serde_json::json!({}),
        prompt: None,
    }));
    assert!(
        app.notifications
            .has_row_visible_for(app.last_tick, Some("session-1"), None)
    );

    app.apply_event(Event::Approval(piko_protocol::ApprovalEvent::Resolved {
        session_id: "session-1".into(),
        approval_id: "approval-1".into(),
        decision: piko_protocol::ApprovalDecision::Decline,
    }));

    assert!(
        !app.notifications
            .has_row_visible_for(app.last_tick, Some("session-1"), None)
    );
}

#[test]
fn snapshot_projects_only_typed_timeline_entries() {
    use piko_protocol::{
        BranchSummaryEntry, CompactionEntry, CustomEntry, CustomMessageContent, CustomMessageEntry,
        LabelEntry, ModelChangeEntry, SessionInfoEntry, SessionSnapshot, SessionTreeEntry,
    };

    let entries = vec![
        SessionTreeEntry::ModelChange(ModelChangeEntry {
            id: "model-entry".into(),
            parent_id: None,
            timestamp: "1".into(),
            provider: "openai".into(),
            model_id: "gpt".into(),
        }),
        SessionTreeEntry::Compaction(CompactionEntry {
            id: "compact-entry".into(),
            parent_id: None,
            timestamp: "2".into(),
            summary: "compact summary".into(),
            first_kept_entry_id: "model-entry".into(),
            tokens_before: 100,
            details: None,
            from_hook: None,
        }),
        SessionTreeEntry::BranchSummary(BranchSummaryEntry {
            id: "branch-entry".into(),
            parent_id: None,
            timestamp: "3".into(),
            from_id: "old-leaf".into(),
            summary: "branch summary".into(),
            details: None,
            from_hook: None,
        }),
        SessionTreeEntry::CustomMessage(CustomMessageEntry {
            id: "visible-custom".into(),
            parent_id: None,
            timestamp: "4".into(),
            custom_type: "skill".into(),
            content: CustomMessageContent::String("used skill".into()),
            details: None,
            display: true,
        }),
        SessionTreeEntry::CustomMessage(CustomMessageEntry {
            id: "hidden-custom".into(),
            parent_id: None,
            timestamp: "5".into(),
            custom_type: "hidden".into(),
            content: CustomMessageContent::String("do not show".into()),
            details: None,
            display: false,
        }),
        SessionTreeEntry::Custom(CustomEntry {
            id: "metadata".into(),
            parent_id: None,
            timestamp: "6".into(),
            custom_type: "metadata".into(),
            data: None,
        }),
        SessionTreeEntry::Label(LabelEntry {
            id: "label".into(),
            parent_id: None,
            timestamp: "7".into(),
            target_id: "model-entry".into(),
            label: Some("internal label".into()),
        }),
        SessionTreeEntry::SessionInfo(SessionInfoEntry {
            id: "session-info".into(),
            parent_id: None,
            timestamp: "8".into(),
            name: Some("session name".into()),
        }),
    ];
    let mut app = app();
    app.session.opening_id = Some("session-1".into());
    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 1,
            },
            snapshot: SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 1,
                entries,
                current_leaf_id: None,
                selected_agent_instance_id: None,
                active_turns: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
                todo_lists: Vec::new(),
            },
            agents: Vec::new(),
        },
    ));

    assert_eq!(
        app.timeline.component_kinds(),
        vec![
            TimelineKind::SessionFact,
            TimelineKind::Summary,
            TimelineKind::Summary,
            TimelineKind::CustomMessage,
        ]
    );
    assert!(matches!(
        app.timeline.components.front().map(|component| component.id()),
        Some(crate::features::timeline::ComponentId::EntryId(id)) if id == "model-entry"
    ));
}

#[test]
fn session_facts_are_merged_into_every_agent_timeline() {
    use piko_protocol::{MessageEntry, ModelChangeEntry, SessionSnapshot, SessionTreeEntry};

    let model = SessionTreeEntry::ModelChange(ModelChangeEntry {
        id: "model".into(),
        parent_id: None,
        timestamp: "1".into(),
        provider: "openai".into(),
        model_id: "gpt".into(),
    });
    let root = SessionTreeEntry::Message(MessageEntry {
        id: "root-message".into(),
        parent_id: Some("model".into()),
        timestamp: "2".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        source_turn_id: "turn-root".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String("root".into()),
            timestamp: None,
        },
    });
    let child = SessionTreeEntry::Message(MessageEntry {
        id: "child-message".into(),
        parent_id: Some("root-message".into()),
        timestamp: "3".into(),
        agent_id: "child".into(),
        agent_instance_id: "child".into(),
        source_turn_id: "turn-child".into(),
        transcript_seq: 1,
        message: assistant("child"),
    });
    let mut app = app();
    app.session.opening_id = Some("session-1".into());
    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 3,
            },
            snapshot: SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp".into(),
                seq: 3,
                entries: vec![model, root, child],
                current_leaf_id: Some("child-message".into()),
                selected_agent_instance_id: Some("child".into()),
                active_turns: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
                todo_lists: Vec::new(),
            },
            agents: Vec::new(),
        },
    ));

    assert_eq!(
        app.timeline.component_kinds(),
        vec![TimelineKind::SessionFact, TimelineKind::Assistant]
    );
    assert_eq!(
        app.agent_timelines["root"].component_kinds(),
        vec![TimelineKind::SessionFact, TimelineKind::User]
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
            checkpoint: None,
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
                todo_lists: Vec::new(),
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
fn reconcile_selects_agent_and_filters_tree_to_viewed_agent() {
    use piko_protocol::{
        AgentActivity, AgentInfo, AgentInstanceLifecycle, AgentStatus, MessageEntry,
        SessionSnapshot, SessionTreeEntry, ToolCallEntry,
    };

    let root = SessionTreeEntry::Message(MessageEntry {
        id: "root".into(),
        parent_id: None,
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-1".into(),
        source_turn_id: "work-1".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String("root prompt".into()),
            timestamp: None,
        },
    });
    let spawn = SessionTreeEntry::ToolCall(ToolCallEntry {
        id: "spawn".into(),
        parent_id: Some("root".into()),
        timestamp: "2026-06-29T12:00:01Z".into(),
        agent_id: Some("main".into()),
        agent_instance_id: Some("task-1".into()),
        tool_call_id: "call-spawn".into(),
        tool_name: "spawn_agent".into(),
        arguments: serde_json::json!({}),
        parent_message_id: None,
        model: None,
        provider: None,
    });
    let child = SessionTreeEntry::Message(MessageEntry {
        id: "child".into(),
        parent_id: Some("spawn".into()),
        timestamp: "2026-06-29T12:00:02Z".into(),
        agent_id: "coder".into(),
        agent_instance_id: "task-child".into(),
        source_turn_id: "work-child".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String("child prompt".into()),
            timestamp: None,
        },
    });
    let agents = vec![
        AgentInfo {
            session_id: "session-1".into(),
            agent_instance_id: "task-1".into(),
            agent_id: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: AgentInstanceLifecycle::Open,
            activity: AgentActivity::Idle,
            unread_report_count: 0,
            name: "Main".into(),
            role: "main".into(),
            status: AgentStatus::Idle,
        },
        AgentInfo {
            session_id: "session-1".into(),
            agent_instance_id: "task-child".into(),
            agent_id: "coder".into(),
            parent_agent_instance_id: Some("task-1".into()),
            lifecycle: AgentInstanceLifecycle::Open,
            activity: AgentActivity::Idle,
            unread_report_count: 0,
            name: "Coder".into(),
            role: "coder".into(),
            status: AgentStatus::Idle,
        },
    ];

    let mut app = app();
    app.session.opening_id = Some("session-1".into());
    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 3,
            },
            snapshot: SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 3,
                entries: vec![root, spawn, child],
                current_leaf_id: Some("child".into()),
                selected_agent_instance_id: Some("task-child".into()),
                active_turns: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
                todo_lists: Vec::new(),
            },
            agents,
        },
    ));

    assert_eq!(
        app.agent_panel.active_agent_instance_id.as_deref(),
        Some("task-child")
    );
    assert_eq!(
        app.tree
            .visible
            .rows
            .iter()
            .map(|row| row.entry_id.as_str())
            .collect::<Vec<_>>(),
        vec!["child"]
    );
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
