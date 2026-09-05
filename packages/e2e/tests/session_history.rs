//! Cross-process Session History queries (F-52 / D-69).
//!
//! After hostd restart, aligned history reads must not open or replay journal
//! segments.

#[path = "support/mod.rs"]
mod support;

use std::fs;

use piko_protocol::{Command, CommandResult};
use support::{HostdHarness, root_agent_id, serial_guard};

fn complete_chat(host: &mut HostdHarness, session_id: &str, command_id: &str, text: &str) {
    host.send(Command::submit_follow_up(
        command_id,
        session_id,
        root_agent_id(session_id),
        piko_protocol::MessageContent::String(text.into()),
    ));
    assert!(matches!(
        host.command_result(command_id),
        CommandResult::AgentInputSubmitted { .. }
    ));
    host.wait_completed(session_id);
}

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

#[test]
fn history_survives_hostd_restart_without_journal_segments() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create-history");
    complete_chat(
        &mut host,
        &session_id,
        "submit-history",
        "inspect after restart",
    );

    let before = history_overview(&mut host, &session_id, "history-before");
    assert_eq!(before.session_id, session_id);
    assert!(
        before
            .works
            .iter()
            .any(|work| work.input_preview.contains("inspect after restart")),
        "live history contains the submitted work"
    );
    let session_path = session_path_of(&mut host, &session_id, "list-before-restart");

    host.restart();
    let events = std::path::Path::new(&session_path).join("events");
    fs::rename(
        &events,
        std::path::Path::new(&session_path).join("hidden-events"),
    )
    .expect("hide journal segments after restart");

    let after = history_overview(&mut host, &session_id, "history-after");
    assert_eq!(after.session_id, before.session_id);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.works, before.works);
    assert_eq!(after.agents, before.agents);

    let root = after
        .works
        .first()
        .map(|work| work.root_input_id.clone())
        .expect("root work after restart");
    host.send(Command::SessionHistoryWorkPageGet {
        command_id: "work-after".into(),
        session_id: session_id.clone(),
        root_input_id: root,
        expected_revision: after.revision,
        after_cursor: None,
        limit: Some(50),
    });
    match host.command_result("work-after") {
        CommandResult::SessionHistoryWorkPaged { page, .. } => {
            assert!(!page.items.is_empty());
            assert!(
                page.items
                    .iter()
                    .all(|item| item.item_ref.revision == after.revision)
            );
        }
        other => panic!("expected work page, got {other:?}"),
    }

    host.send(Command::SessionHistoryJournalPageGet {
        command_id: "journal-after".into(),
        session_id: session_id.clone(),
        expected_revision: after.revision,
        after_cursor: None,
        limit: Some(50),
        provenance: piko_protocol::HistoryProvenanceFilter::All,
    });
    match host.command_result("journal-after") {
        CommandResult::SessionHistoryJournalPaged { page, .. } => {
            assert!(!page.commits.is_empty());
            assert!(page.commits.iter().all(|commit| {
                commit
                    .events
                    .iter()
                    .all(|item| item.item_ref.revision == after.revision)
            }));
        }
        other => panic!("expected journal page, got {other:?}"),
    }
}
