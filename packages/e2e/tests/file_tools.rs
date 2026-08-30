#[path = "support/mod.rs"]
mod support;

use std::fs;

use piko_protocol::{ApprovalDecision, Command, CommandResult, Message, ServerMessage};
use support::{HostdHarness, root_agent_id, serial_guard};

#[test]
fn workspace_edit_requires_approval_updates_the_file_and_records_diff() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("edit");
    fs::write(host.workspace().join("e2e-input.txt"), "before\nafter\n")
        .expect("seed edit fixture");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        agent_instance_id,
        piko_protocol::MessageContent::String("exercise the edit tool".into()),
    ));
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::AgentInputSubmitted { .. }
    ));
    let turn_id = host.wait_started(&session_id);

    let approval_id = match host.wait_for("edit approval", |message| {
        matches!(
            message,
            ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
                tool_name, ..
            }) if tool_name == "edit"
        )
    }) {
        ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
            approval_id,
            tool_args,
            ..
        }) => {
            assert_eq!(tool_args["path"], "e2e-input.txt");
            approval_id
        }
        _ => unreachable!(),
    };

    host.send(Command::ApprovalRespond {
        command_id: "approve".into(),
        session_id: session_id.clone(),
        approval_id,
        decision: ApprovalDecision::Accept,
        note: None,
    });
    assert!(matches!(
        host.command_result("approve"),
        CommandResult::Empty
    ));

    assert!(matches!(
        host.wait_for("edit tool result", |message| {
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
                        } if tool_call_id == "call-edit"
                            && name == "edit"
                            && details["edited"] == true
                    )
            )
        }),
        ServerMessage::TranscriptCommitted(_)
    ));
    assert_eq!(
        fs::read_to_string(host.workspace().join("e2e-input.txt")).expect("edited fixture"),
        "after\nafter\n"
    );

    let diff = host.wait_for("edit turn diff", |message| {
        matches!(
            message,
            ServerMessage::AgentWorkDiff(event)
                if event.session_id == session_id
                    && event.root_input_id == turn_id
                    && event.files.iter().any(|file| file.path == "e2e-input.txt")
        )
    });
    assert!(matches!(diff, ServerMessage::AgentWorkDiff(_)));
    host.wait_completed(&session_id);

    host.send(Command::AgentWorkDiffGet {
        command_id: "diff-query".into(),
        session_id,
        root_input_id: turn_id,
    });
    assert!(matches!(
        host.command_result("diff-query"),
        CommandResult::AgentWorkDiffGot { diff: Some(diff), .. }
            if diff.files.iter().any(|file| file.path == "e2e-input.txt")
    ));
}
