#[path = "support/mod.rs"]
mod support;

use piko_protocol::{
    Command, CommandResult, ContentBlock, MessageContent, QueueEvent, ServerMessage, TurnEvent,
};
use support::{HostdHarness, has_gateway_request, root_agent_id, serial_guard};

#[test]
fn structured_steer_round_trips_through_jsonl_queue_and_orchd() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("steer");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: agent_instance_id.clone(),
        text: "initial work".into(),
    });
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::Empty
    ));
    host.wait_for_gateway("initial work", 1);
    host.wait_for("initial turn started", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Started { session_id: id, .. })
                if id == &session_id
        )
    });

    host.send(Command::QueueSteerMessage {
        command_id: "steer".into(),
        session_id: session_id.clone(),
        agent_instance_id,
        content: MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "course changed".into(),
            },
            ContentBlock::Image {
                data: "ZmFrZS1zdGVlcg==".into(),
                mime_type: "image/png".into(),
            },
        ]),
    });
    // The active execution drains steer controls at its step boundary.  The
    // scripted first step is intentionally paused until the test releases it,
    // so release before awaiting the steer receipt to avoid waiting on that
    // boundary from the command task itself.
    host.release();
    assert!(matches!(host.command_result("steer"), CommandResult::Empty));
    assert!(matches!(
        host.wait_for("steer queue update", |message| {
            matches!(
                message,
                ServerMessage::Queue(QueueEvent::Updated {
                    session_id: id,
                    steer_count: 1,
                    steer_preview: Some(preview),
                    ..
                }) if id == &session_id && preview == "course changed\n[image: image/png]"
            )
        }),
        ServerMessage::Queue(_)
    ));

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
    host.send(Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: agent_instance_id,
        text: "cancel this turn".into(),
    });
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::Empty
    ));

    let started = host.wait_for("cancellable turn started", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Started { session_id: id, .. })
                if id == &session_id
        )
    });
    let ServerMessage::TurnLifecycle(TurnEvent::Started { turn_id, .. }) = started else {
        unreachable!();
    };

    host.send(Command::TurnCancel {
        command_id: "cancel".into(),
        session_id: session_id.clone(),
        turn_id: turn_id.clone(),
    });
    assert!(matches!(
        host.command_result("cancel"),
        CommandResult::Empty
    ));
    assert!(matches!(
        host.wait_for("cancelled turn", |message| {
            matches!(
                message,
                ServerMessage::TurnLifecycle(TurnEvent::Cancelled {
                    session_id: id,
                    turn_id: cancelled_id,
                    ..
                }) if id == &session_id && cancelled_id == &turn_id
            )
        }),
        ServerMessage::TurnLifecycle(_)
    ));

    let snapshot = host.snapshot(&session_id, "snapshot");
    assert!(snapshot.active_turns.is_empty());
}
