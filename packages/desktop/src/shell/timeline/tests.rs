//! Timeline mapping and virtualized-frame tests (F-45/F-46).

use super::*;
use piko_client_core::state::PendingOp;
use piko_client_core::timeline::AgentTimeline;

fn state_with(timeline: AgentTimeline) -> ClientState {
    let mut core = ClientState::default();
    core.session_phase = SessionPhase::Live;
    let mut session = piko_client_core::LiveSession {
        selected_agent: Some("agent-1".to_string()),
        ..piko_client_core::LiveSession::default()
    };
    session.timelines.insert("agent-1".to_string(), timeline);
    core.live_session = Some(session);
    core
}

fn user_msg(text: &str) -> Message {
    Message::User {
        content: MessageContent::String(text.to_string()),
        timestamp: None,
    }
}

fn assistant_msg(blocks: Vec<ContentBlock>) -> Message {
    Message::Assistant {
        content: blocks,
        checkpoint: None,
        provider: "p".to_string(),
        model: "m".to_string(),
        usage: None,
        stop_reason: None,
        error_message: None,
        timestamp: None,
    }
}

fn thinking_block(text: &str) -> ContentBlock {
    ContentBlock::Thinking {
        thinking: text.to_string(),
        thinking_signature: None,
        duration_ms: None,
    }
}

fn text_block(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
    }
}

#[test]
fn no_session_open_failure_and_hydration_states() {
    assert_eq!(
        frame_timeline(&ClientState::default(), 0.).0,
        TimelineState::NoSession
    );
    let mut core = ClientState::default();
    core.command_failures
        .push(piko_client_core::state::CommandFailure {
            command_id: "desktop-1".to_string(),
            operation: PendingOp::Open {
                session_id: "s1".to_string(),
            },
            message: "session journal unreadable".to_string(),
        });
    assert_eq!(
        frame_timeline(&core, 0.).0,
        TimelineState::Error("session journal unreadable".to_string())
    );
    let mut hydrating = ClientState::default();
    hydrating.session_phase = SessionPhase::Hydrating {
        target_id: "s1".to_string(),
    };
    assert_eq!(frame_timeline(&hydrating, 0.).0, TimelineState::Loading);
}

#[test]
fn empty_when_live_without_items() {
    let (state, frame) = frame_timeline(&state_with(AgentTimeline::new()), 0.);
    assert_eq!(state, TimelineState::Empty);
    assert_eq!(frame.total(), 0);
}

#[test]
fn assistant_side_run_collapses_into_one_row() {
    let mut timeline = AgentTimeline::new();
    timeline.apply_committed("u1".into(), 1, user_msg("hello"), "t".into());
    timeline.apply_committed(
        "a1".into(),
        2,
        assistant_msg(vec![thinking_block("hmm"), text_block("part one")]),
        "t".into(),
    );
    timeline.apply_committed(
        "call-1".into(),
        3,
        Message::ToolCall {
            id: "call-1".into(),
            name: "exec_command".into(),
            arguments: serde_json::json!({"cmd": "git status"}),
            model: None,
            provider: None,
            timestamp: None,
        },
        "t".into(),
    );
    timeline.apply_committed(
        "a2".into(),
        4,
        assistant_msg(vec![text_block("continuing")]),
        "t".into(),
    );

    let core = state_with(timeline);
    let (_, frame) = frame_timeline(&core, 12.);
    // [user] then the turn splits at text boundaries: [Think, Text] and
    // [Tool, Text] pack into one bubble via shared turn_id.
    assert_eq!(frame.total(), 3);
    let (prev, piece0) = rows_around(&core, &frame, 1).unwrap();
    assert_eq!(
        prev.as_ref().map(|row| row.id().to_string()).as_deref(),
        Some("u1-user")
    );
    let TimelineRow::Assistant {
        turn_id,
        leads_turn,
        segments,
        ..
    } = &piece0
    else {
        panic!("expected assistant flow");
    };
    assert_eq!(turn_id, "a1-turn");
    assert!(*leads_turn);
    assert_eq!(segments.len(), 2);
    assert!(matches!(&segments[0], TurnSegment::Thinking { text, .. } if text == "hmm"));
    assert!(matches!(&segments[1], TurnSegment::Text { text, .. } if text == "part one"));

    let (prev, piece1) = rows_around(&core, &frame, 2).unwrap();
    assert_eq!(prev.unwrap().id(), piece0.id());
    let TimelineRow::Assistant {
        leads_turn,
        segments,
        ..
    } = &piece1
    else {
        panic!("expected assistant flow");
    };
    assert!(!*leads_turn);
    assert_eq!(segments.len(), 2);
    assert!(matches!(&segments[0], TurnSegment::Tool { name, .. } if name == "exec_command"));
    assert!(matches!(&segments[1], TurnSegment::Text { text, .. } if text == "continuing"));
}

