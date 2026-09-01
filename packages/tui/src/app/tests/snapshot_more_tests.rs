use super::*;

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
        root_input_id: "work-1".into(),
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
        root_input_id: "work-1".into(),
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
                model_steps: Vec::new(),
                current_leaf_id: Some("msg-tool".into()),
                selected_agent_instance_id: None,
                agent_work: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
                agent_usage: Vec::new(),
                todo_lists: Vec::new(),
            },
            agents: Vec::new(),
        },
    ));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![TimelineKind::Assistant, TimelineKind::Tool]
    );
    assert_eq!(app.timeline().tool_call_count(), 1);
    assert_eq!(app.timeline().tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(
        app.timeline().tool_calls[0].args,
        "{\"path\":\"Cargo.toml\"}"
    );
    assert_eq!(app.timeline().tool_calls[0].result.as_deref(), Some("done"));
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
        root_input_id: "work-1".into(),
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
        root_input_id: "work-child".into(),
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
                model_steps: Vec::new(),
                current_leaf_id: Some("child".into()),
                selected_agent_instance_id: Some("task-child".into()),
                agent_work: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
                agent_usage: Vec::new(),
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
    let mut reconcile = empty_reconcile("session-1");
    if let Event::SessionReconciled(event) = &mut reconcile {
        event.snapshot.selected_agent_instance_id = Some("task-1".into());
        event.snapshot.agent_work = vec![piko_protocol::AgentWorkSnapshot {
            agent_instance_id: "task-1".into(),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            foreground: piko_protocol::AgentForeground::Queued,
            active_work: None,
            pending_steers: vec![piko_protocol::AgentInputSummary {
                input_id: "steer-1".into(),
                origin: piko_protocol::AgentInputOrigin::User,
                preview: "steer".into(),
                admission_revision: 1,
                submitted_at: 1,
                delivery: piko_protocol::AgentInputDelivery::SteerActive,
                disposition: piko_protocol::AgentInputDisposition::PendingSteer,
            }],
            queued_inputs: vec![
                piko_protocol::AgentInputSummary {
                    input_id: "q-1".into(),
                    origin: piko_protocol::AgentInputOrigin::User,
                    preview: "follow".into(),
                    admission_revision: 2,
                    submitted_at: 2,
                    delivery: piko_protocol::AgentInputDelivery::FollowUp,
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                },
                piko_protocol::AgentInputSummary {
                    input_id: "q-2".into(),
                    origin: piko_protocol::AgentInputOrigin::User,
                    preview: "later".into(),
                    admission_revision: 3,
                    submitted_at: 3,
                    delivery: piko_protocol::AgentInputDelivery::FollowUp,
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                },
            ],
            pending_action: None,
        }];
        event.agents[0].agent_instance_id = "task-1".into();
    }
    app.apply_event(reconcile);

    assert_eq!(app.queue_status.steer_count, 1);
    assert_eq!(app.queue_status.follow_up_count, 2);
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
        root_input_id: "work-a".into(),
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
        root_input_id: "work-b".into(),
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
        root_input_id: "work-c".into(),
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
        root_input_id: "work-d".into(),
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
