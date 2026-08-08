use super::*;

#[test]
fn timeline_tool_card_failed_on_error_result() {
    let mut state = live_state();
    let tl = state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .get_mut("root")
        .unwrap();
    tl.apply_committed(
        "tc-err".into(),
        2,
        piko_protocol::Message::ToolCall {
            id: "call-err".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
            model: None,
            provider: None,
            timestamp: Some(2),
        },
        "turn-e".into(),
    );
    tl.apply_committed(
        "tr-err".into(),
        3,
        piko_protocol::Message::ToolResult {
            tool_call_id: "call-err".into(),
            tool_name: Some("bash".into()),
            content: vec![piko_protocol::ContentBlock::Text {
                text: "boom".into(),
            }],
            details: None,
            is_error: Some(true),
            timestamp: Some(3),
        },
        "turn-e".into(),
    );

    let vm = derive_timeline(&state);
    let call_row = vm.rows.iter().find(|r| r.label == "bash").unwrap();
    assert_eq!(call_row.tool_status, Some(ToolCardStatus::Failed));
    assert!(call_row.body.contains("boom"));
    assert_eq!(
        vm.rows
            .iter()
            .filter(|r| r.kind == TimelineRowKind::Tool)
            .count(),
        1
    );

    let activity = derive_activity(&state);
    assert!(
        activity
            .items
            .iter()
            .any(|i| i.kind == ActivityItemKind::ToolFailed)
    );
}

#[test]
fn activity_and_composer_show_stop_when_queued() {
    let mut state = live_state();
    state
        .live_session
        .as_mut()
        .unwrap()
        .active_turns
        .push(ActiveTurn {
            turn_id: "tq".into(),
            agent_instance_id: "root".into(),
            status: TurnStatus::Queued,
        });
    let activity = derive_activity(&state);
    assert!(activity.show_stop);
    assert!(
        activity.summary.to_lowercase().contains("queued")
            || activity
                .items
                .iter()
                .any(|i| i.kind == ActivityItemKind::TurnQueued)
    );

    let composer = derive_composer(&state);
    assert!(composer.show_stop);
}

#[test]
fn activity_projects_core_queue_tool_and_turn_failure() {
    let mut state = live_state();
    let live = state.live_session.as_mut().unwrap();
    live.queue.next_turn_count = 2;
    live.turn_failures
        .push(piko_client_core::state::TurnFailure {
            turn_id: "failed-turn".into(),
            agent_instance_id: "root".into(),
            error: "provider unavailable".into(),
        });
    let mut timeline = AgentTimeline::default();
    timeline.apply_tool_started(
        "call-live".into(),
        "exec".into(),
        serde_json::json!({"cmd": "true"}),
        None,
    );
    live.timelines.insert("root".into(), timeline);

    let vm = derive_activity(&state);
    assert!(vm.items.iter().any(|item| item.id == "host-queue"));
    assert!(
        vm.items
            .iter()
            .any(|item| item.id == "turn-fail-failed-turn")
    );
    assert!(vm.items.iter().any(|item| item.id == "tool-call-live"));
}

#[test]
fn timeline_renders_authoritative_tool_projection() {
    let mut state = live_state();
    let mut timeline = AgentTimeline::default();
    timeline.apply_tool_started(
        "call-1".into(),
        "exec".into(),
        serde_json::json!({"cmd": "true"}),
        None,
    );
    timeline.apply_tool_ended(
        "call-1".into(),
        "exec".into(),
        serde_json::json!({"exit": 0}),
        false,
    );
    state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .insert("root".into(), timeline);
    let vm = derive_timeline(&state);
    assert_eq!(vm.rows[0].tool_status, Some(ToolCardStatus::Completed));
    assert!(vm.rows[0].detail.as_deref().unwrap().contains("Arguments"));
}
