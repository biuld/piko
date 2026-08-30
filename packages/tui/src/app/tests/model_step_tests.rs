use super::*;

#[test]
fn model_step_divider_is_inserted_only_between_steps() {
    let mut app = live_app();
    app.apply_event(committed("assistant-1", 1, assistant("first")));
    app.apply_event(model_step("step-1", 1, "assistant-1", Vec::new()));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![TimelineKind::Assistant]
    );

    app.apply_event(committed("assistant-2", 2, assistant("second")));
    app.apply_event(model_step("step-2", 2, "assistant-2", Vec::new()));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![
            TimelineKind::Assistant,
            TimelineKind::ModelStepDivider,
            TimelineKind::Assistant,
        ]
    );
}

#[test]
fn model_step_divider_waits_for_late_message_and_maps_tool_call_message_ids() {
    let mut app = live_app();
    // Boundary-first delivery is valid for the TUI projection: the divider
    // is placed once the referenced transcript item becomes visible.
    app.apply_event(model_step(
        "step-1",
        1,
        "assistant-1",
        vec!["assistant-1:tool_call:0".into()],
    ));
    app.apply_event(committed("assistant-1", 1, assistant("first")));
    app.apply_event(committed(
        "assistant-1:tool_call:0",
        2,
        Message::ToolCall {
            id: "call-1".into(),
            name: "read".into(),
            arguments: json!({"path": "Cargo.toml"}),
            model: None,
            provider: None,
            timestamp: None,
        },
    ));
    app.apply_event(committed(
        "assistant-1:tool_call:1",
        3,
        Message::ToolCall {
            id: "call-2".into(),
            name: "run".into(),
            arguments: json!({ "cmd": "true" }),
            model: None,
            provider: None,
            timestamp: None,
        },
    ));
    app.apply_event(committed(
        "assistant-1:tool_result:0",
        4,
        Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("read".into()),
            content: vec![piko_protocol::ContentBlock::Text {
                text: "done".into(),
            }],
            details: None,
            is_error: Some(false),
            timestamp: None,
        },
    ));

    app.apply_event(committed("assistant-2", 5, assistant("second")));
    app.apply_event(model_step("step-2", 2, "assistant-2", Vec::new()));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![
            TimelineKind::Assistant,
            TimelineKind::Tool,
            TimelineKind::Tool,
            TimelineKind::ModelStepDivider,
            TimelineKind::Assistant,
        ]
    );
    assert_eq!(app.timeline().tool_calls.len(), 2);
}

#[test]
fn snapshot_restores_model_step_dividers() {
    use piko_protocol::{MessageEntry, SessionSnapshot, SessionTreeEntry};

    let first = SessionTreeEntry::Message(MessageEntry {
        id: "assistant-1".into(),
        parent_id: None,
        timestamp: "1".into(),
        agent_id: "agent-1".into(),
        agent_instance_id: "task-1".into(),
        root_input_id: "work-1".into(),
        transcript_seq: 1,
        message: assistant("first"),
    });
    let second = SessionTreeEntry::Message(MessageEntry {
        id: "assistant-2".into(),
        parent_id: Some("assistant-1".into()),
        timestamp: "2".into(),
        agent_id: "agent-1".into(),
        agent_instance_id: "task-1".into(),
        root_input_id: "work-1".into(),
        transcript_seq: 2,
        message: assistant("second"),
    });
    let first_step = match model_step("step-1", 1, "assistant-1", Vec::new()) {
        Event::ModelStepCommitted(boundary) => boundary,
        _ => unreachable!(),
    };
    let second_step = match model_step("step-2", 2, "assistant-2", Vec::new()) {
        Event::ModelStepCommitted(boundary) => boundary,
        _ => unreachable!(),
    };

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
                entries: vec![first, second],
                model_steps: vec![first_step, second_step],
                current_leaf_id: Some("assistant-2".into()),
                selected_agent_instance_id: Some("task-1".into()),
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
            TimelineKind::Assistant,
            TimelineKind::ModelStepDivider,
            TimelineKind::Assistant,
        ]
    );
}
