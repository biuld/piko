//! E2E coverage for the CQRS session read models (F-37 / D-53) at the hostd
//! process boundary: branch/tree cursor durability across restart, rebuild of
//! corrupted read models from the journal, and fork-destination generation
//! isolation.

#[path = "support/mod.rs"]
mod support;

use std::{fs, path::Path};

use piko_protocol::{
    Command, CommandResult, Message, MessageContent, ServerMessage, SessionSnapshot,
    SessionTreeEntry,
};
use support::{HostdHarness, root_agent_id, serial_guard};

fn complete_chat(host: &mut HostdHarness, session_id: &str, command_id: &str, text: &str) {
    host.send(Command::submit_follow_up(
        command_id,
        session_id,
        root_agent_id(session_id),
        MessageContent::String(text.into()),
    ));
    assert!(matches!(
        host.command_result(command_id),
        CommandResult::AgentInputSubmitted { .. }
    ));
    host.wait_completed(session_id);
}

fn message_entry<'a>(snapshot: &'a SessionSnapshot, entry_id: &str) -> &'a SessionTreeEntry {
    snapshot
        .entries
        .iter()
        .find(|entry| entry.id() == entry_id)
        .unwrap_or_else(|| panic!("entry {entry_id} in snapshot"))
}

fn user_entry(snapshot: &SessionSnapshot, text: &str) -> String {
    snapshot
        .entries
        .iter()
        .find_map(|entry| match entry {
            SessionTreeEntry::Message(entry)
                if matches!(
                    &entry.message,
                    Message::User {
                        content: MessageContent::String(value),
                        ..
                    } if value == text
                ) =>
            {
                Some(entry.id.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("user entry {text:?} in snapshot"))
}

fn parent_of(snapshot: &SessionSnapshot, entry_id: &str) -> Option<String> {
    match message_entry(snapshot, entry_id) {
        SessionTreeEntry::Message(entry) => entry.parent_id.clone(),
        other => panic!("entry {entry_id} is not a message: {other:?}"),
    }
}

fn ancestry_contains(snapshot: &SessionSnapshot, entry_id: &str, ancestor_id: &str) -> bool {
    let mut current = Some(entry_id.to_string());
    while let Some(id) = current {
        if id == ancestor_id {
            return true;
        }
        current = match message_entry(snapshot, &id) {
            SessionTreeEntry::Message(entry) => entry.parent_id.clone(),
            other => panic!("entry {id} is not a message: {other:?}"),
        };
    }
    false
}

fn snapshot_has_user_message(snapshot: &SessionSnapshot, text: &str) -> bool {
    snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            SessionTreeEntry::Message(entry)
                if matches!(
                    &entry.message,
                    Message::User {
                        content: MessageContent::String(value),
                        ..
                    } if value == text
                )
        )
    })
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

fn read_json_file(path: &Path, label: &str) -> serde_json::Value {
    let data = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{label} readable at {}: {error}", path.display()));
    serde_json::from_str(&data).unwrap_or_else(|error| panic!("{label} is valid JSON: {error}"))
}

fn readmodels_dir(session_path: &str) -> std::path::PathBuf {
    Path::new(session_path).join("readmodels")
}

fn poison_earliest_journal_record(session_path: &str) {
    let events_dir = Path::new(session_path).join("events");
    let mut segments = fs::read_dir(&events_dir)
        .expect("read journal segments")
        .map(|entry| entry.expect("journal segment entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    segments.sort();
    let first = segments.first().expect("journal segment");
    let contents = fs::read_to_string(first).expect("read journal segment");
    let mut records = contents.lines().map(str::to_string).collect::<Vec<_>>();
    assert!(records.len() > 1, "fast-path proof needs a preserved tail");
    records[0] = "{poisoned-early-record".into();
    fs::write(first, format!("{}\n", records.join("\n"))).expect("poison early journal record");
}

fn open_session_after_restart(
    host: &mut HostdHarness,
    session_id: &str,
    session_path: &str,
) -> SessionSnapshot {
    host.send(Command::SessionOpen {
        command_id: "open-after-restart".into(),
        session_id: session_id.to_string(),
        session_path: Some(session_path.to_string()),
    });
    assert!(matches!(
        host.command_result("open-after-restart"),
        CommandResult::SessionOpened {
            session_id: opened,
            ..
        } if opened == session_id
    ));
    match host.wait_for("reopen reconciliation", |message| {
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
fn branch_tree_and_cursor_survive_restart_across_the_read_model_fast_path() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit-1", "turn one");

    let before = host.snapshot(&session_id, "snapshot-turn-one");
    let first = user_entry(&before, "turn one");

    host.send(Command::SessionNavigate {
        command_id: "navigate".into(),
        session_id: session_id.clone(),
        entry_id: first.clone(),
        summarize: false,
        custom_instructions: None,
    });
    let base = match host.command_result("navigate") {
        CommandResult::SessionNavigated {
            selected_entry_id,
            new_leaf_id,
            editor_text,
            ..
        } => {
            assert_eq!(selected_entry_id, first);
            assert_eq!(editor_text.as_deref(), Some("turn one"));
            new_leaf_id.expect("navigate rewinds the cursor below the selected user entry")
        }
        other => panic!("expected navigation, got {other:?}"),
    };
    let rewound = host.snapshot(&session_id, "snapshot-rewound");
    assert_eq!(rewound.current_leaf_id.as_deref(), Some(base.as_str()));

    complete_chat(&mut host, &session_id, "submit-2", "turn two");
    let branched = host.snapshot(&session_id, "snapshot-branch");
    let second = user_entry(&branched, "turn two");
    assert_ne!(second, first, "the follow-up forks a new sibling branch");
    assert!(
        ancestry_contains(&branched, &second, &base),
        "the new branch descends from the navigate target"
    );
    assert_eq!(
        parent_of(&branched, &first).as_deref(),
        Some(base.as_str()),
        "the original branch shares the navigate target"
    );
    let live_cursor = branched
        .current_leaf_id
        .clone()
        .expect("cursor after branching");
    assert!(
        ancestry_contains(&branched, &live_cursor, &base),
        "the live cursor moved onto the new branch"
    );

    let session_path = session_path_of(&mut host, &session_id, "list-before-restart");
    poison_earliest_journal_record(&session_path);
    host.restart();
    let reopened = open_session_after_restart(&mut host, &session_id, &session_path);
    let restored_cursor = reopened
        .current_leaf_id
        .clone()
        .expect("cursor restored from the published read model");
    assert!(
        ancestry_contains(&reopened, &restored_cursor, &base),
        "the restored cursor is on the new branch, not the abandoned one"
    );
    assert!(ancestry_contains(&reopened, &second, &base));
    assert_eq!(parent_of(&reopened, &first).as_deref(), Some(base.as_str()));

    host.restart();
    let reopened_again = open_session_after_restart(&mut host, &session_id, &session_path);
    assert_eq!(
        reopened_again.current_leaf_id, reopened.current_leaf_id,
        "consecutive reopens serve a stable cursor from the read model fast path"
    );
}

#[test]
fn corrupted_read_models_are_rebuilt_from_the_journal_and_republished() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit", "survive corruption");
    let session_path = session_path_of(&mut host, &session_id, "list");
    let readmodels = readmodels_dir(&session_path);

    fs::write(readmodels.join("current.json"), "{corrupted").expect("corrupt current.json");
    fs::write(readmodels.join("trajectory.json"), "{corrupted").expect("corrupt trajectory.json");
    fs::remove_file(readmodels.join("head.json")).expect("remove head.json watermark");

    host.restart();
    let reopened = open_session_after_restart(&mut host, &session_id, &session_path);
    assert!(
        snapshot_has_user_message(&reopened, "survive corruption"),
        "history is rebuilt from the journal after the read models were destroyed"
    );

    let head = read_json_file(&readmodels.join("head.json"), "republished head.json");
    assert_eq!(head["sessionId"], session_id, "head republished");
    let current = read_json_file(&readmodels.join("current.json"), "republished current.json");
    assert_eq!(current["sessionId"], session_id, "current republished");
    assert_eq!(
        current["throughRevision"], head["revision"],
        "current model is anchored to the head watermark"
    );
    assert_eq!(
        current["throughChecksum"], head["checksum"],
        "current model is checksum-anchored to the head watermark"
    );
    for name in ["catalog.json", "trajectory.json"] {
        let model = read_json_file(&readmodels.join(name), &format!("republished {name}"));
        assert_eq!(model["sessionId"], session_id, "{name} republished");
        assert_eq!(
            model["throughRevision"], head["revision"],
            "{name} is anchored to the head revision"
        );
        assert_eq!(
            model["throughChecksum"], head["checksum"],
            "{name} is anchored to the head checksum"
        );
    }
}

#[test]
fn fork_destination_publishes_read_models_under_its_own_journal_generation() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit", "fork me");
    let source_path = session_path_of(&mut host, &session_id, "list-source");

    host.send(Command::SessionFork {
        command_id: "fork".into(),
        session_id: session_id.clone(),
        entry_id: None,
    });
    let forked_id = match host.command_result("fork") {
        CommandResult::SessionOpened { session_id, .. } => session_id,
        other => panic!("expected fork to open a new session, got {other:?}"),
    };
    host.wait_for("fork reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == forked_id
        )
    });
    let forked_path = session_path_of(&mut host, &forked_id, "list-forked");

    let source_head = read_json_file(
        &readmodels_dir(&source_path).join("head.json"),
        "source head.json",
    );
    let forked_head = read_json_file(
        &readmodels_dir(&forked_path).join("head.json"),
        "forked head.json",
    );
    let source_identity = read_json_file(
        Path::new(&source_path).join("session.json").as_path(),
        "source session.json",
    );
    let forked_identity = read_json_file(
        Path::new(&forked_path).join("session.json").as_path(),
        "forked session.json",
    );

    assert_eq!(source_head["sessionId"], session_id);
    assert_eq!(forked_head["sessionId"], forked_id);
    assert_eq!(source_identity["sessionId"], session_id);
    assert_eq!(forked_identity["sessionId"], forked_id);
    assert_ne!(
        source_head["journalGeneration"], forked_head["journalGeneration"],
        "the fork journal mints its own generation; copied read models would be rejected"
    );
    assert_eq!(
        source_head["journalGeneration"], source_identity["journalGeneration"],
        "source read models anchor to the source journal generation"
    );
    assert_eq!(
        forked_head["journalGeneration"], forked_identity["journalGeneration"],
        "forked read models anchor to the forked journal generation"
    );
}
