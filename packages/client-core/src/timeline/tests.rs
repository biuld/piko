use super::*;
use piko_protocol::agent_runtime::RealtimeDelta;

#[test]
fn tool_arg_chunks_upsert_by_tool_call_id() {
    let mut tl = AgentTimeline::new();
    for (seq, chunk) in [(1u64, "{\"path\":"), (2, "\"a\"}")] {
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s".into()),
            Some("root".into()),
            "msg-1",
            Some(seq),
            &RealtimeDelta::ToolCall {
                content_index: 0,
                tool_call_id: "call-1".into(),
                delta: chunk.into(),
            },
        )
        .into_iter()
        .next()
        .unwrap();
        assert!(matches!(
            tl.apply_stream_item(&patch),
            ApplyOutcome::Applied
        ));
    }

    // ToolCall chunks also open a RealtimeDraft for message-level seq tracking.
    let tool = tl
        .items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::Tool(t) if t.tool_call_id == "call-1" => Some(t),
            _ => None,
        })
        .expect("expected tool item");
    assert_eq!(tool.partial_json.as_deref(), Some("{\"path\":\"a\"}"));
    assert_eq!(tool.status, ToolStatus::Running);
    assert_eq!(tool.parent_message_id.as_deref(), Some("msg-1"));
}

#[test]
fn tool_ended_clears_partial_json() {
    let mut tl = AgentTimeline::new();
    tl.apply_tool_arg_chunk("call-1".into(), "{", Some("msg".into()));
    tl.apply_tool_ended(
        "call-1".into(),
        "read".into(),
        serde_json::json!({"ok": true}),
        false,
    );
    let TimelineItem::Tool(tool) = &tl.items()[0] else {
        panic!("expected tool");
    };
    assert!(tool.partial_json.is_none());
    assert_eq!(tool.status, ToolStatus::Completed);
}

#[test]
fn stream_item_applies_text_chunk() {
    let mut tl = AgentTimeline::new();
    let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s1".into()),
        Some("root".into()),
        "msg-1",
        Some(1),
        &RealtimeDelta::Text {
            content_index: 0,
            delta: "hi".into(),
        },
    )
    .into_iter()
    .next()
    .unwrap();
    assert!(matches!(
        tl.apply_stream_item(&patch),
        ApplyOutcome::Applied
    ));
    // Same seq is ignored (idempotent re-delivery).
    assert!(matches!(
        tl.apply_stream_item(&patch),
        ApplyOutcome::Ignored
    ));
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.text_segments[0], "hi");
}

#[test]
fn stream_item_tool_upsert_starts_tool() {
    let mut tl = AgentTimeline::new();
    let patch = piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("root".into()),
        item_id: "call-1".into(),
        item_kind: piko_protocol::StreamItemKind::ToolCall,
        op: piko_protocol::StreamItemOp::Upsert,
        text: None,
        content_index: None,
        delta_seq: None,
        fields: Some(serde_json::json!({
            "toolName": "read",
            "args": {"path": "a"},
            "status": "running",
            "parentMessageId": "msg",
        })),
    };
    assert!(matches!(
        tl.apply_stream_item(&patch),
        ApplyOutcome::Applied
    ));
    let TimelineItem::Tool(tool) = &tl.items()[0] else {
        panic!("expected tool");
    };
    assert_eq!(tool.tool_name, "read");
    assert_eq!(tool.status, ToolStatus::Running);
}
