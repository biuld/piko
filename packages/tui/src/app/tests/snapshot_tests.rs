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
                model_steps: Vec::new(),
                current_leaf_id: None,
                selected_agent_instance_id: None,
                active_turns: Vec::new(),
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
        vec![
            TimelineKind::SessionFact,
            TimelineKind::Summary,
            TimelineKind::Summary,
            TimelineKind::CustomMessage,
        ]
    );
    assert!(matches!(
        app.timeline()
            .components
            .front()
            .map(|component| component.id()),
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
                model_steps: Vec::new(),
                current_leaf_id: Some("child-message".into()),
                selected_agent_instance_id: Some("child".into()),
                active_turns: Vec::new(),
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
        vec![TimelineKind::SessionFact, TimelineKind::Assistant]
    );
    assert_eq!(
        app.timelines
            .inactive("root")
            .expect("root timeline")
            .component_kinds(),
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
        !app.timeline().components.is_empty(),
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
        app.timeline().components.is_empty(),
        "subscribe must clear stale timeline when active was already set"
    );
    assert_eq!(
        app.agent_panel.active_agent_instance_id.as_deref(),
        Some("task-child")
    );
}
