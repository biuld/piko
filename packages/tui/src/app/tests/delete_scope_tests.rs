use super::*;

#[test]
fn delete_current_session_waits_for_authoritative_clear() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline_mut().push_session_fact(
        "keep-entry".into(),
        "session",
        "keep until listed".into(),
    );

    let effects = app.delete_current_session();
    assert!(app.session.id.as_deref() == Some("session-1"));
    assert!(!app.timeline().components.is_empty());
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::SessionDelete { session_id, .. })]
            if session_id == "session-1"
    ));
    let _ = app.apply_event(Event::SessionCleared(piko_protocol::SessionClearedEvent {
        previous_session_id: "session-1".into(),
    }));
    assert!(app.session.id.is_none());
    assert!(app.timeline().components.is_empty());
}

#[test]
fn tool_execution_scopes_to_non_active_agent_timeline() {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("active".into());

    app.apply_event(Event::StreamItem(
        piko_protocol::StreamItemPatch::from_tool_execution(
            &piko_protocol::ToolExecutionEvent::Started {
                session_id: "session-1".into(),
                agent_instance_id: "other".into(),
                agent_id: "agent-1".into(),
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                args: json!({ "path": "Cargo.toml" }),
                parent_message_id: Some("message-1".into()),
                root_input_id: Some("turn-1".into()),
            },
        )
        .into_iter()
        .next()
        .expect("tool stream item"),
    ));

    assert!(app.timeline().tool_calls.is_empty());
    assert_eq!(
        app.timelines
            .inactive("other")
            .map(|t| t.tool_calls.len())
            .unwrap_or(0),
        1
    );
}
