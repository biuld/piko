#[path = "support/mod.rs"]
mod support;

use piko_protocol::{
    AgentForeground, AgentInputDisposition, AgentWorkViewState, Command, CommandResult,
    ContentBlock, MessageContent, ServerMessage,
};
use support::{HostdHarness, has_gateway_request, root_agent_id, serial_guard};

fn session_path(host: &mut HostdHarness, session_id: &str, command_id: &str) -> String {
    host.send(Command::SessionList {
        command_id: command_id.into(),
        scope: piko_protocol::SessionListScope::All,
        cwd: None,
    });
    match host.command_result(command_id) {
        CommandResult::SessionListed { sessions, .. } => sessions
            .into_iter()
            .find(|session| session.session_id == session_id)
            .and_then(|session| session.session_path)
            .unwrap_or_else(|| panic!("session path for {session_id}")),
        other => panic!("expected session list, got {other:?}"),
    }
}

fn open_after_restart(
    host: &mut HostdHarness,
    session_id: &str,
    session_path: String,
) -> piko_protocol::SessionSnapshot {
    host.send(Command::SessionOpen {
        command_id: "open-after-restart".into(),
        session_id: session_id.into(),
        session_path: Some(session_path),
    });
    assert!(matches!(
        host.command_result("open-after-restart"),
        CommandResult::SessionOpened {
            session_id: opened,
            ..
        } if opened == session_id
    ));
    match host.wait_for("restart reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    }) {
        ServerMessage::SessionReconciled(event) => event.snapshot,
        _ => unreachable!(),
    }
}

#[test]
fn structured_steer_round_trips_through_jsonl_queue_and_orchd() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("steer");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("initial work".into()),
    ));
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::AgentInputSubmitted { .. }
    ));
    host.wait_for_gateway("initial work", 1);
    host.wait_started(&session_id);

    host.send(Command::submit_steer(
        "steer",
        session_id.clone(),
        agent_instance_id,
        MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "course changed".into(),
            },
            ContentBlock::Image {
                data: "ZmFrZS1zdGVlcg==".into(),
                mime_type: "image/png".into(),
            },
        ]),
    ));
    // The active execution drains steer controls at its step boundary.  The
    // scripted first step is intentionally paused until the test releases it,
    // so release before awaiting the steer receipt to avoid waiting on that
    // boundary from the command task itself.
    host.release();
    assert!(matches!(
        host.command_result("steer"),
        CommandResult::AgentInputSubmitted { .. }
    ));
    host.wait_for("steer snapshot", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.agent_work.iter().any(|work| {
                        work.pending_steers.iter().any(|input| {
                            input.preview.contains("course changed")
                        })
                    })
        )
    });

    host.wait_for_gateway("course changed", 2);
    host.wait_for_gateway("course changed", 3);
    assert!(has_gateway_request(
        &host.gateway_trace(),
        "course changed",
        2
    ));
    host.wait_completed(&session_id);
}

#[test]
fn turn_cancel_crosses_jsonl_hostd_and_orchd_and_clears_active_state() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("cancel");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    host.send(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("cancel this turn".into()),
    ));
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::AgentInputSubmitted { .. }
    ));

    let root_input_id = host.wait_started(&session_id);

    host.send(Command::AgentInterrupt {
        command_id: "cancel".into(),
        session_id: session_id.clone(),
        agent_instance_id,
    });
    assert!(matches!(
        host.command_result("cancel"),
        CommandResult::AgentInterrupted { accepted: true, .. }
    ));
    host.wait_for("authoritative cancelling snapshot", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.agent_work.iter().any(|work| {
                        work.agent_instance_id == root_agent_id(&session_id)
                            && work.foreground == AgentForeground::Cancelling
                            && work.active_work.as_ref().is_some_and(|active| {
                                active.root_input_id == root_input_id
                                    && active.state == AgentWorkViewState::Cancelling
                            })
                    })
        )
    });
    host.wait_completed(&session_id);

    let snapshot = host.snapshot(&session_id, "snapshot");
    assert!(
        snapshot
            .agent_work
            .iter()
            .all(|work| work.active_work.is_none())
    );
    assert!(!root_input_id.is_empty());
}

