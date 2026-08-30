#[path = "support/mod.rs"]
mod support;

use std::fs;

use piko_protocol::{
    ApprovalDecision, Command, CommandResult, InteractionAnswer, InteractionEvent, Message,
    ServerMessage, StreamItemKind, StreamItemOp, TodoStatus, UserInteractionResponse,
};
use support::{HostdHarness, root_agent_id, serial_guard};

fn submit(host: &mut HostdHarness, session_id: &str, agent_instance_id: &str, command_id: &str) {
    host.send(Command::submit_follow_up(
        command_id,
        session_id,
        agent_instance_id,
        piko_protocol::MessageContent::String("exercise the real tool path".into()),
    ));
    assert!(matches!(
        host.command_result(command_id),
        CommandResult::AgentInputSubmitted { .. }
    ));
}

fn started_turn(host: &mut HostdHarness, session_id: &str) -> String {
    host.wait_started(session_id)
}

#[test]
fn workspace_read_tool_executes_through_real_orchd_and_persists_result() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("read");
    fs::write(host.workspace().join("e2e-input.txt"), "before\nafter\n")
        .expect("seed read fixture");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    submit(&mut host, &session_id, &agent_instance_id, "submit");
    let turn_id = started_turn(&mut host, &session_id);

    let call = host.wait_for("read tool call", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolCall { name, arguments, .. }
                        if name == "read" && arguments["path"] == "e2e-input.txt"
                )
        )
    });
    let call_id = match call {
        ServerMessage::TranscriptCommitted(event) => match event.message {
            Message::ToolCall { id, .. } => id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    assert!(matches!(
        host.wait_for("read tool stream start", |message| {
            matches!(
                message,
                ServerMessage::StreamItem(patch)
                    if patch.item_id == call_id
                        && patch.item_kind == StreamItemKind::ToolCall
                        && patch.op == StreamItemOp::AppendChunk
                        && patch.text.is_some()
            )
        }),
        ServerMessage::StreamItem(_)
    ));

    assert!(matches!(
        host.wait_for("read tool result", |message| {
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
                            && name == "read"
                            && details["content"] == "before\nafter"
                    )
            )
        }),
        ServerMessage::TranscriptCommitted(_)
    ));
    host.wait_for_gateway("exercise the real tool path", 2);
    host.wait_completed(&session_id);

    let snapshot = host.snapshot(&session_id, "snapshot");
    assert!(snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Message(entry)
                if matches!(&entry.message, Message::ToolResult { tool_call_id, .. } if tool_call_id == &call_id)
        )
    }));
    assert!(snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Message(entry)
                if entry.source_turn_id == turn_id
        )
    }));
}

#[test]
fn workspace_write_requires_approval_and_emits_file_diff() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("write");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    submit(&mut host, &session_id, &agent_instance_id, "submit");
    let turn_id = started_turn(&mut host, &session_id);

    let approval_id = match host.wait_for("write approval", |message| {
        matches!(
            message,
            ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
                tool_name, ..
            }) if tool_name == "write"
        )
    }) {
        ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
            approval_id,
            tool_args,
            ..
        }) => {
            assert_eq!(tool_args["path"], "e2e-output.txt");
            approval_id
        }
        _ => unreachable!(),
    };

    let pending = host.snapshot(&session_id, "pending");
    assert!(
        pending.pending_approvals.iter().any(|approval| {
            approval.approval_id == approval_id && approval.tool_name == "write"
        })
    );

    host.send(Command::ApprovalRespond {
        command_id: "approve".into(),
        session_id: session_id.clone(),
        approval_id: approval_id.clone(),
        decision: ApprovalDecision::Accept,
        note: Some("approved by e2e".into()),
    });
    assert!(matches!(
        host.command_result("approve"),
        CommandResult::Empty
    ));
    assert!(matches!(
        host.wait_for("write approval resolved", |message| {
            matches!(
                message,
                ServerMessage::Approval(piko_protocol::ApprovalEvent::Resolved {
                    approval_id: id,
                    decision: ApprovalDecision::Accept,
                    ..
                }) if id == &approval_id
            )
        }),
        ServerMessage::Approval(_)
    ));

    assert!(matches!(
        host.wait_for("write tool result", |message| {
            matches!(
                message,
                ServerMessage::TranscriptCommitted(event)
                    if matches!(
                        &event.message,
                        Message::ToolResult {
                            tool_call_id,
                            tool_name: Some(name),
                            is_error: Some(false),
                            ..
                        } if tool_call_id == "call-write" && name == "write"
                    )
            )
        }),
        ServerMessage::TranscriptCommitted(_)
    ));
    assert_eq!(
        fs::read_to_string(host.workspace().join("e2e-output.txt")).expect("written fixture"),
        "written by piko e2e\n"
    );

    let diff = host.wait_for("live turn diff", |message| {
        matches!(
            message,
            ServerMessage::TurnDiff(event)
                if event.session_id == session_id
                    && event.turn_id == turn_id
                    && event.files.iter().any(|file| file.path == "e2e-output.txt")
        )
    });
    assert!(matches!(diff, ServerMessage::TurnDiff(_)));
    host.wait_completed(&session_id);

    host.send(Command::TurnDiffGet {
        command_id: "diff-query".into(),
        session_id,
        turn_id,
    });
    let result = host.command_result("diff-query");
    assert!(matches!(
        result,
        CommandResult::TurnDiffGot { diff: Some(diff), .. }
            if diff.files.iter().any(|file| file.path == "e2e-output.txt")
    ));
}

