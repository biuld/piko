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

    app.apply_event(committed("assistant-2", 3, assistant("second")));
    app.apply_event(model_step("step-2", 2, "assistant-2", Vec::new()));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![
            TimelineKind::Assistant,
            TimelineKind::Tool,
            TimelineKind::ModelStepDivider,
            TimelineKind::Assistant,
        ]
    );
}
