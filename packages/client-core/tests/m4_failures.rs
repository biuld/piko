//! M4 failure / lifecycle regression cases for Client Core.

mod helpers;

use helpers::*;
use piko_client_core::{AttentionKind, ClientIntent, TimelineItem, prompt_queue};
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{
    ApprovalEvent, ApprovalSnapshot, ApprovalStatus, ReconcileReason, ServerMessage, TurnEvent,
    TurnStatus, UserInteractionSnapshot, UserInteractionStatus,
};

#[test]
fn m4_turn_queued_then_started_then_cancelled() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Queued {
            session_id: "sess-1".into(),
            turn_id: "turn-q".into(),
            agent_instance_id: "root".into(),
            timestamp: 1,
        }),
        &mut ids,
    );
    {
        let session = state.live_session.as_ref().unwrap();
        assert_eq!(session.active_turns.len(), 1);
        assert_eq!(session.active_turns[0].status, TurnStatus::Queued);
    }

    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Started {
            session_id: "sess-1".into(),
            turn_id: "turn-q".into(),
            agent_instance_id: "root".into(),
            timestamp: 2,
        }),
        &mut ids,
    );
    assert_eq!(
        state.live_session.as_ref().unwrap().active_turns[0].status,
        TurnStatus::Running
    );

    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Cancelled {
            session_id: "sess-1".into(),
            turn_id: "turn-q".into(),
            agent_instance_id: "root".into(),
            timestamp: 3,
        }),
        &mut ids,
    );
    assert!(state.live_session.as_ref().unwrap().active_turns.is_empty());
}

#[test]
fn m4_stale_realtime_delta_seq_ignored() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            message_id: "msg-1".into(),
            delta_seq: 2,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: "second".into(),
            },
        }),
        &mut ids,
    );

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
                delta: "STALE".into(),
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
    let draft = tl
        .items()
        .iter()
        .find_map(|i| match i {
            TimelineItem::RealtimeDraft(d) => Some(d),
            _ => None,
        })
        .expect("draft");
    let body = draft.text_segments.join("");
    assert!(body.contains("second"));
    assert!(!body.contains("STALE"));
    assert_eq!(draft.last_delta_seq, 2);

    // Committed still replaces the draft.
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
                    text: "final".into(),
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

#[test]
fn m4_reconcile_restores_pending_prompts() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let mut event = match reconcile_event("sess-1", ReconcileReason::ExplicitRefresh) {
        ServerMessage::SessionReconciled(e) => e,
        _ => unreachable!(),
    };
    event.snapshot.pending_approvals.push(ApprovalSnapshot {
        approval_id: "ap-restore".into(),
        agent_instance_id: "root".into(),
        tool_name: "exec".into(),
        request: serde_json::json!({"cmd": "ls"}),
        status: ApprovalStatus::Pending,
    });
    event
        .snapshot
        .pending_interactions
        .push(UserInteractionSnapshot {
            interaction_id: "ix-restore".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            tool_call_id: "tc-1".into(),
            status: UserInteractionStatus::Pending,
            title: Some("Pick".into()),
            questions: vec![],
            require_confirm: false,
            auto_resolution_ms: None,
        });

    let (state, _) = host(state, ServerMessage::SessionReconciled(event), &mut ids);
    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_approvals.len(), 1);
    assert_eq!(session.pending_approvals[0].approval_id, "ap-restore");
    assert_eq!(session.pending_interactions.len(), 1);
    assert_eq!(session.pending_interactions[0].interaction_id, "ix-restore");

    let queue = prompt_queue(session);
    assert!(
        queue
            .iter()
            .any(|i| i.kind == AttentionKind::Approval && i.id == "ap-restore")
    );
    assert!(
        queue
            .iter()
            .any(|i| i.kind == AttentionKind::Interaction && i.id == "ix-restore")
    );
}

#[test]
fn m4_failed_tool_result_on_timeline() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "tc-1".into(),
            transcript_seq: 1,
            message: piko_protocol::Message::ToolCall {
                id: "call-fail".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"cmd": "false"}),
                model: None,
                provider: None,
                timestamp: Some(1),
            },
        }),
        &mut ids,
    );

    let (state, _) = host(
        state,
        ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "tr-1".into(),
            transcript_seq: 2,
            message: piko_protocol::Message::ToolResult {
                tool_call_id: "call-fail".into(),
                tool_name: Some("bash".into()),
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "exit 1".into(),
                }],
                details: None,
                is_error: Some(true),
                timestamp: Some(2),
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
    let has_error_result = tl.items().iter().any(|i| match i {
        TimelineItem::Committed(c) => matches!(
            &c.message,
            piko_protocol::Message::ToolResult {
                is_error: Some(true),
                ..
            }
        ),
        _ => false,
    });
    assert!(has_error_result);

    // Foreign approval must not leak into this session.
    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "other".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            approval_id: "foreign".into(),
            tool_name: "x".into(),
            tool_args: serde_json::json!({}),
        }),
        &mut ids,
    );
    assert!(
        state
            .live_session
            .as_ref()
            .unwrap()
            .pending_approvals
            .is_empty()
    );
}

#[test]
fn m4_submit_rejected_keeps_live() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");
    let (state, _) = intent(
        state,
        ClientIntent::SubmitTurn {
            text: "hello".into(),
        },
        &mut ids,
    );
    let (state, _) = host(state, cmd_err("cmd-2", "model unavailable"), &mut ids);
    assert!(state.is_live());
    assert_eq!(state.last_error.as_deref(), Some("model unavailable"));
    assert!(state.pending_commands.is_empty());
}