#[test]
fn session_entry_breaks_the_run() {
    let mut timeline = AgentTimeline::new();
    timeline.apply_committed(
        "a1".into(),
        1,
        assistant_msg(vec![text_block("one")]),
        "t".into(),
    );
    timeline.apply_session_entry(
        piko_protocol::session::SessionTreeEntry::ModelChange(
            piko_protocol::session::ModelChangeEntry {
                id: "mc-1".into(),
                parent_id: None,
                timestamp: "2026-08-23T00:00:00Z".into(),
                provider: "p".into(),
                model_id: "m".into(),
            },
        ),
        1,
    );
    timeline.apply_committed(
        "a2".into(),
        2,
        assistant_msg(vec![text_block("two")]),
        "t".into(),
    );
    let core = state_with(timeline);
    let (_, frame) = frame_timeline(&core, 0.);
    assert_eq!(frame.total(), 3);
    assert!(matches!(
        rows_around(&core, &frame, 1).unwrap().1,
        TimelineRow::System { .. }
    ));
}

#[test]
fn adjacent_same_kind_merges_within_a_message_only() {
    let mut timeline = AgentTimeline::new();
    timeline.apply_committed(
        "a1".into(),
        1,
        assistant_msg(vec![text_block("one"), text_block("two")]),
        "t".into(),
    );
    timeline.apply_committed(
        "a2".into(),
        2,
        assistant_msg(vec![text_block("three")]),
        "t".into(),
    );
    let core = state_with(timeline);
    let (_, frame) = frame_timeline(&core, 0.);
    // One turn, two pieces: each merged text block owns a row; both carry
    // the same turn_id so they pack with zero gap.
    assert_eq!(frame.total(), 2);
    let first = rows_around(&core, &frame, 0).unwrap().1;
    let second = rows_around(&core, &frame, 1).unwrap().1;
    for (index, row) in [&first, &second].into_iter().enumerate() {
        assert!(
            matches!(row, TimelineRow::Assistant { .. }),
            "piece {index}"
        );
    }
    match (&first, &second) {
        (
            TimelineRow::Assistant {
                segments: a,
                turn_id: ta,
                ..
            },
            TimelineRow::Assistant {
                segments: b,
                turn_id: tb,
                ..
            },
        ) => {
            assert_eq!(ta, tb);
            assert_eq!(a.len(), 1);
            assert!(matches!(&a[0], TurnSegment::Text { text, .. } if text == "one\ntwo"));
            assert!(matches!(&b[0], TurnSegment::Text { text, .. } if text == "three"));
        }
        _ => unreachable!(),
    }
}

#[test]
fn live_draft_tail_drives_spinner_and_caret_only_at_run_end() {
    fn chunk(
        seq: u64,
        kind: piko_protocol::StreamItemKind,
        index: u32,
        text: &str,
    ) -> piko_protocol::StreamItemPatch {
        piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("agent-1".into()),
            item_id: "d1".into(),
            item_kind: kind,
            op: piko_protocol::StreamItemOp::AppendChunk,
            text: Some(text.into()),
            content_index: Some(index),
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({"parentMessageId": "d1"})),
        }
    }

    let mut timeline = AgentTimeline::new();
    timeline.apply_stream_item(&chunk(
        1,
        piko_protocol::StreamItemKind::AgentThought,
        0,
        "hmm",
    ));
    timeline.apply_stream_item(&chunk(
        2,
        piko_protocol::StreamItemKind::AgentMessage,
        1,
        "partial",
    ));
    let core = state_with(timeline);
    let (_, frame) = frame_timeline(&core, 0.);
    assert!(frame.streaming);
    // Draft tail is a Text segment: caret lights, thinking stays settled.
    let TimelineRow::Assistant { segments, .. } = rows_around(&core, &frame, 0).unwrap().1 else {
        panic!("expected assistant flow");
    };
    assert!(matches!(
        &segments[0],
        TurnSegment::Thinking { active: false, .. }
    ));
    assert!(matches!(
        &segments[1],
        TurnSegment::Text { caret: true, .. }
    ));
}

