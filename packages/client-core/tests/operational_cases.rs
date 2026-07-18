mod helpers;

use helpers::*;
use piko_client_core::{
    ClientIntent, ClientMsg, ClientState, TimelineItem, ToolStatus, TransportObservation,
};
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{
    ApprovalDecision, ApprovalEvent, QueueEvent, ServerMessage, ToolExecutionEvent, TurnEvent,
};

#[test]
fn tool_lifecycle_is_projected_and_scoped() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::ToolExecution(ToolExecutionEvent::Started {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            tool_call_id: "call-1".into(),
            tool_name: "exec".into(),
            args: serde_json::json!({"cmd": "true"}),
            parent_message_id: Some("m1".into()),
        }),
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
        ServerMessage::ToolExecution(ToolExecutionEvent::Ended {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            tool_call_id: "call-1".into(),
            tool_name: "exec".into(),
            result: serde_json::json!({"exit": 0}),
            is_error: false,
        }),
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
fn queue_update_populates_projection() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::Queue(QueueEvent::Updated {
            session_id: "s1".into(),
            steer_count: 1,
            follow_up_count: 2,
            next_turn_count: 3,
            steer_preview: Some("steer".into()),
            follow_up_preview: Some("later".into()),
        }),
        &mut ids,
    );
    let queue = &state.live_session.as_ref().unwrap().queue;
    assert_eq!(queue.steer_count, 1);
    assert_eq!(queue.follow_up_count, 2);
    assert_eq!(queue.next_turn_count, 3);
}

#[test]
fn failed_turn_remains_actionable() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "s1");
    let (state, _) = host(
        state,
        ServerMessage::TurnLifecycle(TurnEvent::Failed {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            agent_instance_id: "root".into(),
            error: "model failed".into(),
            timestamp: 1,
        }),
        &mut ids,
    );
    let live = state.live_session.as_ref().unwrap();
    assert!(live.active_turns.is_empty());
    assert_eq!(live.turn_failures[0].error, "model failed");
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
        ServerMessage::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            session_id: "s1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            message_id: "m1".into(),
            delta_seq,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: delta.into(),
            },
        })
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
    assert_eq!(draft.text_segments.join(""), "a");

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
