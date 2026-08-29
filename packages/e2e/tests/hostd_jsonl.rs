#[path = "support/mod.rs"]
mod support;

use piko_protocol::{
    Command, CommandResult, ContentBlock, Message, MessageContent, ServerMessage, TurnEvent,
};
use support::{HostdHarness, root_agent_id, serial_guard};

#[test]
fn jsonl_bootstrap_exposes_the_host_command_surfaces() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");

    host.send(Command::CommandCatalogGet {
        command_id: "catalog".into(),
    });
    let catalog = host.command_result("catalog");
    let CommandResult::CommandCatalogListed { commands, .. } = catalog else {
        panic!("expected command catalog");
    };
    assert!(commands.iter().any(|command| command.id == "session.new"));
    assert!(
        commands
            .iter()
            .any(|command| command.id == "session.compact")
    );

    host.send(Command::ModelList {
        command_id: "models".into(),
    });
    assert!(matches!(
        host.command_result("models"),
        CommandResult::ModelListed { .. }
    ));

    host.send(Command::ProcessList {
        command_id: "processes".into(),
    });
    assert!(matches!(
        host.command_result("processes"),
        CommandResult::ProcessListed { processes, .. } if processes.is_empty()
    ));

    host.send(Command::McpStatus {
        command_id: "mcp".into(),
    });
    assert!(matches!(
        host.command_result("mcp"),
        CommandResult::McpStatusListed { servers, .. } if servers.is_empty()
    ));

    host.send(Command::AgentSpecList {
        command_id: "agent-specs".into(),
    });
    let specs = host.command_result("agent-specs");
    let CommandResult::AgentSpecListed { agents, .. } = specs else {
        panic!("expected agent specs");
    };
    assert!(agents.iter().any(|agent| agent.id == "main"));
}

#[test]
fn session_config_and_agent_commands_round_trip_over_jsonl() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    let root_agent = root_agent_id(&session_id);

    host.send(Command::SessionList {
        command_id: "list".into(),
        scope: piko_protocol::SessionListScope::All,
        cwd: None,
    });
    let listed = host.command_result("list");
    let CommandResult::SessionListed { sessions, .. } = listed else {
        panic!("expected session list");
    };
    let session_path = sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .and_then(|session| session.session_path.clone())
        .expect("created session has a durable path");

    host.send(Command::SessionList {
        command_id: "list-current-folder".into(),
        scope: piko_protocol::SessionListScope::CurrentFolder,
        cwd: Some(host.workspace().display().to_string()),
    });
    assert!(matches!(
        host.command_result("list-current-folder"),
        CommandResult::SessionListed { sessions, .. }
            if sessions.iter().any(|session| session.session_id == session_id)
    ));

    host.send(Command::SessionOpen {
        command_id: "open".into(),
        session_id: session_id.clone(),
        session_path: Some(session_path),
    });
    assert!(matches!(
        host.command_result("open"),
        CommandResult::SessionOpened { .. }
    ));
    host.wait_for("open reconciliation", |message| {
        matches!(
            message,
            ServerMessage::SessionReconciled(event) if event.session_id == session_id
        )
    });

    host.send(Command::ConfigGet {
        command_id: "get-tui".into(),
        namespace: "tui".into(),
    });
    assert!(matches!(
        host.command_result("get-tui"),
        CommandResult::ConfigEntry { namespace, value }
            if namespace == "tui" && value.is_object()
    ));

    host.send(Command::ConfigUpdate {
        command_id: "update-tui".into(),
        patch: serde_json::json!({"tui":{"bottom_bar":{"items":["agent"]}}}),
    });
    assert!(matches!(
        host.command_result("update-tui"),
        CommandResult::ConfigEntry { namespace, value }
            if namespace == "tui" && value["bottom_bar"]["items"][0] == "agent"
    ));

    host.send(Command::AgentList {
        command_id: "agents".into(),
        session_id: session_id.clone(),
    });
    let agents = host.command_result("agents");
    let CommandResult::AgentListed { agents, .. } = agents else {
        panic!("expected active agent list");
    };
    assert!(
        agents
            .iter()
            .any(|agent| agent.agent_instance_id == root_agent)
    );

    host.send(Command::AgentSubscribe {
        command_id: "subscribe".into(),
        session_id: session_id.clone(),
        agent_instance_id: root_agent.clone(),
        after_seq: None,
    });
    assert!(matches!(
        host.command_result("subscribe"),
        CommandResult::AgentSubscribed { agent_instance_id, .. }
            if agent_instance_id == root_agent
    ));

    host.send(Command::AgentUnsubscribe {
        command_id: "unsubscribe".into(),
        session_id,
        agent_instance_id: root_agent,
    });
    assert!(matches!(
        host.command_result("unsubscribe"),
        CommandResult::Empty
    ));
}

