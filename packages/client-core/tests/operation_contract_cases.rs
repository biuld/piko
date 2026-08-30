mod helpers;

use helpers::*;
use piko_client_core::ClientIntent;
use piko_protocol::{
    ApprovalDecision, ApprovalEvent, Command, InteractionEvent, ServerMessage, TurnEvent,
    UserInteractionResponse, UserInteractionStatus,
};

fn test_cost(total: f64) -> piko_protocol::messages::UsageCost {
    piko_protocol::messages::UsageCost {
        entries: vec![piko_protocol::messages::UsageCostEntry {
            currency: "USD".into(),
            basis: piko_protocol::messages::UsageCostBasis::ListPrice,
            components: [("input_tokens".into(), total)].into(),
            total,
        }],
    }
}

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
        Command::AgentInputSubmit { input, .. } => {
            assert_eq!(input.session_id, "sess-1");
            assert_eq!(input.agent_instance_id, "root");
            assert_eq!(
                input.content,
                piko_protocol::MessageContent::String("Hello agent".into())
            );
            assert_eq!(input.delivery, piko_protocol::AgentInputDelivery::FollowUp);
        }
        _ => panic!("expected AgentInputSubmit"),
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

    let mut snapshot = session_snapshot("sess-1");
    snapshot.agent_work = vec![piko_protocol::AgentWorkSnapshot {
        agent_instance_id: "root".into(),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        foreground: piko_protocol::AgentForeground::Running,
        active_work: Some(piko_protocol::ActiveWorkSnapshot {
            root_input_id: "turn-1".into(),
            state: piko_protocol::AgentWorkViewState::Running,
            active_model_step_id: None,
            started_at: 1,
        }),
        pending_steers: Vec::new(),
        queued_inputs: Vec::new(),
        pending_action: None,
    }];
    let (state, _) = host(
        state,
        ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
            session_id: "sess-1".into(),
            reason: piko_protocol::ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "e1".into(),
                seq: 2,
            },
            snapshot,
            agents: vec![agent_info("sess-1", "root", None)],
        }),
        &mut ids,
    );

    // Cancel intent
    let (state, effects) = intent(state, ClientIntent::CancelTurn, &mut ids);
    match first_command(&effects) {
        Command::AgentInterrupt {
            session_id,
            agent_instance_id,
            ..
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(agent_instance_id, "root");
        }
        _ => panic!("expected AgentInterrupt"),
    }

    // Turn completed
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Completed {
            session_id: "sess-1".into(),
            turn_id: "turn-1".into(),
            agent_instance_id: "root".into(),
            usage: Default::default(),
            timestamp: 2,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert!(session.turn_failures.is_empty());
}

#[test]
fn turn_completed_does_not_roll_usage_chrome() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Started {
            session_id: "sess-1".into(),
            turn_id: "turn-u".into(),
            agent_instance_id: "root".into(),
            timestamp: 1,
        }),
        &mut ids,
    );

    let usage = piko_protocol::messages::Usage {
        input: 10_000,
        output: 100,
        cache_read: 3_000,
        cache_write: 0,
        total_tokens: 13_100,
        units: Default::default(),
        cost: test_cost(0.01),
    };
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Completed {
            session_id: "sess-1".into(),
            turn_id: "turn-u".into(),
            agent_instance_id: "root".into(),
            usage: usage.clone(),
            timestamp: 2,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.last_context_tokens, None);
    assert_eq!(session.cumulative_usage, None);
}

#[test]
fn usage_updated_event_is_authoritative_for_chrome() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Terminal turn alone leaves chrome empty; Usage is the sole path.
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Completed {
            session_id: "sess-1".into(),
            turn_id: "turn-u".into(),
            agent_instance_id: "root".into(),
            usage: piko_protocol::messages::Usage {
                input: 1_000,
                output: 10,
                cache_read: 0,
                cache_write: 0,
                total_tokens: 1_010,
                units: Default::default(),
                cost: Default::default(),
            },
            timestamp: 1,
        }),
        &mut ids,
    );
    assert_eq!(
        state.live_session.as_ref().unwrap().last_context_tokens,
        None
    );

    let cumulative = piko_protocol::messages::Usage {
        input: 50_000,
        output: 2_000,
        cache_read: 10_000,
        cache_write: 0,
        total_tokens: 62_000,
        units: Default::default(),
        cost: test_cost(1.25),
    };
    let (state, _) = host(
        state,
        ServerMessage::Usage(piko_protocol::UsageEvent::Updated {
            session_id: "sess-1".into(),
            agent_instance_id: Some("root".into()),
            turn_id: Some("turn-u".into()),
            used: 60_000,
            size: Some(200_000),
            cumulative: Some(cumulative),
            turn_usage: None,
            timestamp: 2,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.last_context_tokens, Some(60_000));
    assert_eq!(
        session.cumulative_usage.as_ref().map(|u| u.total_tokens),
        Some(62_000)
    );
    assert!(
        (session.cumulative_usage.as_ref().unwrap().cost.entries[0].total - 1.25).abs()
            < f64::EPSILON
    );
    assert_eq!(state.model.context_window, Some(200_000));
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
            prompt: None,
        }),
        &mut ids,
    );

    let session = state.live_session.as_ref().unwrap();
    assert_eq!(session.pending_approvals.len(), 1);
    assert_eq!(session.pending_approvals[0].approval_id, "approval-1");
    assert_eq!(
        piko_client_core::agent_foreground("root", session),
        piko_protocol::AgentForeground::RequiresAction
    );

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
            prompt: None,
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
            prompt: None,
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
