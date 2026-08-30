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
    assert_eq!(tool.args, serde_json::Value::String("{".into()));
}

#[test]
fn tool_ended_promotes_streamed_json_args() {
    let mut tl = AgentTimeline::new();
    tl.apply_tool_arg_chunk("call-1".into(), "{\"cmd\":\"ls\"}", Some("msg".into()));
    tl.apply_tool_ended(
        "call-1".into(),
        "exec_command".into(),
        serde_json::json!({"ok": true}),
        false,
    );
    let TimelineItem::Tool(tool) = &tl.items()[0] else {
        panic!("expected tool");
    };
    assert_eq!(tool.args, serde_json::json!({"cmd": "ls"}));
    assert!(tool.partial_json.is_none());
}

#[test]
fn committed_tool_call_keeps_exec_cmd_after_result() {
    let mut tl = AgentTimeline::new();
    let call = Message::ToolCall {
        id: "call-1".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({"cmd": "git status"}),
        model: None,
        provider: None,
        timestamp: None,
    };
    assert!(tl.apply_committed("msg-call".into(), 1, call, "turn".into()));
    let result = Message::ToolResult {
        tool_call_id: "call-1".into(),
        tool_name: Some("exec_command".into()),
        content: vec![piko_protocol::ContentBlock::Text {
            text: r#"{"exit_code":0,"output":"ok"}"#.into(),
        }],
        details: None,
        is_error: Some(false),
        timestamp: None,
    };
    assert!(tl.apply_committed("msg-result".into(), 2, result, "turn".into()));
    let TimelineItem::Tool(tool) = &tl.items()[0] else {
        panic!("expected tool");
    };
    assert_eq!(tool.tool_name, "exec_command");
    assert_eq!(tool.args, serde_json::json!({"cmd": "git status"}));
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
    assert_eq!(draft.text(), "hi");
}

#[test]
fn realtime_segments_coalesce_chunks_and_keep_first_seen_kind_order() {
    let mut tl = AgentTimeline::new();
    for (seq, delta) in [
        (
            0,
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ),
        (
            1,
            RealtimeDelta::Thinking {
                content_index: 0,
                delta: "thinking".into(),
            },
        ),
        (
            2,
            RealtimeDelta::Thinking {
                content_index: 0,
                delta: " now".into(),
            },
        ),
        (
            3,
            RealtimeDelta::Text {
                content_index: 0,
                delta: "hello".into(),
            },
        ),
        (
            4,
            RealtimeDelta::Text {
                content_index: 0,
                delta: " world".into(),
            },
        ),
    ] {
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s".into()),
            Some("root".into()),
            "msg",
            Some(seq),
            &delta,
        )
        .pop()
        .unwrap();
        assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    }

    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.content_segments.len(), 2);
    assert_eq!(
        draft.content_segments[0].kind,
        RealtimeContentKind::Thinking
    );
    assert_eq!(draft.content_segments[0].text, "thinking now");
    assert_eq!(draft.content_segments[1].kind, RealtimeContentKind::Text);
    assert_eq!(draft.content_segments[1].text, "hello world");
}

