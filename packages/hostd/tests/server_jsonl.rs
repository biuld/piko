use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hostd::api::{ApprovalDecision, Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::server::{HostServer, run_jsonl_server};
use hostd::turn_runner::{TurnRunInput, TurnRunOutput, TurnRunner};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedSender;

struct SlowRunner;

#[async_trait]
impl TurnRunner for SlowRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, hostd::api::ProtocolError> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = input;
        Ok(TurnRunOutput {
            events: Vec::new(),
            total_tasks: 1,
        })
    }
}

#[tokio::test]
async fn command_catalog_get_returns_slash_commands() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::CommandCatalogGet {
            command_id: "catalog-1".into(),
        })
        .await;

    let [Event::CommandResult(hostd::api::CommandResult::CommandCatalogListed { commands, .. })] =
        events.as_slice()
    else {
        panic!("expected command catalog event, got {events:?}");
    };
    assert!(commands.iter().any(|command| command.slash_name == "/help"));
}

struct AssistantRunner;

#[async_trait]
impl TurnRunner for AssistantRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, hostd::api::ProtocolError> {
        let assistant = Event::Message(hostd::api::MessageEvent::AssistantCompleted {
            session_id: input.session_id.clone(),
            message_id: "assistant-1".into(),
            task_id: input.turn_id.clone(),
            agent_id: "agent-1".into(),
            message: Message::Assistant {
                content: vec![piko_protocol::ContentBlock::Text {
                    text: "world".into(),
                }],
                api: "test".into(),
                provider: "test-provider".into(),
                model: "test-model".into(),
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: Some(3),
            },
        });
        Ok(TurnRunOutput {
            events: vec![assistant],
            total_tasks: 1,
        })
    }
}

struct WaitingApprovalRunner {
    started: Arc<Notify>,
    finish: Arc<Notify>,
}

#[async_trait]
impl TurnRunner for WaitingApprovalRunner {
    async fn run_turn(
        &self,
        _input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, hostd::api::ProtocolError> {
        self.started.notify_one();
        self.finish.notified().await;
        Ok(TurnRunOutput {
            events: Vec::new(),
            total_tasks: 1,
        })
    }

    async fn respond_approval(
        &self,
        _approval_id: &str,
        _decision: ApprovalDecision,
    ) -> Result<bool, hostd::api::ProtocolError> {
        Ok(true)
    }
}

#[tokio::test]
async fn turn_submit_streams_started_before_runner_finishes() {
    let server = HostServer::with_turn_runner(Arc::new(SlowRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { session_id, .. }) => {
            session_id.clone()
        }
        other => panic!("expected session_created, got {other:?}"),
    };

    let mut events = server.handle_command_stream(Command::TurnSubmit {
        command_id: "submit".into(),
        session_id,
        text: "hello".into(),
    });

    let started = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        started,
        Event::Turn(hostd::api::TurnEvent::Started { .. })
    ));
}

#[tokio::test]
async fn approval_response_is_not_blocked_by_active_turn() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
    }));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { session_id, .. }) => {
            session_id.clone()
        }
        other => panic!("expected session_created, got {other:?}"),
    };

    let mut events = server.handle_command_stream(Command::TurnSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        text: "hello".into(),
    });

    let first = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        first,
        Event::Turn(hostd::api::TurnEvent::Started { .. })
    ));
    tokio::time::timeout(Duration::from_millis(50), started.notified())
        .await
        .unwrap();

    let approval_events = tokio::time::timeout(
        Duration::from_millis(50),
        server.handle_command(Command::ApprovalRespond {
            command_id: "approval".into(),
            session_id,
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        }),
    )
    .await
    .expect("approval response should not wait for the active turn to finish");

    assert!(matches!(
        approval_events.first(),
        Some(Event::Approval(hostd::api::ApprovalEvent::Resolved { .. }))
    ));

    finish.notify_one();
}

#[tokio::test]
async fn create_session_returns_session_created() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::SessionCreate {
            command_id: "cmd-1".into(),
            cwd: "/tmp/project".into(),
        })
        .await;

    assert!(!events.is_empty());
    assert!(matches!(
        events[0],
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { .. })
    ));
}

#[tokio::test]
async fn turn_submit_persists_completed_assistant_as_session_entry() {
    let server = HostServer::with_turn_runner(Arc::new(AssistantRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { session_id, .. }) => {
            session_id.clone()
        }
        other => panic!("expected session_created, got {other:?}"),
    };

    let _ = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;

    let Event::CommandResult(hostd::api::CommandResult::StateSnapshot { snapshot, .. }) =
        &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    assert_eq!(snapshot.entries.len(), 2);
    assert!(matches!(
        &snapshot.entries[0],
        hostd::api::SessionTreeEntry::Message(entry)
            if matches!(entry.message, hostd::api::Message::User { .. })
    ));
    assert!(matches!(
        &snapshot.entries[1],
        hostd::api::SessionTreeEntry::Message(entry)
            if entry.id == "assistant-1"
                && matches!(entry.message, hostd::api::Message::Assistant { .. })
    ));
}

