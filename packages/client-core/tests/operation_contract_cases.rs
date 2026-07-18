mod helpers;

use helpers::*;
use piko_client_core::ClientIntent;
use piko_protocol::{
    ApprovalDecision, ApprovalEvent, Command, InteractionEvent, ServerMessage, TurnEvent,
    TurnStatus, UserInteractionResponse, UserInteractionStatus,
};

// C8 — Submit and cancel Turn
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c8_submit_turn() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (_state, effects) = intent(
        state,
        ClientIntent::SubmitTurn {
            text: "Hello agent".into(),
        },
        &mut ids,
    );

    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::ChatSubmit {
            session_id,
            target_agent_instance_id,
            text,
            ..
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(target_agent_instance_id, "root");
            assert_eq!(text, "Hello agent");
        }
        _ => panic!("expected ChatSubmit"),
    }
}

#[test]
fn c8_submit_empty_text_rejected() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (_, effects) = intent(
        state,
        ClientIntent::SubmitTurn { text: "   ".into() },
        &mut ids,
    );

    assert!(effects.is_empty());
}

#[test]
fn c8_turn_lifecycle_tracking() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Turn started
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Started {
            session_id: "sess-1".into(),
            turn_id: "turn-1".into(),
            agent_instance_id: "root".into(),
            timestamp: 1,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.active_turns.len(), 1);
    assert_eq!(session.active_turns[0].status, TurnStatus::Running);

    // Cancel intent
    let (state, effects) = intent(state, ClientIntent::CancelTurn, &mut ids);
    match first_command(&effects) {
        Command::TurnCancel { turn_id, .. } => assert_eq!(turn_id, "turn-1"),
        _ => panic!("expected TurnCancel"),
    }

    // Turn completed
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Completed {
            session_id: "sess-1".into(),
            turn_id: "turn-1".into(),
            agent_instance_id: "root".into(),
            timestamp: 2,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert!(session.active_turns.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// C9 — Approval lifecycle
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c9_approval_requested_then_responded() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Approval requested
    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            approval_id: "approval-1".into(),
            tool_name: "write_file".into(),
            tool_args: serde_json::json!({"path": "/tmp/x"}),
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_approvals.len(), 1);
    assert_eq!(session.pending_approvals[0].approval_id, "approval-1");

    // Respond
    let (state, effects) = intent(
        state,
        ClientIntent::RespondApproval {
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        },
        &mut ids,
    );

    assert_eq!(effects.len(), 1);
    let session = state.live_session.as_ref().unwrap();
    // Response does NOT remove the prompt
    assert_eq!(session.pending_approvals.len(), 1);
    assert!(session.pending_approvals[0].response_in_flight);
}

#[test]
fn c9_response_keeps_prompt_until_resolved() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Add approval
    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            approval_id: "approval-1".into(),
            tool_name: "exec".into(),
            tool_args: serde_json::json!({}),
        }),
        &mut ids,
    );

    // Respond
    let (state, _) = intent(
        state,
        ClientIntent::RespondApproval {
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        },
        &mut ids,
    );

    // Still pending
    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_approvals.len(), 1);

    // Resolved removes it
    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Resolved {
            session_id: "sess-1".into(),
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert!(session.pending_approvals.is_empty());
}

#[test]
fn c9_attention_queue_approvals_before_interactions() {
    use piko_client_core::{AttentionKind, prompt_queue};

    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::Interaction(InteractionEvent::Requested {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            interaction_id: "ix-1".into(),
            tool_call_id: "tc-1".into(),
            title: Some("Choose".into()),
            questions: vec![],
            require_confirm: false,
            auto_resolution_ms: None,
        }),
        &mut ids,
    );

    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            approval_id: "approval-1".into(),
            tool_name: "exec".into(),
            tool_args: serde_json::json!({}),
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    let q = prompt_queue(session);
    assert_eq!(q.len(), 2);
    assert_eq!(q[0].kind, AttentionKind::Approval);
    assert_eq!(q[0].id, "approval-1");
    assert_eq!(q[1].kind, AttentionKind::Interaction);
    assert_eq!(q[1].id, "ix-1");

    // Respond does not remove from queue
    let (state, _) = intent(
        state,
        ClientIntent::RespondApproval {
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        },
        &mut ids,
    );
    let session = state.live_session.as_ref().unwrap();
    let q = prompt_queue(session);
    assert_eq!(q.len(), 2);
    assert!(q[0].response_in_flight);
}

// ═══════════════════════════════════════════════════════════════════════════
// C10 — User interaction lifecycle
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c10_interaction_requested_then_resolved() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::Interaction(InteractionEvent::Requested {
            session_id: "sess-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            interaction_id: "ix-1".into(),
            tool_call_id: "tc-1".into(),
            title: Some("Choose".into()),
            questions: vec![],
            require_confirm: false,
            auto_resolution_ms: None,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_interactions.len(), 1);

    // Respond with cancel
    let (state, effects) = intent(
        state,
        ClientIntent::RespondInteraction {
            interaction_id: "ix-1".into(),
            response: UserInteractionResponse::Cancel {
                reason: Some("nope".into()),
            },
        },
        &mut ids,
    );
    assert_eq!(effects.len(), 1);

    // Still pending until resolved
    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_interactions.len(), 1);
    assert!(session.pending_interactions[0].response_in_flight);

    // Resolved
    let (state, _) = host(
        state,
        ServerMessage::Interaction(InteractionEvent::Resolved {
            session_id: "sess-1".into(),
            interaction_id: "ix-1".into(),
            status: UserInteractionStatus::Cancelled,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert!(session.pending_interactions.is_empty());
}
