#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult, Message, ServerMessage, TurnEvent};
use support::{HostdHarness, root_agent_id, serial_guard};

#[test]
fn spawn_agent_round_trips_from_jsonl_hostd_through_orchd_and_back() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("multi-agent");
    let session_id = host.create_session("create");
    let root_agent = root_agent_id(&session_id);

    host.send(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        root_agent.clone(),
        piko_protocol::MessageContent::String("delegate this subtask".into()),
    ));
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::AgentInputSubmitted { .. }
    ));

    let root_turn_id = match host.wait_for("root turn started", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Started {
                session_id: id,
                agent_instance_id,
                ..
            }) if id == &session_id && agent_instance_id == &root_agent
        )
    }) {
        ServerMessage::TurnLifecycle(TurnEvent::Started { turn_id, .. }) => turn_id,
        _ => unreachable!(),
    };

    let call_id = match host.wait_for("spawn_agent tool call", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolCall { name, arguments, .. }
                        if name == "spawn_agent"
                            && arguments["agent_spec_id"] == "general"
                            && arguments["prompt"] == "inspect this subtask"
                )
        )
    }) {
        ServerMessage::TranscriptCommitted(event) => match event.message {
            Message::ToolCall { id, .. } => id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    let child_result = host.wait_for("attached child report", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolResult {
                        tool_call_id,
                        tool_name: Some(name),
                        details: Some(details),
                        is_error: Some(false),
                        ..
                    } if tool_call_id == &call_id
                        && name == "spawn_agent"
                        && details["agent_spec_id"] == "general"
                        && details["attached"] == true
                )
        )
    });
    let child_id = match child_result {
        ServerMessage::TranscriptCommitted(event) => match event.message {
            Message::ToolResult {
                details: Some(details),
                ..
            } => details["agent_instance_id"]
                .as_str()
                .expect("spawn result has child id")
                .to_string(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    host.wait_for_gateway("inspect this subtask", 2);
    host.wait_for("root turn completion", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Completed {
                session_id: id,
                turn_id,
                ..
            }) if id == &session_id && turn_id == &root_turn_id
        )
    });

    host.send(Command::AgentList {
        command_id: "agents".into(),
        session_id: session_id.clone(),
    });
    let result = host.command_result("agents");
    let CommandResult::AgentListed { agents, .. } = result else {
        panic!("expected agent list");
    };
    assert!(agents.iter().any(|agent| {
        agent.agent_instance_id == child_id
            && agent.parent_agent_instance_id.as_deref() == Some(root_agent.as_str())
    }));
}
