#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult, CompactMode, ServerMessage, TurnEvent};
use support::{HostdHarness, root_agent_id, serial_guard};

#[test]
fn summarize_compaction_uses_the_injected_gateway_and_rehydrates_the_tree() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("compact");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: agent_instance_id.clone(),
        text: "compact me".into(),
    });
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::Empty
    ));
    host.wait_for("initial completion", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Completed { session_id: id, .. })
                if id == &session_id
        )
    });

    host.send(Command::SessionCompact {
        command_id: "compact".into(),
        session_id: session_id.clone(),
        agent_instance_id,
        mode: CompactMode::Summarize,
    });
    assert!(matches!(
        host.command_result("compact"),
        CommandResult::Empty
    ));
    host.wait_for_gateway_step(2);

    let reconciled = host.wait_for("summarized reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });
    let ServerMessage::SessionReconciled(event) = reconciled else {
        unreachable!();
    };
    assert!(event.snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Compaction(compaction)
                if compaction.summary == "history summary"
                    && compaction.details.as_ref().is_some_and(|details| {
                        details["trigger"] == "manual"
                    })
        )
    }));
}