#[test]
fn chat_persists_transcript_and_supports_rollout_diff_and_usage_queries() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);

    host.send(Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: agent_instance_id.clone(),
        text: "hello from jsonl".into(),
    });
    assert!(matches!(
        host.command_result("submit"),
        CommandResult::Empty
    ));
    host.wait_for("turn started", |message| {
        matches!(
            message,
            ServerMessage::TurnLifecycle(TurnEvent::Started { session_id: id, .. })
                if id == &session_id
        )
    });
    host.wait_for("user transcript", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(event.message, Message::User { .. })
        )
    });
    host.wait_for("assistant transcript", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(event.message, Message::Assistant { .. })
        )
    });
    let completed = host.wait_completed(&session_id);
    let TurnEvent::Completed { turn_id, .. } = completed else {
        panic!("expected completed turn");
    };
    host.wait_for_gateway("hello from jsonl", 1);

    let snapshot = host.snapshot(&session_id, "snapshot");
    assert!(snapshot.entries.len() >= 2);
    assert!(snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Message(message)
                if matches!(message.message, Message::User { .. })
        )
    }));
    assert!(snapshot.entries.iter().any(|entry| {
        matches!(
            entry,
            piko_protocol::SessionTreeEntry::Message(message)
                if matches!(message.message, Message::Assistant { .. })
        )
    }));
    assert!(snapshot.cumulative_usage.is_some());

    host.send(Command::RolloutPageGet {
        command_id: "page-1".into(),
        session_id: session_id.clone(),
        agent_instance_id: agent_instance_id.clone(),
        after_cursor: None,
        limit: Some(1),
    });
    let page = host.command_result("page-1");
    let CommandResult::RolloutPaged { page, .. } = page else {
        panic!("expected rollout page");
    };
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some());

    host.send(Command::TurnDiffGet {
        command_id: "diff".into(),
        session_id,
        turn_id,
    });
    assert!(matches!(
        host.command_result("diff"),
        CommandResult::TurnDiffGot { diff: None, .. }
    ));
}

#[test]
fn multimodal_submit_preserves_structured_content_at_the_host_boundary() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("immediate");
    let session_id = host.create_session("create");
    let agent_instance_id = root_agent_id(&session_id);
    let content = MessageContent::Blocks(vec![
        ContentBlock::Text {
            text: "describe this image".into(),
        },
        ContentBlock::Image {
            data: "ZmFrZS1pbWFnZQ==".into(),
            mime_type: "image/png".into(),
        },
    ]);

    host.send(Command::ChatSubmitMessage {
        command_id: "submit-message".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: agent_instance_id,
        content,
    });
    assert!(matches!(
        host.command_result("submit-message"),
        CommandResult::Empty
    ));
    let user = host.wait_for("multimodal user transcript", |message| {
        matches!(
            message,
            ServerMessage::TranscriptCommitted(event)
                if matches!(&event.message, Message::User { content: MessageContent::Blocks(blocks), .. }
                    if blocks.iter().any(|block| matches!(block, ContentBlock::Image { .. })))
        )
    });
    assert!(matches!(user, ServerMessage::TranscriptCommitted(_)));
    host.wait_completed(&session_id);
}
