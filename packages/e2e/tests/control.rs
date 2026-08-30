#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult, ContentBlock, MessageContent, ServerMessage};
use support::{HostdHarness, has_gateway_request, root_agent_id, serial_guard};

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
