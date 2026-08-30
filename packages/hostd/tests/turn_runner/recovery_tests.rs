use super::*;

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockAgentRunRunner::default();
    let receipt = AgentRunRunner::submit_agent_input(
        &runner,
        user_input("session-test", "agent_session-test_root", "turn-test"),
        piko_orchd_api::AgentInputRuntime::default(),
    )
    .await
    .expect("admission");
    let subscription = AgentRunRunner::wait_agent_input_started(
        &runner,
        "session-test",
        "agent_session-test_root",
        &receipt.input_id,
        receipt.disposition,
    )
    .await
    .unwrap();
    let mut output = subscription.output;
    output.next().await;
}

#[tokio::test]
async fn mock_turn_with_storage_populates_state() {
    use piko_hostd::api::{Command, ServerMessage as Event};
    use piko_hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner::default()));

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
        .handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("hello".into()),
        ))
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
    let runner = MockAgentRunRunner::default();
    let receipt = AgentRunRunner::submit_agent_input(
        &runner,
        user_input("session-test", "agent_session-test_root", "turn-test"),
        piko_orchd_api::AgentInputRuntime::default(),
    )
    .await
    .expect("admission");
    let subscription = AgentRunRunner::wait_agent_input_started(
        &runner,
        "session-test",
        "agent_session-test_root",
        &receipt.input_id,
        receipt.disposition,
    )
    .await
    .unwrap();
    let mut output = subscription.output;
    output.next().await;
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
        .handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;

    assert!(
        events.iter().any(|event| matches!(event,
            Event::SessionReconciled(reconciled)
                if reconciled.reason == piko_protocol::ReconcileReason::RetentionExhausted
                    && reconciled.snapshot.pending_approvals.len() == 1
                    && reconciled.snapshot.active_turns.is_empty()
        )),
        "events={events:?}"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}

fn user_input(
    session_id: &str,
    agent_instance_id: &str,
    input_id: &str,
) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: input_id.to_string(),
        request_id: input_id.to_string(),
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        content: piko_protocol::MessageContent::String("hello".into()),
        submitted_at: 0,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}
