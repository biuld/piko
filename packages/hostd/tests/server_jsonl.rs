use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hostd::api::{Command, CommandAck, Event};
use hostd::server::{HostServer, run_jsonl_server};
use hostd::state::HostState;
use hostd::turn_runner::{TurnRunInput, TurnRunOutput, TurnRunner};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc::UnboundedSender;

struct SlowRunner;

#[async_trait]
impl TurnRunner for SlowRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        state: &mut HostState,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, hostd::api::ProtocolError> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let complete = state.complete_turn(&input.session_id, &input.turn_id)?;
        Ok(TurnRunOutput {
            events: vec![complete],
        })
    }
}

struct AssistantRunner;

#[async_trait]
impl TurnRunner for AssistantRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        state: &mut HostState,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, hostd::api::ProtocolError> {
        let assistant = Event::AssistantMessageCompleted {
            session_id: input.session_id.clone(),
            message_id: "assistant-1".into(),
            task_id: input.turn_id.clone(),
            agent_id: "agent-1".into(),
            text: "world".into(),
            tool_calls: Vec::new(),
            model: "test-model".into(),
            provider: "test-provider".into(),
            usage: None,
            timestamp: 3,
        };
        let complete = state.complete_turn(&input.session_id, &input.turn_id)?;
        Ok(TurnRunOutput {
            events: vec![assistant, complete],
        })
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
        Event::SessionCreated { session_id, .. } => session_id.clone(),
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

    assert!(matches!(started, Event::TurnStarted { .. }));
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
    assert!(matches!(events[0], Event::SessionCreated { .. }));
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
        Event::SessionCreated { session_id, .. } => session_id.clone(),
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

    let Event::StateSnapshot { snapshot, .. } = &snapshot[0] else {
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
async fn jsonl_server_round_trips_events() {
    let input = serde_json::to_string(&Command::SessionCreate {
        command_id: "create".into(),
        cwd: "/tmp/project".into(),
    })
    .unwrap()
        + "\n";
    let (mut read_out, write_out) = tokio::io::duplex(4096);
    run_jsonl_server(
        BufReader::new(input.as_bytes()),
        write_out,
        HostServer::new(),
    )
    .await
    .unwrap();

    let mut output = String::new();
    read_out.read_to_string(&mut output).await.unwrap();
    let mut lines = output.lines();
    let ack = serde_json::from_str::<CommandAck>(lines.next().unwrap()).unwrap();
    assert!(matches!(ack, CommandAck::CommandAccepted { .. }));

    let event = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(event, Event::SessionCreated { .. }));
}