#[test]
fn queue_cancellation_and_surviving_identity_rehydrate_after_restart() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("queue");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::submit_follow_up(
        "submit-root",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("block the root".into()),
    ));
    assert!(matches!(
        host.command_result("submit-root"),
        CommandResult::AgentInputSubmitted { .. }
    ));
    host.wait_started(&session_id);

    host.send(Command::submit_follow_up(
        "submit-survivor",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("survive the restart".into()),
    ));
    let surviving_input_id = match host.command_result("submit-survivor") {
        CommandResult::AgentInputSubmitted { receipt, .. } => {
            assert_eq!(receipt.disposition, AgentInputDisposition::PendingFollowUp);
            receipt.input_id
        }
        other => panic!("expected queued input receipt, got {other:?}"),
    };

    host.send(Command::submit_follow_up(
        "submit-cancelled",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("cancel by identity".into()),
    ));
    let cancelled_input_id = match host.command_result("submit-cancelled") {
        CommandResult::AgentInputSubmitted { receipt, .. } => {
            assert_eq!(receipt.disposition, AgentInputDisposition::PendingFollowUp);
            receipt.input_id
        }
        other => panic!("expected queued input receipt, got {other:?}"),
    };

    host.send(Command::AgentInputCancel {
        command_id: "cancel-queued".into(),
        session_id: session_id.clone(),
        agent_instance_id: agent_instance_id.clone(),
        input_id: cancelled_input_id.clone(),
    });
    match host.command_result("cancel-queued") {
        CommandResult::AgentInputCancelled { receipt, .. } => {
            assert!(receipt.accepted);
            assert_eq!(receipt.input_id, cancelled_input_id);
            assert_eq!(receipt.agent_instance_id, agent_instance_id);
        }
        other => panic!("expected input cancellation receipt, got {other:?}"),
    }
    host.wait_for("identity-based queue cancellation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.agent_work.iter().any(|work| {
                        work.agent_instance_id == agent_instance_id
                            && work.queued_inputs.len() == 1
                            && work.queued_inputs[0].input_id == surviving_input_id
                            && work.queued_inputs.iter().all(|input| {
                                input.input_id != cancelled_input_id
                            })
                    })
        )
    });

    let path = session_path(&mut host, &session_id, "list-before-restart");
    host.restart();
    let recovered = open_after_restart(&mut host, &session_id, path);
    let recovered_work = recovered
        .agent_work
        .iter()
        .find(|work| work.agent_instance_id == agent_instance_id)
        .expect("root agent work after restart");
    assert!(
        recovered_work.active_work.is_none(),
        "the interrupted root is terminalized during open recovery"
    );
    assert_eq!(recovered_work.foreground, AgentForeground::Queued);
    assert_eq!(recovered_work.queued_inputs.len(), 1);
    assert_eq!(recovered_work.queued_inputs[0].input_id, surviving_input_id);
    assert!(
        recovered_work
            .queued_inputs
            .iter()
            .all(|input| input.input_id != cancelled_input_id),
        "the cancelled queue fact remains absent after journal/read-model hydration"
    );

    host.send(Command::submit_steer(
        "steer-after-restart",
        session_id.clone(),
        agent_instance_id.clone(),
        MessageContent::String("must not retarget the queue".into()),
    ));
    assert!(
        host.command_error("steer-after-restart")
            .contains("not running"),
        "an idle recovered agent rejects steer instead of binding it to a successor"
    );
    let after_rejected_steer = host.snapshot(&session_id, "snapshot-after-rejected-steer");
    let work_after_rejected_steer = after_rejected_steer
        .agent_work
        .iter()
        .find(|work| work.agent_instance_id == agent_instance_id)
        .expect("root agent work after rejected steer");
    assert!(work_after_rejected_steer.pending_steers.is_empty());
    assert_eq!(work_after_rejected_steer.queued_inputs.len(), 1);
    assert_eq!(
        work_after_rejected_steer.queued_inputs[0].input_id,
        surviving_input_id
    );

    host.send(Command::AgentInputCancel {
        command_id: "cancel-after-restart".into(),
        session_id: session_id.clone(),
        agent_instance_id: agent_instance_id.clone(),
        input_id: surviving_input_id.clone(),
    });
    assert!(matches!(
        host.command_result("cancel-after-restart"),
        CommandResult::AgentInputCancelled { receipt, .. }
            if receipt.accepted && receipt.input_id == surviving_input_id
    ));
    host.wait_for("post-restart queue cancellation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.agent_work.iter().any(|work| {
                        work.agent_instance_id == agent_instance_id
                            && work.queued_inputs.is_empty()
                            && work.foreground == AgentForeground::Idle
                    })
        )
    });
}
