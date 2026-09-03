//! Cross-process branching behavior at the hostd/orchd/model boundary.

#[path = "support/mod.rs"]
mod support;

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
    snapshot
        .entries
        .iter()
        .find(|entry| entry.id() == entry_id)
        .and_then(|entry| entry.parent_id().map(str::to_string))
}

#[test]
fn backtrack_excludes_abandoned_messages_from_model_history() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create-backtrack");
    complete_chat(&mut host, &session_id, "submit-old-1", "abandoned one");
    complete_chat(&mut host, &session_id, "submit-old-2", "abandoned two");
    let before = host.snapshot(&session_id, "snapshot-before-backtrack");
    let first_user = user_entry(&before, "abandoned one");
    let expected_base = parent_of(&before, &first_user);

    host.send(Command::SessionNavigate {
        command_id: "navigate-backtrack".into(),
        session_id: session_id.clone(),
        entry_id: first_user,
        summarize: false,
        custom_instructions: None,
    });
    let navigation = host.command_result("navigate-backtrack");
    assert!(
        matches!(
            &navigation,
            CommandResult::SessionNavigated { new_leaf_id, .. }
                if new_leaf_id == &expected_base
        ),
        "expected backtrack to the first user's parent, got {navigation:?}"
    );
    host.wait_for("backtrack reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.current_leaf_id == expected_base
        )
    });

    complete_chat(&mut host, &session_id, "submit-fresh", "fresh root");
    let trace = host.gateway_trace();
    let request = trace
        .iter()
        .find(|record| record["value"]["step"].as_u64() == Some(3))
        .expect("third gateway request");
    assert_eq!(
        request["value"]["user_messages"],
        serde_json::json!(["fresh root"]),
        "backtracking must not replay the abandoned branch"
    );
}

#[test]
fn branch_summary_is_injected_into_the_continuation_context() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("compact");
    let session_id = host.create_session("create-branch-summary");
    complete_chat(
        &mut host,
        &session_id,
        "submit-summary-1",
        "summary source one",
    );
    complete_chat(
        &mut host,
        &session_id,
        "submit-summary-2",
        "summary source two",
    );
    let before = host.snapshot(&session_id, "snapshot-before-summary");
    let first_user = user_entry(&before, "summary source one");

    host.send(Command::SessionNavigate {
        command_id: "navigate-with-summary".into(),
        session_id: session_id.clone(),
        entry_id: first_user,
        summarize: true,
        custom_instructions: None,
    });
    let summary_id = match host.command_result("navigate-with-summary") {
        CommandResult::SessionNavigated {
            summary_entry: Some(SessionTreeEntry::BranchSummary(summary)),
            new_leaf_id,
            ..
        } => {
            assert_eq!(new_leaf_id.as_deref(), Some(summary.id.as_str()));
            summary.id
        }
        other => panic!("expected summarized navigation, got {other:?}"),
    };
    host.wait_for("summary reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event)
                if event.session_id == session_id
                    && event.snapshot.current_leaf_id.as_deref() == Some(summary_id.as_str())
        )
    });

    complete_chat(
        &mut host,
        &session_id,
        "submit-after-summary",
        "continue summarized branch",
    );
    let trace = host.gateway_trace();
    let request = trace
        .iter()
        .find(|record| record["value"]["step"].as_u64() == Some(4))
        .expect("post-summary gateway request");
    let contexts = request["value"]["context_messages"]
        .as_array()
        .expect("logged context messages");
    assert!(contexts.iter().any(|context| {
        context["source"]["kind"] == "branch_summary"
            && context["content"]
                .as_str()
                .is_some_and(|text| text.contains("turn complete"))
    }));
}
