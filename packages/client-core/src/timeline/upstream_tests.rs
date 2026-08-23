use super::*;

#[test]
fn upstream_stream_item_upserts_one_card_with_args() {
    let mut tl = AgentTimeline::new();

    // in_progress first; no args yet.
    let running = piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("root".into()),
        item_id: "ws_1".into(),
        item_kind: piko_protocol::StreamItemKind::Upstream,
        op: piko_protocol::StreamItemOp::Upsert,
        text: None,
        content_index: None,
        delta_seq: Some(1),
        fields: Some(serde_json::json!({
            "status": "running",
            "toolName": "web_search",
            "kind": "search",
            "args": serde_json::Value::Null,
            "parentMessageId": "msg-1",
        })),
    };
    assert!(matches!(
        tl.apply_stream_item(&running),
        ApplyOutcome::Applied
    ));

    let tool = find_tool(&tl, "ws_1");
    assert_eq!(tool.status, ToolStatus::Running);
    assert!(tool.upstream.is_some());
    assert_eq!(tool.upstream.as_ref().unwrap().kind, "search");

    // completed with the same activity_id updates the same card + args.
    let done = piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("root".into()),
        item_id: "ws_1".into(),
        item_kind: piko_protocol::StreamItemKind::Upstream,
        op: piko_protocol::StreamItemOp::Upsert,
        text: None,
        content_index: None,
        delta_seq: Some(2),
        fields: Some(serde_json::json!({
            "status": "completed",
            "toolName": "web_search",
            "kind": "search",
            "args": { "type": "search", "query": "深圳天气" },
            "action": { "type": "search", "queries": ["深圳天气"] },
            "parentMessageId": "msg-1",
        })),
    };
    assert!(matches!(tl.apply_stream_item(&done), ApplyOutcome::Applied));

    // Exactly one tool card.
    let tools: Vec<_> = tl
        .items()
        .iter()
        .filter_map(|item| match item {
            TimelineItem::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 1);
    let tool = find_tool(&tl, "ws_1");
    assert_eq!(tool.status, ToolStatus::Completed);
    assert_eq!(
        tool.upstream.as_ref().unwrap().action,
        Some(piko_protocol::messages::UpstreamAction::Search {
            queries: vec!["深圳天气".into()]
        })
    );
}

#[test]
fn committed_assistant_upstream_blocks_become_one_tool_item() {
    let mut tl = AgentTimeline::new();
    let message = Message::Assistant {
        content: vec![
            ContentBlock::Text {
                text: "searching".into(),
            },
            ContentBlock::UpstreamToolActivity {
                activity_id: "ws_1".into(),
                tool_name: "web_search".into(),
                kind: "search".into(),
                status: piko_protocol::messages::UpstreamActivityStatus::InProgress,
                arguments: None,
                action: None,
            },
            ContentBlock::UpstreamToolActivity {
                activity_id: "ws_1".into(),
                tool_name: "web_search".into(),
                kind: "search".into(),
                status: piko_protocol::messages::UpstreamActivityStatus::Completed,
                arguments: Some(serde_json::json!({ "query": "深圳天气" })),
                action: Some(piko_protocol::messages::UpstreamAction::Search {
                    queries: vec!["深圳天气".into()],
                }),
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

    assert!(tl.apply_committed("assistant-1".into(), 7, message, "turn-1".into()));

    let tools: Vec<_> = tl
        .items()
        .iter()
        .filter_map(|item| match item {
            TimelineItem::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 1, "upstream blocks collapse to one card");
    let tool = find_tool(&tl, "ws_1");
    assert_eq!(tool.status, ToolStatus::Completed);
    assert_eq!(tool.transcript_seq, Some(7));
    assert_eq!(
        tool.args.get("query").and_then(|q| q.as_str()),
        Some("深圳天气")
    );
    assert_eq!(
        tool.upstream.as_ref().unwrap().action,
        Some(piko_protocol::messages::UpstreamAction::Search {
            queries: vec!["深圳天气".into()]
        })
    );
    assert!(tool.result.is_none());
}

#[test]
fn live_upstream_start_captures_before_snapshot() {
    let mut tl = AgentTimeline::new();

    // Stream the "before" text draft.
    let started = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s".into()),
        Some("root".into()),
        "msg-1",
        Some(0),
        &RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    )
    .pop()
    .unwrap();
    assert!(matches!(
        tl.apply_stream_item(&started),
        ApplyOutcome::Applied
    ));
    let text = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s".into()),
        Some("root".into()),
        "msg-1",
        Some(1),
        &RealtimeDelta::Text {
            content_index: 0,
            delta: "searching".into(),
        },
    )
    .pop()
    .unwrap();
    assert!(matches!(tl.apply_stream_item(&text), ApplyOutcome::Applied));

    // Upstream tool starts → capture the before snapshot.
    let running = piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("root".into()),
        item_id: "ws_1".into(),
        item_kind: piko_protocol::StreamItemKind::Upstream,
        op: piko_protocol::StreamItemOp::Upsert,
        text: None,
        content_index: None,
        delta_seq: Some(2),
        fields: Some(serde_json::json!({
            "status": "running",
            "toolName": "web_search",
            "kind": "search",
            "args": serde_json::Value::Null,
            "parentMessageId": "msg-1",
        })),
    };
    assert!(matches!(
        tl.apply_stream_item(&running),
        ApplyOutcome::Applied
    ));

    let tool = find_tool(&tl, "ws_1");
    let split = tool.upstream_split.as_ref().expect("before snapshot");
    assert_eq!(split.before_text, "searching");
    assert_eq!(split.before_thinking, "");

    // A later "running" update must not overwrite the snapshot.
    let running2 = piko_protocol::StreamItemPatch { ..running.clone() };
    assert!(matches!(
        tl.apply_stream_item(&running2),
        ApplyOutcome::Applied
    ));
    let tool = find_tool(&tl, "ws_1");
    assert_eq!(
        tool.upstream_split.as_ref().unwrap().before_text,
        "searching"
    );
}

fn find_tool<'a>(tl: &'a AgentTimeline, id: &str) -> &'a ToolItem {
    tl.items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::Tool(tool) if tool.tool_call_id == id => Some(tool),
            _ => None,
        })
        .expect("tool item")
}
