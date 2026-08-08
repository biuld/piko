use super::*;

#[test]
fn timeline_rows_from_committed() {
    let vm = derive_timeline(&live_state());
    assert_eq!(vm.rows.len(), 1);
    assert_eq!(vm.rows[0].body, "hello");
    assert_eq!(vm.rows[0].kind, TimelineRowKind::User);
    assert!(!vm.rows[0].render_markdown);
    assert_eq!(vm.selected_agent_name.as_deref(), Some("Main"));
}

#[test]
fn committed_assistant_marks_markdown_path() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_committed(
        "msg-a".into(),
        2,
        piko_protocol::Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Text {
                text: "**bold** and `code`".into(),
            }],
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
    assert_eq!(row.kind, TimelineRowKind::Assistant);
    assert!(row.render_markdown);
    assert!(!row.streaming);
    assert!(row.body.contains("**bold**"));
}

#[test]
fn assistant_thinking_is_separate_payload_not_inlined_in_body() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_committed(
        "msg-thinking".into(),
        2,
        piko_protocol::Message::Assistant {
            content: vec![
                piko_protocol::ContentBlock::Thinking {
                    thinking: "consider alternatives".into(),
                    thinking_signature: None,
                },
                piko_protocol::ContentBlock::Text {
                    text: "final answer".into(),
                },
            ],
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
    let row = vm.rows.iter().find(|r| r.id == "msg-thinking").unwrap();
    assert_eq!(row.body, "final answer");
    assert_eq!(row.thinking.as_deref(), Some("consider alternatives"));
    assert!(!row.body.to_lowercase().contains("thinking"));
}