#[test]
fn user_interaction_round_trips_answers_into_the_tool_result() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("interaction");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    submit(&mut host, &session_id, &agent_instance_id, "submit");
    started_turn(&mut host, &session_id);

    let interaction_id = match host.wait_for("interaction request", |message| {
        matches!(
            message,
            ServerMessage::Interaction(InteractionEvent::Requested { .. })
        )
    }) {
        ServerMessage::Interaction(InteractionEvent::Requested {
            interaction_id,
            questions,
            ..
        }) => {
            assert_eq!(questions.len(), 1);
            assert_eq!(questions[0].id, "answer");
            assert!(questions[0].prompt.contains("scripted turn"));
            interaction_id
        }
        _ => unreachable!(),
    };

    let pending = host.snapshot(&session_id, "pending");
    assert!(
        pending
            .pending_interactions
            .iter()
            .any(|interaction| interaction.interaction_id == interaction_id)
    );

    host.send(Command::UserInteractionRespond {
        command_id: "answer".into(),
        session_id: session_id.clone(),
        interaction_id: interaction_id.clone(),
        response: UserInteractionResponse::Submit {
            answers: vec![InteractionAnswer {
                question_id: "answer".into(),
                choice_id: "answer".into(),
                value: serde_json::json!("answer"),
                input: Some("yes".into()),
            }],
        },
    });
    assert!(matches!(
        host.command_result("answer"),
        CommandResult::Empty
    ));
    assert!(matches!(
        host.wait_for("interaction resolved", |message| {
            matches!(
                message,
                ServerMessage::Interaction(InteractionEvent::Resolved {
                    interaction_id: id,
                    status: piko_protocol::UserInteractionStatus::Submitted,
                    ..
                }) if id == &interaction_id
            )
        }),
        ServerMessage::Interaction(_)
    ));
    assert!(matches!(
        host.wait_for("interaction tool result", |message| {
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
                        } if tool_call_id == "call-interaction"
                            && name == "ask_user"
                            && details == "yes"
                    )
            )
        }),
        ServerMessage::TranscriptCommitted(_)
    ));
    host.wait_for_gateway("exercise the real tool path", 2);
    host.wait_completed(&session_id);
}

#[test]
fn todo_tool_publishes_and_persists_the_host_authoritative_projection() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("todo");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    submit(&mut host, &session_id, &agent_instance_id, "submit");
    started_turn(&mut host, &session_id);

    let update = host.wait_for("todo update", |message| {
        matches!(
            message,
            ServerMessage::TodoListUpdated(update)
                if update.todo_list.agent_instance_id == agent_instance_id
                    && update.todo_list.items.iter().any(|item| {
                        item.id == "e2e"
                            && item.content == "verify the E2E path"
                            && item.status == TodoStatus::InProgress
                    })
        )
    });
    assert!(matches!(update, ServerMessage::TodoListUpdated(_)));
    host.wait_for_gateway("exercise the real tool path", 2);
    host.wait_completed(&session_id);

    let snapshot = host.snapshot(&session_id, "snapshot");
    let list = snapshot
        .todo_lists
        .iter()
        .find(|list| list.agent_instance_id == agent_instance_id)
        .expect("todo list in session snapshot");
    assert_eq!(list.items[0].status, TodoStatus::InProgress);
    assert_eq!(list.revision, 1);
}

#[test]
fn environment_tool_reports_the_real_session_workspace() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("environment");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    let workspace = fs::canonicalize(host.workspace())
        .expect("canonicalize session workspace")
        .display()
        .to_string();
    submit(&mut host, &session_id, &agent_instance_id, "submit");
    started_turn(&mut host, &session_id);

    let environment_result = host.wait_for("environment result", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(&event.message, Message::ToolResult { tool_name: Some(name), .. } if name == "environment")
        )
    });
    assert!(
        matches!(
            &environment_result,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolResult {
                        details: Some(details),
                        is_error: Some(false),
                        ..
                    } if details["cwd"] == workspace
                )
        ),
        "unexpected environment result: {environment_result:?}"
    );
    host.wait_completed(&session_id);
}

#[test]
fn exec_command_is_approvable_and_process_list_stop_crosses_the_host_boundary() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("exec");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    submit(&mut host, &session_id, &agent_instance_id, "submit");
    started_turn(&mut host, &session_id);

    let approval_id = match host.wait_for("exec approval", |message| {
        matches!(
            message,
            ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
                tool_name, ..
            }) if tool_name == "exec_command"
        )
    }) {
        ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
            approval_id, ..
        }) => approval_id,
        _ => unreachable!(),
    };
    host.send(Command::ApprovalRespond {
        command_id: "approve-exec".into(),
        session_id: session_id.clone(),
        approval_id,
        decision: ApprovalDecision::Accept,
        note: None,
    });
    assert!(matches!(
        host.command_result("approve-exec"),
        CommandResult::Empty
    ));

    let process_id: String = match host.wait_for("running process result", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolResult {
                        tool_name: Some(name),
                        details: Some(details),
                        is_error: Some(false),
                        ..
                    } if name == "exec_command" && details["state"] == "running"
                )
        )
    }) {
        ServerMessage::TranscriptCommitted(event) => match event.message {
            Message::ToolResult {
                details: Some(details),
                ..
            } => details["session_id"]
                .as_str()
                .expect("process session id")
                .into(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    host.send(Command::ProcessList {
        command_id: "processes".into(),
    });
    let result = host.command_result("processes");
    let CommandResult::ProcessListed { processes, .. } = result else {
        panic!("expected process list");
    };
    assert!(processes.iter().any(|process| {
        process.process_id == process_id && !process.exited && process.command.contains("sleep")
    }));

    host.send(Command::ProcessStop {
        command_id: "stop-process".into(),
        process_id: process_id.clone(),
    });
    assert!(matches!(
        host.command_result("stop-process"),
        CommandResult::ProcessStopped {
            process_id: id,
            stopped: true,
            ..
        } if id == process_id
    ));
    host.wait_completed(&session_id);
}
