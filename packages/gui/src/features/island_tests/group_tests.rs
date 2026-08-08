use super::*;

#[test]
fn visual_sender_maps_tool_to_assistant() {
    assert_eq!(
        TimelineRowKind::Tool.visual_sender(),
        VisualSender::Assistant
    );
    assert_eq!(TimelineRowKind::User.visual_sender(), VisualSender::You);
    assert_eq!(
        TimelineRowKind::System.visual_sender(),
        VisualSender::System
    );
}

#[test]
fn group_timeline_keeps_assistant_and_tool_together() {
    let rows = vec![
        sample_row("u1", TimelineRowKind::User),
        sample_row("a1", TimelineRowKind::Assistant),
        sample_row("t1", TimelineRowKind::Tool),
        sample_row("t2", TimelineRowKind::Tool),
    ];
    let groups = group_timeline_rows(&rows);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].len(), 1);
    assert_eq!(groups[1].len(), 3);
    assert_eq!(groups[1][0].kind, TimelineRowKind::Assistant);
}

#[test]
fn group_timeline_system_breaks_assistant_grouping() {
    let rows = vec![
        sample_row("a1", TimelineRowKind::Assistant),
        sample_row("s1", TimelineRowKind::System),
        sample_row("a2", TimelineRowKind::Assistant),
        sample_row("t1", TimelineRowKind::Tool),
    ];
    let groups = group_timeline_rows(&rows);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].len(), 1);
    assert_eq!(groups[1][0].kind, TimelineRowKind::System);
    assert_eq!(groups[2].len(), 2);
}

#[test]
fn consecutive_users_share_one_group() {
    let rows = vec![
        sample_row("u1", TimelineRowKind::User),
        sample_row("u2", TimelineRowKind::User),
        sample_row("a1", TimelineRowKind::Assistant),
    ];
    let groups = group_timeline_rows(&rows);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].len(), 2);
    assert_eq!(groups[0][0].kind.visual_sender(), VisualSender::You);
}

#[test]
fn user_and_assistant_labels_are_chat_senders() {
    crate::i18n::init();
    let vm = derive_timeline(&live_state());
    assert_eq!(vm.rows[0].label, "You");

    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_committed(
        "msg-a".into(),
        2,
        piko_protocol::Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Text { text: "hi".into() }],
            api: "chat".into(),
            provider: "test".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(2),
        },
        "turn-1".into(),
    );
    let vm = derive_timeline(&state);
    let row = vm.rows.iter().find(|r| r.id == "msg-a").unwrap();
    assert_eq!(row.label, "Assistant");
}
