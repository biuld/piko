mod helpers;

use helpers::*;
use piko_client_core::{
    ClientIntent, ClientMsg, ClientState, TimelineItem, ToolStatus, TransportObservation,
};
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{
    ApprovalDecision, ApprovalEvent, ReconcileReason, ServerMessage, ToolExecutionEvent,
};

#[test]
fn tool_lifecycle_is_projected_and_scoped() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::StreamItem(
            piko_protocol::StreamItemPatch::from_tool_execution(&ToolExecutionEvent::Started {
                session_id: "s1".into(),
                agent_instance_id: "root".into(),
                agent_id: "main".into(),
                tool_call_id: "call-1".into(),
                tool_name: "exec".into(),
                args: serde_json::json!({"cmd": "true"}),
                parent_message_id: Some("m1".into()),
                source_turn_id: Some("turn-1".into()),
            })
            .into_iter()
            .next()
            .unwrap(),
        ),
        &mut ids,
    );
    let tool = state.live_session.as_ref().unwrap().timelines["root"]
        .items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::Tool(tool) => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.status, ToolStatus::Running);

    let (state, _) = host(
        state,
        ServerMessage::StreamItem(
            piko_protocol::StreamItemPatch::from_tool_execution(&ToolExecutionEvent::Ended {
                session_id: "s1".into(),
                agent_instance_id: "root".into(),
                agent_id: "main".into(),
                tool_call_id: "call-1".into(),
                tool_name: "exec".into(),
                result: serde_json::json!({"exit": 0}),
                is_error: false,
                parent_message_id: None,
                source_turn_id: Some("turn-1".into()),
            })
            .into_iter()
            .next()
            .unwrap(),
        ),
        &mut ids,
    );
    let tool = state.live_session.as_ref().unwrap().timelines["root"]
        .items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::Tool(tool) => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.status, ToolStatus::Completed);
    assert_eq!(tool.result, Some(serde_json::json!({"exit": 0})));
}

#[test]
fn live_session_entry_is_backfilled_into_future_agent_timeline() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::SessionEntryCommitted(piko_protocol::SessionEntryCommittedEvent {
            session_id: "s1".into(),
            entry: piko_protocol::SessionTreeEntry::ModelChange(piko_protocol::ModelChangeEntry {
                id: "model-change".into(),
                parent_id: None,
                timestamp: "1".into(),
                provider: "openai".into(),
                model_id: "gpt".into(),
            }),
        }),
        &mut ids,
    );
    let (state, _) = host(
        state,
        ServerMessage::StreamItem(
            piko_protocol::StreamItemPatch::from_realtime_delta(
                Some("s1".into()),
                Some("child".into()),
                "child-message",
                Some(1),
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: "hello".into(),
                },
            )
            .into_iter()
            .next()
            .unwrap(),
        ),
        &mut ids,
    );
    let timeline = &state.live_session.as_ref().unwrap().timelines["child"];
    assert!(matches!(
        timeline.items().first(),
        Some(TimelineItem::SessionEntry(entry))
            if entry.entry.id() == "model-change"
    ));
}

#[test]
fn queue_update_populates_projection() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let mut snapshot = session_snapshot("s1");
    snapshot.agent_work = vec![piko_protocol::AgentWorkSnapshot {
        agent_instance_id: "root".into(),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        foreground: piko_protocol::AgentForeground::Queued,
        active_work: None,
        pending_steers: vec![piko_protocol::AgentInputSummary {
            input_id: "steer-1".into(),
            origin: piko_protocol::AgentInputOrigin::User,
            preview: "steer".into(),
            admission_revision: 1,
            submitted_at: 1,
            delivery: piko_protocol::AgentInputDelivery::SteerActive,
            disposition: piko_protocol::AgentInputDisposition::PendingSteer,
        }],
        queued_inputs: vec![
            piko_protocol::AgentInputSummary {
                input_id: "q-1".into(),
                origin: piko_protocol::AgentInputOrigin::User,
                preview: "later".into(),
                admission_revision: 2,
                submitted_at: 2,
                delivery: piko_protocol::AgentInputDelivery::FollowUp,
                disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
            },
            piko_protocol::AgentInputSummary {
                input_id: "q-2".into(),
                origin: piko_protocol::AgentInputOrigin::User,
                preview: "after".into(),
                admission_revision: 3,
                submitted_at: 3,
                delivery: piko_protocol::AgentInputDelivery::FollowUp,
                disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
            },
        ],
        pending_action: None,
    }];
    let (state, _) = host(
        state,
        ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
            session_id: "s1".into(),
            reason: ReconcileReason::ExplicitRefresh,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "e1".into(),
                seq: 2,
            },
            snapshot,
            agents: vec![agent_info("s1", "root", None)],
        }),
        &mut ids,
    );
    let queue = &state.live_session.as_ref().unwrap().queue;
    assert_eq!(queue.steer_count, 1);
    assert_eq!(queue.follow_up_count, 2);
    assert_eq!(queue.next_turn_count, 2);
}

#[test]
fn rejected_approval_response_reenables_prompt() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            approval_id: "a1".into(),
            tool_name: "exec".into(),
            tool_args: serde_json::json!({}),
            prompt: None,
        }),
        &mut ids,
    );
    let (state, _) = intent(
        state,
        ClientIntent::RespondApproval {
            approval_id: "a1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        },
        &mut ids,
    );
    assert!(state.live_session.as_ref().unwrap().pending_approvals[0].response_in_flight);
    let (state, _) = host(state, cmd_err("cmd-2", "denied"), &mut ids);
    assert!(!state.live_session.as_ref().unwrap().pending_approvals[0].response_in_flight);
}

#[test]
fn realtime_gap_requests_one_refresh() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let realtime = |delta_seq, delta: &str| {
        ServerMessage::StreamItem(
            piko_protocol::StreamItemPatch::from_realtime_delta(
                Some("s1".into()),
                Some("root".into()),
                "m1",
                Some(delta_seq),
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: delta.into(),
                },
            )
            .into_iter()
            .next()
            .unwrap(),
        )
    };
    let (state, effects) = host(state, realtime(1, "a"), &mut ids);
    assert!(effects.is_empty());
    let (state, effects) = host(state, realtime(3, "c"), &mut ids);
    assert!(matches!(
        first_command(&effects),
        piko_protocol::Command::StateSnapshot { .. }
    ));
    let draft = state.live_session.as_ref().unwrap().timelines["root"]
        .items()
        .iter()
        .find_map(|item| match item {
            TimelineItem::RealtimeDraft(draft) => Some(draft),
            _ => None,
        })
        .unwrap();
    assert_eq!(draft.text(), "a");

    let (_, effects) = host(state, realtime(4, "d"), &mut ids);
    assert!(
        effects.is_empty(),
        "refresh must be coalesced while pending"
    );
}

#[test]
fn send_failure_correlates_and_clears_pending_commands() {
    let mut ids = SeqIds(0);
    let (state, _) = intent(ClientState::default(), ClientIntent::ListModels, &mut ids);
    assert!(state.pending_commands.contains_key("cmd-1"));

    let (state, _) = apply(
        state,
        ClientMsg::Transport(TransportObservation::SendFailure {
            detail: "broken pipe".into(),
        }),
        &mut ids,
    );

    assert!(state.pending_commands.is_empty());
    assert_eq!(state.command_failures.len(), 1);
    assert_eq!(state.command_failures[0].command_id, "cmd-1");
    assert_eq!(state.command_failures[0].message, "broken pipe");
}
