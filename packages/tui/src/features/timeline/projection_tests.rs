use super::*;

#[test]
fn assistant_bubble_strips_upstream_blocks() {
    let message = piko_protocol::Message::Assistant {
        content: vec![
            piko_protocol::ContentBlock::Text {
                text: "checking".into(),
            },
            piko_protocol::ContentBlock::UpstreamToolActivity {
                activity_id: "act-1".into(),
                tool_name: "web_search".into(),
                kind: "search".into(),
                status: piko_protocol::messages::UpstreamActivityStatus::InProgress,
                arguments: Some(serde_json::json!({ "type": "search", "query": "深圳天气" })),
                action: Some(piko_protocol::messages::UpstreamAction::Search {
                    queries: vec!["深圳天气".into()],
                }),
            },
            piko_protocol::ContentBlock::UpstreamToolApproval {
                approval_id: "appr-1".into(),
                tool_name: "web_search".into(),
                summary: "needs consent".into(),
            },
        ],
        checkpoint: None,
        provider: "test".into(),
        model: "test".into(),
        usage: None,
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: None,
    };

    let components =
        components_from_message("a-1".into(), &message, &HashMap::new(), &HashMap::new());

    assert_eq!(components.len(), 1, "no tool cards from TUI");
    let TimelineComponent::Assistant(assistant) = &components[0] else {
        panic!("expected an assistant bubble");
    };
    assert_eq!(assistant.blocks.len(), 1);
    assert!(matches!(&assistant.blocks[0], ContentBlock::Text(t) if t == "checking"));
}

#[test]
fn upstream_cards_interleave_with_text_runs() {
    let message = piko_protocol::Message::Assistant {
        content: vec![
            piko_protocol::ContentBlock::Text {
                text: "before".into(),
            },
            piko_protocol::ContentBlock::UpstreamToolActivity {
                activity_id: "ws_1".into(),
                tool_name: "web_search".into(),
                kind: "search".into(),
                status: piko_protocol::messages::UpstreamActivityStatus::Completed,
                arguments: Some(serde_json::json!({ "query": "深圳天气" })),
                action: Some(piko_protocol::messages::UpstreamAction::Search {
                    queries: vec!["深圳天气".into()],
                }),
            },
            piko_protocol::ContentBlock::Text {
                text: "after".into(),
            },
        ],
        checkpoint: None,
        provider: "test".into(),
        model: "test".into(),
        usage: None,
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: None,
    };
    let mut card = ToolEntry::new(
        "ws_1".into(),
        "web_search".into(),
        crate::app::ToolStatus::Completed,
        r#"{"query":"深圳天气"}"#.into(),
        None,
        None,
    );
    card.upstream = Some(Box::new(UpstreamInfo {
        kind: "search".into(),
        summary: None,
        action: None,
    }));
    let mut upstream_tools = HashMap::new();
    upstream_tools.insert("ws_1".into(), card);

    let components =
        components_from_message("a-1".into(), &message, &HashMap::new(), &upstream_tools);
    assert_eq!(components.len(), 3, "text → card → text");
    assert!(matches!(components[0], TimelineComponent::Assistant(_)));
    assert!(matches!(components[1], TimelineComponent::Tool(_)));
    assert!(matches!(components[2], TimelineComponent::Assistant(_)));
    if let TimelineComponent::Assistant(trailing) = &components[2] {
        assert!(matches!(&trailing.blocks[0], ContentBlock::Text(t) if t == "after"));
    }
}

#[test]
fn live_draft_splits_around_upstream_card() {
    use piko_client_core::RealtimeContentKind;
    use piko_client_core::RealtimeContentSegment;

    let draft = piko_client_core::RealtimeDraft {
        message_id: "msg-1".into(),
        last_delta_seq: 3,
        content_segments: vec![
            RealtimeContentSegment {
                kind: RealtimeContentKind::Thinking,
                content_index: 0,
                text: "think".into(),
            },
            RealtimeContentSegment {
                kind: RealtimeContentKind::Text,
                content_index: 0,
                text: "beforeafter".into(),
            },
        ],
        live_order: 0,
        active_thinking_index: None,
        ended: false,
        stop_reason: None,
        error_message: None,
    };
    let card = ToolEntry::new(
        "ws_1".into(),
        "web_search".into(),
        crate::app::ToolStatus::Completed,
        r#"{"query":"深圳天气"}"#.into(),
        None,
        Some("msg-1".into()),
    );
    let slice = DraftSlice {
        tool: card,
        text_before: "before".chars().count(),
        thinking_before: "think".chars().count(),
    };
    let components = components_from_draft(&draft, &[slice]);
    assert_eq!(components.len(), 4, "thought → text → card → text");
    assert!(matches!(&components[0], TimelineComponent::Thought(thought)
        if thought.text == "think"));
    if let TimelineComponent::Assistant(before) = &components[1] {
        assert_eq!(before.blocks.len(), 1);
        assert!(matches!(&before.blocks[0], ContentBlock::Text(t) if t == "before"));
    } else {
        panic!("expected before bubble");
    }
    assert!(matches!(&components[2], TimelineComponent::Tool(t) if t.id == "ws_1"));
    if let TimelineComponent::Assistant(after) = &components[3] {
        assert!(matches!(&after.blocks[0], ContentBlock::Text(t) if t == "after"));
    } else {
        panic!("expected after bubble");
    }
}