#[test]
fn realtime_thinking_lifecycle_closes_on_non_thinking_and_message_end() {
    let mut tl = AgentTimeline::new();
    let apply = |tl: &mut AgentTimeline, seq: u64, delta: RealtimeDelta| {
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s".into()),
            Some("root".into()),
            "msg",
            Some(seq),
            &delta,
        )
        .pop()
        .expect("realtime patch");
        assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    };

    apply(
        &mut tl,
        0,
        RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    );
    apply(
        &mut tl,
        1,
        RealtimeDelta::Thinking {
            content_index: 0,
            delta: "first".into(),
        },
    );
    apply(
        &mut tl,
        2,
        RealtimeDelta::Thinking {
            content_index: 0,
            delta: " thought".into(),
        },
    );
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.active_thinking_index, Some(0));

    apply(
        &mut tl,
        3,
        RealtimeDelta::Text {
            content_index: 0,
            delta: "answer".into(),
        },
    );
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.active_thinking_index, None);

    apply(
        &mut tl,
        4,
        RealtimeDelta::Thinking {
            content_index: 1,
            delta: "second".into(),
        },
    );
    apply(
        &mut tl,
        5,
        RealtimeDelta::MessageEnded {
            stop_reason: Some("stop".into()),
            error_message: None,
        },
    );

    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert!(draft.ended);
    assert_eq!(draft.active_thinking_index, None);
    assert_eq!(
        draft
            .content_segments
            .iter()
            .filter(|segment| segment.kind == RealtimeContentKind::Thinking)
            .map(|segment| segment.content_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn reliable_model_step_boundary_closes_thought_without_message_end() {
    let mut tl = AgentTimeline::new();
    let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s".into()),
        Some("root".into()),
        "assistant-1",
        Some(0),
        &RealtimeDelta::Thinking {
            content_index: 0,
            delta: "still thinking".into(),
        },
    )
    .pop()
    .unwrap();
    assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    let boundary = piko_protocol::ModelStepBoundary {
        session_id: "s".into(),
        root_input_id: "input-1".into(),
        agent_instance_id: "root".into(),
        model_step_id: "step-1".into(),
        step_index: 1,
        started_at: 10,
        finished_at: 42,
        outcome: piko_protocol::ModelStepOutcome::Completed,
        assistant_message_id: "assistant-1".into(),
        tool_call_message_ids: Vec::new(),
    };
    assert_eq!(
        tl.apply_model_step_committed(boundary.clone()),
        ApplyOutcome::Applied
    );
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.active_thinking_index, None);
    assert_eq!(tl.model_steps().len(), 1);
    assert_eq!(
        tl.apply_model_step_committed(boundary.clone()),
        ApplyOutcome::Ignored
    );
    let mut conflicting = boundary;
    conflicting.finished_at = 43;
    assert_eq!(
        tl.apply_model_step_committed(conflicting),
        ApplyOutcome::Inconsistent
    );
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

#[test]
fn tool_upsert_closes_the_active_thinking_segment() {
    let mut tl = AgentTimeline::new();
    let thinking = piko_protocol::StreamItemPatch::from_realtime_delta(
        Some("s".into()),
        Some("root".into()),
        "msg",
        Some(1),
        &RealtimeDelta::Thinking {
            content_index: 0,
            delta: "thinking".into(),
        },
    )
    .pop()
    .expect("thinking patch");
    assert_eq!(tl.apply_stream_item(&thinking), ApplyOutcome::Applied);

    let tool = piko_protocol::StreamItemPatch {
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
            "args": {},
            "status": "running",
            "parentMessageId": "msg",
        })),
    };
    assert_eq!(tl.apply_stream_item(&tool), ApplyOutcome::Applied);
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.active_thinking_index, None);
}

#[test]
fn replace_and_clear_message_content_are_applied_by_segment() {
    let mut tl = AgentTimeline::new();
    for (seq, op, text, index) in [
        (
            1,
            piko_protocol::StreamItemOp::AppendChunk,
            Some("draft"),
            Some(0),
        ),
        (
            2,
            piko_protocol::StreamItemOp::AppendChunk,
            Some("second"),
            Some(1),
        ),
        (
            3,
            piko_protocol::StreamItemOp::ReplaceContent,
            Some("correct"),
            Some(0),
        ),
        (4, piko_protocol::StreamItemOp::ClearContent, None, None),
    ] {
        let patch = piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("root".into()),
            item_id: "msg".into(),
            item_kind: piko_protocol::StreamItemKind::AgentMessage,
            op,
            text: text.map(str::to_string),
            content_index: index,
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({"parentMessageId": "msg"})),
        };
        assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    }
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.text(), "");
}