#[tokio::test]
async fn in_memory_session_navigate_to_root_user_appends_leaf_and_resets_current_leaf() {
    let server = HostServer::new();
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { session_id, .. }) => {
            session_id.clone()
        }
        other => panic!("expected session_created, got {other:?}"),
    };

    let _ = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let Event::CommandResult(hostd::api::CommandResult::StateSnapshot { snapshot, .. }) =
        &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    let root_user_id = snapshot.entries[0].id().to_string();

    let navigated = server
        .handle_command(Command::SessionNavigate {
            command_id: "navigate".into(),
            session_id: session_id.clone(),
            entry_id: root_user_id.clone(),
            summarize: false,
            custom_instructions: None,
        })
        .await;

    assert!(matches!(
        &navigated[0],
        Event::CommandResult(hostd::api::CommandResult::SessionNavigated {
            new_leaf_id: None,
            selected_entry_id,
            editor_text: Some(text),
            ..
        }) if selected_entry_id == &root_user_id && text == "hello"
    ));
    let Event::CommandResult(hostd::api::CommandResult::SessionOpened { snapshot, .. }) =
        &navigated[1]
    else {
        panic!("expected session opened");
    };
    assert_eq!(snapshot.current_leaf_id, None);
    assert!(matches!(
        snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
    ));
}

#[tokio::test]
async fn jsonl_server_round_trips_events() {
    let input = serde_json::to_string(&Command::SessionCreate {
        command_id: "create".into(),
        cwd: "/tmp/project".into(),
    })
    .unwrap()
        + "\n";
    let (mut read_out, write_out) = tokio::io::duplex(4096);
    run_jsonl_server(
        std::io::Cursor::new(input.into_bytes()),
        write_out,
        HostServer::new(),
    )
    .await
    .unwrap();

    let mut output = String::new();
    read_out.read_to_string(&mut output).await.unwrap();
    let mut lines = output.lines();
    let ack = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(ack, Event::CommandAccepted { .. }));

    let event = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(
        event,
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { .. })
    ));
}

#[tokio::test]
async fn jsonl_server_reads_next_command_while_turn_is_running() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
    }));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResult(hostd::api::CommandResult::SessionCreated { session_id, .. }) => {
            session_id.clone()
        }
        other => panic!("expected session_created, got {other:?}"),
    };

    let (client_in, server_in) = tokio::io::duplex(4096);
    let (server_out, client_out) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        run_jsonl_server(BufReader::new(server_in), server_out, server)
            .await
            .unwrap();
    });
    let mut writer = client_in;
    let mut reader = BufReader::new(client_out);

    let submit = serde_json::to_string(&Command::TurnSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        text: "hello".into(),
    })
    .unwrap();
    writer.write_all(submit.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();

    let mut line = String::new();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_submit ack should arrive")
        .unwrap();
    let ack = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(ack, Event::CommandAccepted { .. }));

    line.clear();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_started should arrive")
        .unwrap();
    let event = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        event,
        Event::Turn(hostd::api::TurnEvent::Started { .. })
    ));

    let approval = serde_json::to_string(&Command::ApprovalRespond {
        command_id: "approval".into(),
        session_id,
        approval_id: "approval-1".into(),
        decision: ApprovalDecision::Accept,
        note: None,
    })
    .unwrap();
    writer.write_all(approval.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut saw_approval_ack = false;
    let mut saw_approval_event = false;
    for _ in 0..10 {
        line.clear();
        tokio::time::timeout(Duration::from_millis(500), reader.read_line(&mut line))
            .await
            .expect("approval output should not wait for turn completion")
            .unwrap();
        let value = serde_json::from_str::<serde_json::Value>(line.trim()).unwrap();
        if value.get("kind").and_then(|v| v.as_str()) == Some("command_accepted")
            && value.get("command_id").and_then(|v| v.as_str()) == Some("approval")
        {
            saw_approval_ack = true;
        }
        if value.get("kind").and_then(|v| v.as_str()) == Some("approval")
            && value.get("type").and_then(|v| v.as_str()) == Some("resolved")
        {
            saw_approval_event = true;
        }
        if saw_approval_ack && saw_approval_event {
            break;
        }
    }
    assert!(saw_approval_ack);
    assert!(saw_approval_event);

    finish.notify_one();
    drop(writer);
    server_task.await.unwrap();
}
