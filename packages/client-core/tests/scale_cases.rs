//! Large transcript / rapid realtime delta scale harness (no GPUI).

mod helpers;

use helpers::*;
use piko_client_core::TimelineItem;
use piko_protocol::ServerMessage;
use piko_protocol::agent_runtime::RealtimeDelta;

const COMMITTED_N: usize = 220;
const RAPID_DELTAS: u64 = 80;

#[test]
fn scale_many_committed_messages() {
    let mut ids = SeqIds(0);
    let mut state = drive_to_live(&mut ids, "sess-1");

    for i in 0..COMMITTED_N {
        let (next, _) = host(
            state,
            ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
                session_id: "sess-1".into(),
                agent_instance_id: "root".into(),
                agent_id: "main".into(),
                source_turn_id: format!("turn-{i}"),
                message_id: format!("msg-{i}"),
                transcript_seq: (i as u64) + 1,
                message: piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String(format!("user line {i}")),
                    timestamp: Some(i as i64),
                },
            }),
            &mut ids,
        );
        state = next;
    }

    let tl = state
        .live_session
        .as_ref()
        .unwrap()
        .timelines
        .get("root")
        .unwrap();
    assert_eq!(tl.committed_count(), COMMITTED_N);
    assert_eq!(tl.draft_count(), 0);
    assert_eq!(tl.items().len(), COMMITTED_N);
}

#[test]
fn scale_rapid_realtime_then_commit() {
    let mut ids = SeqIds(0);
    let mut state = drive_to_live(&mut ids, "sess-1");

    for seq in 1..=RAPID_DELTAS {
        let (next, _) = host(
            state,
            ServerMessage::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
                session_id: "sess-1".into(),
                agent_instance_id: "root".into(),
                agent_id: "main".into(),
                message_id: "stream-1".into(),
                delta_seq: seq,
                delta: RealtimeDelta::Text {
                    content_index: 0,
                    delta: "x".into(),
                },
            }),
            &mut ids,
        );
        state = next;
    }

    {
        let tl = state
            .live_session
            .as_ref()
            .unwrap()
            .timelines
            .get("root")
            .unwrap();
        assert_eq!(tl.draft_count(), 1);
        let draft = tl.items().iter().find_map(|i| match i {
            TimelineItem::RealtimeDraft(d) => Some(d),
            _ => None,
        });
        let draft = draft.expect("draft");
        assert_eq!(draft.last_delta_seq, RAPID_DELTAS);
        assert_eq!(draft.text_segments.join("").len(), RAPID_DELTAS as usize);
    }

    let (state, _) = host(
        state,
        ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-s".into(),
            message_id: "stream-1".into(),
            transcript_seq: 1,
            message: piko_protocol::Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "x".repeat(RAPID_DELTAS as usize),
                }],
                api: "api".into(),
                provider: "test".into(),
                model: "m".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: Some(1),
            },
        }),
        &mut ids,
    );

    let tl = state
        .live_session
        .as_ref()
        .unwrap()
        .timelines
        .get("root")
        .unwrap();
    assert_eq!(tl.committed_count(), 1);
    assert_eq!(tl.draft_count(), 0);
}