#[test]
fn replace_and_clear_tool_argument_content_are_applied() {
    let mut tl = AgentTimeline::new();
    for (seq, op, text, index) in [
        (
            1,
            piko_protocol::StreamItemOp::AppendChunk,
            Some("bad"),
            Some(0),
        ),
        (
            2,
            piko_protocol::StreamItemOp::ReplaceContent,
            Some("{\"ok\":true}"),
            Some(0),
        ),
        (3, piko_protocol::StreamItemOp::ClearContent, None, None),
    ] {
        let patch = piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("root".into()),
            item_id: "call".into(),
            item_kind: piko_protocol::StreamItemKind::ToolCall,
            op,
            text: text.map(str::to_string),
            content_index: index,
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({
                "parentMessageId": "msg",
                "rootInputId": "turn-1"
            })),
        };
        assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    }
    let tool = tl
        .items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::Tool(tool) => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.partial_json.as_deref(), Some(""));
    assert!(tool.argument_segments.is_empty());
}

#[test]
fn full_text_upsert_replaces_all_prior_chunks() {
    let mut tl = AgentTimeline::new();
    for (seq, op, text, index) in [
        (1, piko_protocol::StreamItemOp::AppendChunk, "a", 0),
        (2, piko_protocol::StreamItemOp::AppendChunk, "b", 1),
        (3, piko_protocol::StreamItemOp::Upsert, "final", 0),
    ] {
        let patch = piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("root".into()),
            item_id: "msg".into(),
            item_kind: piko_protocol::StreamItemKind::AgentMessage,
            op,
            text: Some(text.into()),
            content_index: Some(index),
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({"parentMessageId": "msg"})),
        };
        assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Applied);
    }
    let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
        panic!("expected draft");
    };
    assert_eq!(draft.text(), "final");
}

#[test]
fn committed_message_rejects_late_content_correction() {
    let mut tl = AgentTimeline::new();
    tl.apply_committed(
        "msg".into(),
        1,
        piko_protocol::Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Text {
                text: "final".into(),
            }],
            checkpoint: None,
            provider: "test".into(),
            model: "test".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        },
        "turn-1".into(),
    );
    let patch = piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("root".into()),
        item_id: "msg".into(),
        item_kind: piko_protocol::StreamItemKind::AgentMessage,
        op: piko_protocol::StreamItemOp::ReplaceContent,
        text: Some("stale".into()),
        content_index: Some(0),
        delta_seq: Some(2),
        fields: None,
    };
    assert_eq!(tl.apply_stream_item(&patch), ApplyOutcome::Ignored);
}

#[test]
fn mixed_session_facts_keep_branch_position_when_commits_reorder() {
    let mut tl = AgentTimeline::new();
    tl.apply_session_entry(
        piko_protocol::SessionTreeEntry::ModelChange(piko_protocol::ModelChangeEntry {
            id: "model-change".into(),
            parent_id: None,
            timestamp: "1".into(),
            provider: "openai".into(),
            model_id: "gpt".into(),
        }),
        0,
    );
    for (id, seq, text) in [("assistant", 2, "answer"), ("user", 1, "question")] {
        let message = if id == "user" {
            piko_protocol::Message::User {
                content: piko_protocol::MessageContent::String(text.into()),
                timestamp: None,
            }
        } else {
            piko_protocol::Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text { text: text.into() }],
                checkpoint: None,
                provider: "test".into(),
                model: "test".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }
        };
        tl.apply_committed(id.into(), seq, message, "turn".into());
    }
    assert!(matches!(tl.items()[0], TimelineItem::SessionEntry(_)));
    assert!(matches!(
        &tl.items()[1],
        TimelineItem::Committed(item) if item.message_id == "user"
    ));
    assert!(matches!(
        &tl.items()[2],
        TimelineItem::Committed(item) if item.message_id == "assistant"
    ));
}
