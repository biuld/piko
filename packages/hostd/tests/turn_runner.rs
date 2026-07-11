use hostd::turn_runner::{MockTurnRunner, TurnRunInput, TurnRunner};
use tokio_stream::StreamExt;

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockTurnRunner;
    let subscription = runner
        .run_turn_subscription(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            work_id: "work-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: None,
            persist_sink: None,
            event_tx: None,
            resume_root_task: None,
        })
        .await
        .unwrap();

    let mut output = subscription.output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn mock_turn_with_storage_populates_state() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::server::HostServer;
    use hostd::session::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage(repo);

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("unexpected {other:?}"),
    };

    let turn_events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;
    for event in &turn_events {
        if let Event::CommandResponse {
            result: Err(err), ..
        } = event
        {
            panic!("turn failed: {err}");
        }
    }

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }),
        ..
    } = &snapshot[0]
    else {
        panic!("expected snapshot");
    };
    assert!(
        !snapshot.entries.is_empty(),
        "expected user message in snapshot, got {snapshot:?}"
    );
}

#[tokio::test]
async fn turn_runner_returns_streaming_events() {
    let runner = MockTurnRunner;

    let subscription = runner
        .run_turn_subscription(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            work_id: "work-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: None,
            persist_sink: None,
            event_tx: None,
            resume_root_task: None,
        })
        .await
        .unwrap();

    let mut output = subscription.output;
    assert!(output.next().await.is_some());
}
