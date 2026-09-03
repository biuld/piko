#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult, Message, ServerMessage};
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

    let root_input_id = host.wait_started(&session_id);

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
    host.wait_completed(&session_id);
    assert!(!root_input_id.is_empty());

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

    let snapshot = host.snapshot(&session_id, "snapshot-before-branch-fork");
    let root_user_id = snapshot
        .entries
        .iter()
        .find_map(|entry| match entry {
            piko_protocol::SessionTreeEntry::Message(message)
                if message.agent_instance_id == root_agent
                    && matches!(&message.message, Message::User { .. }) =>
            {
                Some(message.id.clone())
            }
            _ => None,
        })
        .expect("root user entry");
    host.send(Command::SessionFork {
        command_id: "fork-before-child".into(),
        session_id: session_id.clone(),
        entry_id: Some(root_user_id),
    });
    let forked_id = match host.command_result("fork-before-child") {
        CommandResult::SessionOpened { session_id, .. } => session_id,
        other => panic!("expected forked session open, got {other:?}"),
    };
    host.wait_for("forked session reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == forked_id
        )
    });
    host.send(Command::AgentList {
        command_id: "forked-agents".into(),
        session_id: forked_id,
    });
    let CommandResult::AgentListed { agents, .. } = host.command_result("forked-agents") else {
        panic!("expected forked agent list");
    };
    assert_eq!(
        agents.len(),
        1,
        "unreferenced child agents must not cross a branch fork"
    );
    assert_eq!(agents[0].agent_instance_id, root_agent);
}
