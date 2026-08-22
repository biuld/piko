use super::*;

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockAgentRunRunner;
    let subscription = runner
        .run_agent(AgentRunInput {
            session_id: "session-test".into(),
            operation_id: "turn-test".into(),
            agent_instance_id: "agent_session-test_root".into(),
            content: piko_protocol::MessageContent::String("hello".into()),
            source_turn_id: Some("turn-test".into()),
            prompt_resources: Some(piko_protocol::PromptResourceSnapshot::default()),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            resume_agent: None,
        })
        .await
        .unwrap();

    let mut process = subscription.process;
    let mut output = process.wait_started().await.unwrap().output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn mock_turn_with_storage_populates_state() {
    use piko_hostd::api::{Command, ServerMessage as Event};
    use piko_hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("unexpected {other:?}"),
    };

    let turn_events = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
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

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
    assert!(
        !snapshot.entries.is_empty(),
        "expected user message in snapshot, got {snapshot:?}"
    );
}

#[tokio::test]
async fn turn_runner_returns_streaming_events() {
    let runner = MockAgentRunRunner;

    let subscription = runner
        .run_agent(AgentRunInput {
            session_id: "session-test".into(),
            operation_id: "turn-test".into(),
            agent_instance_id: "agent_session-test_root".into(),
            content: piko_protocol::MessageContent::String("hello".into()),
            source_turn_id: Some("turn-test".into()),
            prompt_resources: Some(piko_protocol::PromptResourceSnapshot::default()),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            resume_agent: None,
        })
        .await
        .unwrap();

    let mut process = subscription.process;
    let mut output = process.wait_started().await.unwrap().output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn snapshot_required_reconciles_and_resubscribes_without_losing_turn() {
    use piko_hostd::api::{Command, ServerMessage as Event};
    use piko_hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(RecoveringAgentRunRunner::default()),
    );
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap();

    let events = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    assert!(
        events.iter().any(|event| matches!(event,
            Event::SessionReconciled(reconciled)
                if reconciled.reason == piko_protocol::ReconcileReason::RetentionExhausted
                    && reconciled.snapshot.pending_approvals.len() == 1
                    && reconciled.snapshot.active_turns.iter().any(|turn|
                        turn.status == piko_protocol::TurnStatus::WaitingForApproval)
        )),
        "events={events:?}"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}