#[test]
fn thinking_only_draft_tail_spins_until_more_content() {
    let chunk = |seq: u64| piko_protocol::StreamItemPatch {
        session_id: Some("s".into()),
        agent_instance_id: Some("agent-1".into()),
        item_id: "d1".into(),
        item_kind: piko_protocol::StreamItemKind::AgentThought,
        op: piko_protocol::StreamItemOp::AppendChunk,
        text: Some("hmm".into()),
        content_index: Some(0),
        delta_seq: Some(seq),
        fields: Some(serde_json::json!({"parentMessageId": "d1"})),
    };
    let mut timeline = AgentTimeline::new();
    timeline.apply_stream_item(&chunk(1));
    let core = state_with(timeline);
    let TimelineRow::Assistant { segments, .. } =
        rows_around(&core, &(frame_timeline(&core, 0.).1), 0)
            .unwrap()
            .1
    else {
        panic!("expected assistant flow");
    };
    assert!(matches!(
        &segments[0],
        TurnSegment::Thinking { active: true, .. }
    ));
}

#[test]
fn running_tool_sorts_before_live_drafts_and_suppresses_tail_spinner() {
    let chunk = |seq: u64, kind: piko_protocol::StreamItemKind, index: u32, text: &str| {
        piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("agent-1".into()),
            item_id: "d1".into(),
            item_kind: kind,
            op: piko_protocol::StreamItemOp::AppendChunk,
            text: Some(text.into()),
            content_index: Some(index),
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({"parentMessageId": "d1"})),
        }
    };
    let mut timeline = AgentTimeline::new();
    timeline.apply_stream_item(&chunk(
        1,
        piko_protocol::StreamItemKind::AgentThought,
        0,
        "hmm",
    ));
    timeline.apply_stream_item(&chunk(
        2,
        piko_protocol::StreamItemKind::AgentMessage,
        1,
        "partial",
    ));
    // A tool call committed mid-run is a durable fact: it sorts before the
    // same run's still-streaming drafts, and the thinking tail no longer
    // carries the spinner (the tool chip is running, the text owns the caret).
    timeline.apply_committed(
        "call-9".into(),
        1,
        Message::ToolCall {
            id: "call-9".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "a.rs"}),
            model: None,
            provider: None,
            timestamp: None,
        },
        "t".into(),
    );
    let core = state_with(timeline);
    let frame = frame_timeline(&core, 0.).1;
    let TimelineRow::Assistant { segments, .. } = rows_around(&core, &frame, 0).unwrap().1 else {
        panic!("expected assistant flow");
    };
    assert!(
        matches!(&segments[0], TurnSegment::Tool { id, .. } if id == "call-9"),
        "the committed tool renders before the live drafts: {segments:?}"
    );
    assert!(matches!(
        &segments[1],
        TurnSegment::Thinking { active: false, .. }
    ));
    assert!(matches!(
        &segments[2],
        TurnSegment::Text { caret: true, .. }
    ));
}

#[test]
fn tool_first_group_renders_chips_only_row() {
    let mut timeline = AgentTimeline::new();
    timeline.apply_committed(
        "call-1".into(),
        1,
        Message::ToolCall {
            id: "call-1".into(),
            name: "read".into(),
            arguments: serde_json::json!({}),
            model: None,
            provider: None,
            timestamp: None,
        },
        "t".into(),
    );
    let core = state_with(timeline);
    let (_, frame) = frame_timeline(&core, 0.);
    assert_eq!(frame.total(), 1);
    match rows_around(&core, &frame, 0).unwrap().1 {
        TimelineRow::Assistant { segments, .. } => {
            assert!(matches!(&segments[..], [TurnSegment::Tool { .. }]));
        }
        other => panic!("unexpected row {other:?}"),
    }
}
