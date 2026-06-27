use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hostd::api::{CommandAck, HostCommand, HostEvent};
use hostd::server::{HostServer, run_jsonl_server};
use hostd::state::HostState;
use hostd::turn_runner::{TurnRunInput, TurnRunOutput, TurnRunner};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc::UnboundedSender;

struct SlowRunner;

impl TurnRunner for SlowRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        state: &'a mut HostState,
        _event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<
        Box<dyn Future<Output = Result<TurnRunOutput, hostd::api::HostProtocolError>> + Send + 'a>,
    > {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let complete = state.complete_turn(&input.session_id, &input.turn_id)?;
            Ok(TurnRunOutput {
                events: vec![complete],
            })
        })
    }
}

#[tokio::test]
async fn turn_submit_streams_started_before_runner_finishes() {
    let server = HostServer::with_turn_runner(Arc::new(SlowRunner));
    let created = server
        .handle_command(HostCommand::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        HostEvent::SessionCreated { session_id, .. } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let mut events = server.handle_command_stream(HostCommand::TurnSubmit {
        command_id: "submit".into(),
        session_id,
        text: "hello".into(),
    });

    let started = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(started, HostEvent::TurnStarted { .. }));
}

#[tokio::test]
async fn create_session_returns_session_created() {
    let server = HostServer::new();
    let events = server
        .handle_command(HostCommand::SessionCreate {
            command_id: "cmd-1".into(),
            cwd: "/tmp/project".into(),
        })
        .await;

    assert!(!events.is_empty());
    assert!(matches!(events[0], HostEvent::SessionCreated { .. }));
}

#[tokio::test]
async fn jsonl_server_round_trips_events() {
    let input = serde_json::to_string(&HostCommand::SessionCreate {
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

    let event = serde_json::from_str::<HostEvent>(lines.next().unwrap()).unwrap();
    assert!(matches!(event, HostEvent::SessionCreated { .. }));
}
