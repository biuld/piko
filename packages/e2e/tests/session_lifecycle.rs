#[path = "support/mod.rs"]
mod support;

use std::{fs, path::Path};

use piko_protocol::{
    Command, CommandResult, CompactMode, Message, ServerMessage, SessionSnapshot, TurnEvent,
};
use support::{HostdHarness, root_agent_id, serial_guard};

fn complete_chat(host: &mut HostdHarness, session_id: &str, command_id: &str, text: &str) {
    host.send(Command::ChatSubmit {
        command_id: command_id.into(),
        session_id: session_id.into(),
        target_agent_instance_id: root_agent_id(session_id),
        text: text.into(),
    });
    assert!(matches!(
        host.command_result(command_id),
        CommandResult::Empty
    ));
    host.wait_for("completed chat", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Completed { session_id: id, .. })
                if id == session_id
        )
    });
}

fn user_entry(snapshot: &SessionSnapshot, text: &str) -> String {
    snapshot
        .entries
        .iter()
        .find_map(|entry| match entry {
            piko_protocol::SessionTreeEntry::Message(entry)
                if matches!(
                    &entry.message,
                    Message::User {
                        content: piko_protocol::MessageContent::String(value),
                        ..
                    } if value == text
                ) =>
            {
                Some(entry.id.clone())
            }
            _ => None,
        })
        .expect("user entry in snapshot")
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create import source");
    for entry in fs::read_dir(source).expect("read source session") {
        let entry = entry.expect("read source entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &destination_path);
        } else {
            fs::copy(source_path, destination_path).expect("copy source session file");
        }
    }
}

#[test]
fn session_rename_label_fork_navigate_delete_round_trip_over_jsonl() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit", "session lifecycle");
    let snapshot = host.snapshot(&session_id, "snapshot-before-mutate");
    let user_id = user_entry(&snapshot, "session lifecycle");

    host.send(Command::SessionRename {
        command_id: "rename".into(),
        session_id: session_id.clone(),
        name: "E2E session".into(),
    });
    assert!(matches!(
        host.command_result("rename"),
        CommandResult::Empty
    ));
    host.wait_for("rename reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });

    host.send(Command::SessionSetLabel {
        command_id: "label".into(),
        session_id: session_id.clone(),
        entry_id: user_id.clone(),
        label: Some("important".into()),
    });
    assert!(matches!(host.command_result("label"), CommandResult::Empty));
    host.wait_for("label reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });

    let renamed = host.snapshot(&session_id, "snapshot-renamed");
    assert_eq!(renamed.name.as_deref(), Some("E2E session"));
    assert!(renamed.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Label(label)
                if label.target_id == user_id && label.label.as_deref() == Some("important")
        )
    }));

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

    host.send(Command::SessionList {
        command_id: "list-forked".into(),
        scope: piko_protocol::SessionListScope::All,
        cwd: None,
    });
    let listed = host.command_result("list-forked");
    assert!(matches!(
        listed,
        CommandResult::SessionListed { sessions, .. }
            if sessions.iter().any(|session| session.session_id == session_id)
                && sessions.iter().any(|session| session.session_id == forked_id)
    ));

    host.send(Command::SessionDelete {
        command_id: "delete-fork".into(),
        session_id: forked_id.clone(),
    });
    assert!(matches!(
        host.command_result("delete-fork"),
        CommandResult::Empty
    ));
    assert!(matches!(
        host.wait_for("fork cleared", |message| {
            matches!(
                message,
                ServerMessage::SessionCleared(event) if event.previous_session_id == forked_id
            )
        }),
        ServerMessage::SessionCleared(_)
    ));

    host.send(Command::SessionNavigate {
        command_id: "navigate".into(),
        session_id: session_id.clone(),
        entry_id: user_id.clone(),
        summarize: false,
        custom_instructions: None,
    });
    let navigation = host.command_result("navigate");
    assert!(matches!(
        navigation,
        CommandResult::SessionNavigated {
            selected_entry_id,
            editor_text: Some(text),
            new_leaf_id: Some(_),
            ..
        } if selected_entry_id == user_id && text == "session lifecycle"
    ));
    let navigated = host.wait_for("navigate reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });
    assert!(matches!(
        navigated,
        ServerMessage::SessionReconciled(event) if event.snapshot.current_leaf_id.is_some()
    ));

    host.send(Command::SessionDelete {
        command_id: "delete-original".into(),
        session_id: session_id.clone(),
    });
    assert!(matches!(
        host.command_result("delete-original"),
        CommandResult::Empty
    ));
    assert!(matches!(
        host.wait_for("original cleared", |message| {
            matches!(
                message,
                ServerMessage::SessionCleared(event) if event.previous_session_id == session_id
            )
        }),
        ServerMessage::SessionCleared(_)
    ));
}

