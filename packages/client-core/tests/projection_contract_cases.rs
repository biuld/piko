mod helpers;

use helpers::*;
use piko_client_core::{ClientIntent, SessionPhase, TimelineItem};
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{
    AgentViewSnapshot, Command, CommandResult, ReconcileReason, SequencedServerMessage,
    ServerMessage, SessionClearedEvent,
};

// C4 — Refresh
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c4_refresh_session() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, effects) = intent(state, ClientIntent::RefreshSession, &mut ids);

    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::StateSnapshot { session_id, .. } => assert_eq!(session_id, "sess-1"),
        _ => panic!("expected StateSnapshot"),
    }

    // Empty response (no apply path)
    let (state, _) = host(state, cmd_ok("cmd-2", CommandResult::Empty), &mut ids);
    assert_eq!(state.session_phase, SessionPhase::Live);

    // Reconcile replaces projection
    let (state, _) = host(
        state,
        reconcile_event("sess-1", ReconcileReason::ExplicitRefresh),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::Live);
}

// ═══════════════════════════════════════════════════════════════════════════
// C5 — Clear / delete visible Session
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c5_clear_only_on_session_cleared() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Delete intent
    let (state, effects) = intent(
        state,
        ClientIntent::DeleteSession {
            session_id: "sess-1".into(),
        },
        &mut ids,
    );
    match first_command(&effects) {
        Command::SessionDelete { session_id, .. } => assert_eq!(session_id, "sess-1"),
        _ => panic!("expected SessionDelete"),
    }

    // Empty response does NOT clear
    let (state, _) = host(state, cmd_ok("cmd-2", CommandResult::Empty), &mut ids);
    assert_eq!(state.session_phase, SessionPhase::Live);
    assert!(state.live_session.is_some());

    // SessionCleared actually clears
    let (state, _) = host(
        state,
        ServerMessage::SessionCleared(SessionClearedEvent {
            previous_session_id: "sess-1".into(),
        }),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::IdleNoSession);
    assert!(state.live_session.is_none());
}

#[test]
fn c5_foreign_session_cleared_ignored() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::SessionCleared(SessionClearedEvent {
            previous_session_id: "sess-other".into(),
        }),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::Live);
    assert!(state.live_session.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// C6 — Committed and realtime transcript
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c6_realtime_then_committed_replaces_draft() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Realtime delta
    let (state, _) = host(
        state,
        ServerMessage::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            message_id: "msg-1".into(),
            delta_seq: 1,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: "Hello".into(),
            },
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    let tl = session.timelines.get("root").unwrap();
    assert_eq!(tl.draft_count(), 1);
    assert_eq!(tl.committed_count(), 0);

    // Committed replaces draft
    let (state, _) = host(
        state,
        ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "msg-1".into(),
            transcript_seq: 1,
            message: piko_protocol::Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "Hello world".into(),
                }],
                api: "api".into(),
                provider: "test".into(),
                model: "test-model".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: Some(1),
            },
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    let tl = session.timelines.get("root").unwrap();
    assert_eq!(tl.committed_count(), 1);
    assert_eq!(tl.draft_count(), 0);
}

#[test]
fn c6_committed_deduplicates() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let committed = ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
        session_id: "sess-1".into(),
        agent_instance_id: "root".into(),
        agent_id: "main".into(),
        source_turn_id: "turn-1".into(),
        message_id: "msg-1".into(),
        transcript_seq: 1,
        message: piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String("hi".into()),
            timestamp: Some(1),
        },
    });

    let (state, _) = host(state, committed.clone(), &mut ids);
    let (state, _) = host(state, committed, &mut ids);

    let session = state.live_session.as_ref().unwrap();
    let tl = session.timelines.get("root").unwrap();
    assert_eq!(tl.committed_count(), 1);
}

#[test]
fn c6_foreign_session_transcript_rejected() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
            session_id: "other-session".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "msg-1".into(),
            transcript_seq: 1,
            message: piko_protocol::Message::User {
                content: piko_protocol::MessageContent::String("hi".into()),
                timestamp: Some(1),
            },
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert!(session.timelines.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// C7 — Agent select / subscribe / replay
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c7_select_agent_emits_subscribe() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (_state, effects) = intent(
        state,
        ClientIntent::SelectAgent {
            agent_instance_id: "child-1".into(),
        },
        &mut ids,
    );

    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::AgentSubscribe {
            session_id,
            agent_instance_id,
            after_seq,
            ..
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(agent_instance_id, "child-1");
            assert_eq!(*after_seq, None);
        }
        _ => panic!("expected AgentSubscribe"),
    }
}

#[test]
fn c7_subscribe_prefers_snapshot_events() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = intent(
        state,
        ClientIntent::SelectAgent {
            agent_instance_id: "child-1".into(),
        },
        &mut ids,
    );

    // Build snapshot with events
    let snapshot_events = vec![SequencedServerMessage {
        seq: 1,
        message: Box::new(ServerMessage::TranscriptCommitted(
            piko_protocol::TranscriptCommittedEvent {
                session_id: "sess-1".into(),
                agent_instance_id: "child-1".into(),
                agent_id: "child-spec".into(),
                source_turn_id: "turn-1".into(),
                message_id: "snap-msg-1".into(),
                transcript_seq: 1,
                message: piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String("from snapshot".into()),
                    timestamp: Some(1),
                },
            },
        )),
    }];

    let replay_events = vec![SequencedServerMessage {
        seq: 1,
        message: Box::new(ServerMessage::TranscriptCommitted(
            piko_protocol::TranscriptCommittedEvent {
                session_id: "sess-1".into(),
                agent_instance_id: "child-1".into(),
                agent_id: "child-spec".into(),
                source_turn_id: "turn-1".into(),
                message_id: "replay-msg-1".into(),
                transcript_seq: 1,
                message: piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String("from replay".into()),
                    timestamp: Some(1),
                },
            },
        )),
    }];

    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-2",
            CommandResult::AgentSubscribed {
                session_id: "sess-1".into(),
                agent_instance_id: "child-1".into(),
                agent_id: "child-spec".into(),
                snapshot: AgentViewSnapshot {
                    agent_instance_id: "child-1".into(),
                    agent_id: "child-spec".into(),
                    parent_agent_instance_id: Some("root".into()),
                    status: None,
                    next_seq: 2,
                    events: snapshot_events,
                },
                replay: replay_events,
                next_seq: 2,
            },
        ),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.selected_agent, Some("child-1".to_string()));
    let tl = session.timelines.get("child-1").unwrap();
    assert_eq!(tl.committed_count(), 1);
    // Should have used snapshot event (snap-msg-1), not replay
    match &tl.items()[0] {
        TimelineItem::Committed(item) => assert_eq!(item.message_id, "snap-msg-1"),
        TimelineItem::RealtimeDraft(_) => panic!("expected committed item"),
        TimelineItem::Tool(_) => panic!("expected committed item"),
    }
}
