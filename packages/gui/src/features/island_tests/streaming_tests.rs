use super::*;

#[test]
fn timeline_includes_realtime_draft() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s1".into()),
        Some("root".into()),
        "draft-1",
        Some(1),
        &piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "stream…".into(),
        },
    )
    .into_iter()
    .next()
    .unwrap();
    assert!(matches!(
        tl.apply_stream_item(&patch),
        piko_client_core::ApplyOutcome::Applied
    ));
    // Ensure draft exists
    assert!(
        tl.items()
            .iter()
            .any(|i| matches!(i, TimelineItem::RealtimeDraft(_)))
    );

    let vm = derive_timeline(&state);
    let draft = vm
        .rows
        .iter()
        .find(|r| r.streaming && r.body.contains("stream"))
        .unwrap();
    assert!(draft.render_markdown);
}

#[test]
fn streaming_thinking_shows_text_with_live_flag() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s1".into()),
        Some("root".into()),
        "draft-thinking",
        Some(1),
        &piko_protocol::agent_runtime::RealtimeDelta::Thinking {
            content_index: 0,
            delta: "working through it".into(),
        },
    )
    .into_iter()
    .next()
    .unwrap();
    assert!(matches!(
        tl.apply_stream_item(&patch),
        piko_client_core::ApplyOutcome::Applied
    ));

    let vm = derive_timeline(&state);
    let draft = vm.rows.iter().find(|r| r.id == "draft-thinking").unwrap();
    assert!(draft.body.is_empty());
    assert_eq!(draft.thinking.as_deref(), Some("working through it"));
    assert!(draft.thinking_live);
    assert!(draft.streaming);
    assert!(draft.render_markdown);
}

#[test]
fn activity_show_stop_when_running() {
    let mut state = live_state();
    state
        .live_session
        .as_mut()
        .unwrap()
        .active_turns
        .push(ActiveTurn {
            turn_id: "t1".into(),
            agent_instance_id: "root".into(),
            status: TurnStatus::Running,
        });
    let vm = derive_activity(&state);
    assert!(vm.show_stop);
    assert!(vm.summary.contains("running"));
}

#[test]
fn composer_can_send_when_live_with_agent() {
    let vm = derive_composer(&live_state());
    assert!(vm.can_send);
    assert_eq!(vm.target_label, "Main");
    assert!(!vm.show_stop);
}

#[test]
fn composer_idle_without_session() {
    let vm = derive_composer(&ClientState::default());
    assert!(vm.can_send);
}
