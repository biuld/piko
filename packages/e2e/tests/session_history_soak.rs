//! Multi-agent history soak (F-52 / D-69): spawn, approved write, compaction,
//! interrupt, then restart without journal segments.

#[path = "support/mod.rs"]
mod support;

use std::fs;

use piko_protocol::{
    AgentForeground, AgentWorkViewState, ApprovalDecision, Command, CommandResult, CompactMode,
    Message, MessageContent, ServerMessage,
};
use support::{HostdHarness, root_agent_id, serial_guard};

fn session_path_of(host: &mut HostdHarness, session_id: &str, command_id: &str) -> String {
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

fn history_overview(
    host: &mut HostdHarness,
    session_id: &str,
    command_id: &str,
) -> piko_protocol::SessionHistoryOverview {
    host.send(Command::SessionHistoryOverviewGet {
        command_id: command_id.into(),
        session_id: session_id.into(),
        after_cursor: None,
        limit: Some(50),
    });
    match host.command_result(command_id) {
        CommandResult::SessionHistoryOverviewGot { overview, .. } => overview,
        other => panic!("expected history overview, got {other:?}"),
    }
}

fn journal_kinds(
    host: &mut HostdHarness,
    session_id: &str,
    revision: u64,
    command_id: &str,
) -> Vec<String> {
    host.send(Command::SessionHistoryJournalPageGet {
        command_id: command_id.into(),
        session_id: session_id.into(),
        expected_revision: revision,
        after_cursor: None,
        limit: Some(200),
        provenance: piko_protocol::HistoryProvenanceFilter::Facts,
    });
    match host.command_result(command_id) {
        CommandResult::SessionHistoryJournalPaged { page, .. } => page
            .commits
            .into_iter()
            .flat_map(|commit| commit.events)
            .map(|item| item.kind.0)
            .collect(),
        other => panic!("expected journal page, got {other:?}"),
    }
}

fn submit(host: &mut HostdHarness, session_id: &str, agent: &str, command_id: &str, text: &str) {
    host.send(Command::submit_follow_up(
        command_id,
        session_id,
        agent,
        MessageContent::String(text.into()),
    ));
    assert!(matches!(
        host.command_result(command_id),
        CommandResult::AgentInputSubmitted { .. }
    ));
}

#[test]
fn history_soak_survives_restart_without_journal_replay() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("history-soak");
    let session_id = host.create_session("create-soak");
    let root = root_agent_id(&session_id);

    submit(
        &mut host,
        &session_id,
        &root,
        "submit-spawn",
        "delegate this subtask",
    );
    let spawn = host.wait_for("spawn_agent tool call", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(&event.message, Message::ToolCall { name, .. } if name == "spawn_agent")
        )
    });
    let spawn_id = match spawn {
        ServerMessage::TranscriptCommitted(event) => match event.message {
            Message::ToolCall { id, .. } => id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    host.wait_for("attached child report", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(
                    &event.message,
                    Message::ToolResult { tool_call_id, tool_name: Some(name), is_error: Some(false), .. }
                        if tool_call_id == &spawn_id && name == "spawn_agent"
                )
        )
    });
    host.wait_for_gateway("inspect this subtask", 2);
    host.wait_completed(&session_id);

    submit(&mut host, &session_id, &root, "submit-write", "write file");
    let approval_id = match host.wait_for("write approval", |message| {
        matches!(
            message,
            ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested { tool_name, .. })
                if tool_name == "write"
        )
    }) {
        ServerMessage::Approval(piko_protocol::ApprovalEvent::Requested {
            approval_id, ..
        }) => approval_id,
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
    host.wait_completed(&session_id);

    host.send(Command::SessionCompact {
        command_id: "compact".into(),
        session_id: session_id.clone(),
        agent_instance_id: root.clone(),
        mode: CompactMode::Summarize,
    });
    assert!(matches!(
        host.command_result("compact"),
        CommandResult::Empty
    ));
    host.wait_for("summarized reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.entries.iter().any(|entry| matches!(
                        entry,
                        piko_protocol::SessionTreeEntry::Compaction(compaction)
                            if compaction.summary == "history summary"
                    ))
        )
    });

    submit(
        &mut host,
        &session_id,
        &root,
        "submit-fail",
        "cancel this turn",
    );
    let failing = host.wait_started(&session_id);
    host.send(Command::AgentInterrupt {
        command_id: "interrupt".into(),
        session_id: session_id.clone(),
        agent_instance_id: root.clone(),
    });
    assert!(matches!(
        host.command_result("interrupt"),
        CommandResult::AgentInterrupted { accepted: true, .. }
    ));
    host.wait_for("cancelling snapshot", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.agent_work.iter().any(|work| {
                        work.foreground == AgentForeground::Cancelling
                            && work.active_work.as_ref().is_some_and(|active| {
                                active.root_input_id == failing
                                    && active.state == AgentWorkViewState::Cancelling
                            })
                    })
        )
    });
    host.wait_completed(&session_id);

    let before = history_overview(&mut host, &session_id, "overview-before");
    assert!(before.agents.len() >= 2);
    assert!(
        before.agents.iter().any(|agent| {
            agent.parent_agent_instance_id.as_deref() == Some(root.as_str())
                && agent.origin.is_some()
        }),
        "child origin is recorded"
    );
    assert!(before.works.len() >= 3);
    let kinds = journal_kinds(&mut host, &session_id, before.revision, "journal-before");
    assert!(kinds.iter().any(|kind| kind == "input"));
    assert!(kinds.iter().any(|kind| kind == "agent_origin"));
    assert!(kinds.iter().any(|kind| kind == "compaction_recorded"));
    assert!(kinds.iter().any(|kind| kind.contains("interrupt")));

    let session_path = session_path_of(&mut host, &session_id, "list-before-restart");
    host.restart();
    fs::rename(
        std::path::Path::new(&session_path).join("events"),
        std::path::Path::new(&session_path).join("hidden-events"),
    )
    .expect("hide journal segments");

    let after = history_overview(&mut host, &session_id, "overview-after");
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.works, before.works);
    assert_eq!(after.agents, before.agents);
    let after_kinds = journal_kinds(&mut host, &session_id, after.revision, "journal-after");
    assert_eq!(after_kinds, kinds);

    host.send(Command::SessionHistoryTranscriptPageGet {
        command_id: "transcript-after".into(),
        session_id: session_id.clone(),
        expected_revision: after.revision,
        after_cursor: None,
        limit: Some(200),
    });
    match host.command_result("transcript-after") {
        CommandResult::SessionHistoryTranscriptPaged { page, .. } => {
            assert!(!page.items.is_empty());
        }
        other => panic!("expected transcript page, got {other:?}"),
    }
}