#[test]
fn new_context_window_compaction_rewrites_durable_history_without_model_summary() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit-1", "first context");
    complete_chat(&mut host, &session_id, "submit-2", "second context");
    let before = host.snapshot(&session_id, "before-compact");
    let second_user_id = user_entry(&before, "second context");

    host.send(Command::SessionCompact {
        command_id: "compact".into(),
        session_id: session_id.clone(),
        agent_instance_id: root_agent_id(&session_id),
        mode: CompactMode::NewContextWindow,
    });
    assert!(matches!(
        host.command_result("compact"),
        CommandResult::Empty
    ));
    let reconciled = host.wait_for("compact reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });
    let ServerMessage::SessionReconciled(event) = reconciled else {
        unreachable!();
    };
    let compaction = event
        .snapshot
        .entries
        .iter()
        .find_map(|entry| match entry {
            piko_protocol::SessionTreeEntry::Compaction(compaction) => Some(compaction),
            _ => None,
        })
        .expect("compaction checkpoint in reconciled snapshot");
    assert_eq!(
        compaction.first_kept_entry_id, second_user_id,
        "the latest user message anchors the new context window"
    );
    assert_eq!(
        compaction.summary,
        "A new context window was started without summarizing conversation history."
    );
    assert_eq!(
        compaction.details.as_ref().expect("compact details")["trigger"],
        "new_context_window"
    );
}

#[test]
fn durable_session_reopens_with_history_after_hostd_process_restart() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit", "survive restart");

    host.send(Command::SessionList {
        command_id: "list".into(),
        scope: piko_protocol::SessionListScope::All,
        cwd: None,
    });
    let session_path = match host.command_result("list") {
        CommandResult::SessionListed { sessions, .. } => sessions
            .into_iter()
            .find(|session| session.session_id == session_id)
            .and_then(|session| session.session_path)
            .expect("session path before restart"),
        other => panic!("expected session list, got {other:?}"),
    };
    let before = host.snapshot(&session_id, "before-restart");
    assert!(before.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Message(entry)
                if matches!(
                    &entry.message,
                    Message::User {
                        content: piko_protocol::MessageContent::String(text),
                        ..
                    } if text == "survive restart"
                )
        )
    }));

    host.restart();
    host.send(Command::SessionOpen {
        command_id: "open-after-restart".into(),
        session_id: session_id.clone(),
        session_path: Some(session_path),
    });
    assert!(matches!(
        host.command_result("open-after-restart"),
        CommandResult::SessionOpened {
            session_id: opened,
            ..
        } if opened == session_id
    ));
    let reopened = host.wait_for("reopen reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });
    assert!(matches!(
        reopened,
        ServerMessage::SessionReconciled(event)
            if event.snapshot.entries.iter().any(|entry| {
                matches!(
                    entry,
                    piko_protocol::SessionTreeEntry::Message(entry)
                        if matches!(
                            &entry.message,
                            Message::User {
                                content: piko_protocol::MessageContent::String(text),
                                ..
                            } if text == "survive restart"
                        )
                )
            })
    ));
}

#[test]
fn session_import_copies_a_durable_session_and_rehydrates_it_over_jsonl() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    complete_chat(&mut host, &session_id, "submit", "import me");

    host.send(Command::SessionList {
        command_id: "list".into(),
        scope: piko_protocol::SessionListScope::All,
        cwd: None,
    });
    let source_path = match host.command_result("list") {
        CommandResult::SessionListed { sessions, .. } => sessions
            .into_iter()
            .find(|session| session.session_id == session_id)
            .and_then(|session| session.session_path)
            .expect("source session path"),
        other => panic!("expected session list, got {other:?}"),
    };
    let import_source = host
        .workspace()
        .parent()
        .expect("e2e root")
        .join("import-source");
    copy_directory(Path::new(&source_path), &import_source);

    host.restart();
    host.send(Command::SessionImport {
        command_id: "import".into(),
        path: import_source.display().to_string(),
    });
    assert!(matches!(
        host.command_result("import"),
        CommandResult::SessionOpened {
            session_id: imported,
            ..
        } if imported == session_id
    ));
    let imported = host.wait_for("import reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });
    assert!(matches!(
        imported,
        ServerMessage::SessionReconciled(event)
            if event.snapshot.entries.iter().any(|entry| {
                matches!(
                    entry,
                    piko_protocol::SessionTreeEntry::Message(entry)
                        if matches!(
                            &entry.message,
                            Message::User {
                                content: piko_protocol::MessageContent::String(text),
                                ..
                            } if text == "import me"
                        )
                )
            })
    ));
}
