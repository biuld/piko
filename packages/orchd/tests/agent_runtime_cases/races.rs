use std::time::Duration;

use crate::blocking_tool::BlockingToolProvider;

fn race_input(
    input_id: &str,
    delivery: AgentInputDelivery,
    content: &str,
) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: input_id.into(),
        request_id: input_id.into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery,
        content: MessageContent::String(content.into()),
        submitted_at: 1,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}

async fn wait_model_calls(model: &FauxProvider, n: u32) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while model.call_count().await < n {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected {n} model calls"));
}

async fn blocking_steer_runtime() -> (
    Arc<AgentRuntime>,
    Arc<CollectingAgentCommitPort>,
    Arc<CollectingExecutionCommitPort>,
    Arc<FauxProvider>,
    BlockingToolProvider,
) {
    let model = Arc::new(FauxProvider::new());
    let mut agents = std::collections::HashMap::new();
    let mut agent = test_agent();
    agent.tool_set_ids = vec!["block".into()];
    agents.insert("main".into(), agent);
    let mut config = test_orchd_config();
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        config,
    )
    .await;
    let blocker = BlockingToolProvider::new();
    runtime
        .register_tool_provider(Box::new(blocker.clone()))
        .await;
    runtime
        .register_tool_set(BlockingToolProvider::tool_set())
        .await;
    let commits = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-1".into(),
            root: AgentInstanceIdentity {
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: commits.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();
    (runtime, commits, executions, model, blocker)
}

/// R1: steer while a blocking tool holds the current step applies at the next
/// model-request boundary. Do not use `waiting_for_cancel` here.
#[tokio::test]
async fn steer_then_root_still_running_applies_to_next_step() {
    let (runtime, commits, executions, model, blocker) = blocking_steer_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::tool_calls(vec![
            piko_protocol::ToolCall {
                id: "call-1".into(),
                name: "block_until_released".into(),
                arguments: serde_json::json!({}),
                partial_json: None,
            },
        ]))
        .await;
    model.push_text("steered").await;
    model.push_text("done").await;

    let root = runtime
        .submit_agent_input(race_input(
            "root-r1",
            AgentInputDelivery::StartWhenIdle,
            "investigate",
        ))
        .await
        .unwrap();
    assert_eq!(root.disposition, piko_protocol::AgentInputDisposition::AppliedAsRoot);

    tokio::time::timeout(Duration::from_secs(15), async {
        while !blocker.started.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("blocking tool must start");

    let steer_runtime = Arc::clone(&runtime);
    let steer_task = tokio::spawn(async move {
        steer_runtime
            .submit_agent_input(race_input(
                "steer-r1",
                AgentInputDelivery::SteerActive,
                "status",
            ))
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    blocker.release.store(true, Ordering::SeqCst);
    let steer = steer_task.await.unwrap().unwrap();
    assert_eq!(
        steer.disposition,
        piko_protocol::AgentInputDisposition::PendingSteer
    );

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = runtime
                .agent_snapshot("session-1".into(), "root".into())
                .await
                .unwrap()
                .unwrap();
            if matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
                && model.call_count().await >= 3
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("root must finish after applying the steer");

    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.input_id == "steer-r1"
                && admission.disposition == piko_protocol::AgentInputDisposition::PendingSteer
                && admission.root_input_id.as_deref() == Some("root-r1")
    )));
    let applied = executions.steer_changes();
    assert!(
        applied.iter().any(|change| {
            change.input_id == "steer-r1"
                && change.disposition == piko_protocol::AgentInputDisposition::AppliedToStep
                && change.root_input_id.as_deref() == Some("root-r1")
                && change.model_step_id.is_some()
        }),
        "steer must apply to R's next step, never a successor: {applied:?}"
    );
}

/// R2: admit steer during `waiting_for_cancel`, then interrupt before the next
/// model boundary. Unused pending steers must not bind to successor S.
#[tokio::test]
async fn steer_then_root_recovers_cancels_pending_steer() {
    let (runtime, commits, executions, model) = attached_runtime_ports().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model.push_text("successor").await;

    runtime
        .submit_agent_input(race_input(
            "root-r2",
            AgentInputDelivery::StartWhenIdle,
            "first",
        ))
        .await
        .unwrap();
    wait_model_calls(&model, 1).await;

    let follow = runtime
        .submit_agent_input(race_input(
            "follow-r2",
            AgentInputDelivery::FollowUp,
            "second",
        ))
        .await
        .unwrap();
    assert_eq!(
        follow.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );

    let steer = runtime
        .submit_agent_input(race_input(
            "steer-r2",
            AgentInputDelivery::SteerActive,
            "late",
        ))
        .await
        .unwrap();
    assert_eq!(
        steer.disposition,
        piko_protocol::AgentInputDisposition::PendingSteer
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.input_id == "steer-r2"
                && admission.root_input_id.as_deref() == Some("root-r2")
    )));

    runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
    wait_model_calls(&model, 2).await;
    wait_until_idle(&runtime).await;

    assert!(
        executions
            .steer_changes()
            .iter()
            .all(|change| change.root_input_id.as_deref() != Some("follow-r2")),
        "unused pending steer must never bind to successor S"
    );
}

/// R3: steer after the root is terminal writes no AgentInput.
#[tokio::test]
async fn steer_after_root_terminal_writes_no_input() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("done").await;
    runtime
        .submit_agent_input(race_input(
            "root-r3",
            AgentInputDelivery::StartWhenIdle,
            "first",
        ))
        .await
        .unwrap();
    runtime
        .wait_agent_input_completion("session-1".into(), "root".into(), "root-r3".into())
        .await
        .unwrap();

    let error = runtime
        .submit_agent_input(race_input(
            "steer-r3",
            AgentInputDelivery::SteerActive,
            "too late",
        ))
        .await
        .expect_err("steer after terminal is InvalidState");
    assert_eq!(error, piko_orchd_api::AgentApiError::InvalidState);
    assert!(!commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.input_id == "steer-r3"
    )));
}

/// R4: follow-up while busy is pending, then the same input_id becomes a new root.
#[tokio::test]
async fn follow_up_while_busy_is_pending() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model.push_text("follow-up root").await;

    runtime
        .submit_agent_input(race_input(
            "root-r4",
            AgentInputDelivery::StartWhenIdle,
            "first",
        ))
        .await
        .unwrap();
    wait_model_calls(&model, 1).await;

    let follow = runtime
        .submit_agent_input(race_input(
            "follow-r4",
            AgentInputDelivery::FollowUp,
            "next",
        ))
        .await
        .unwrap();
    assert_eq!(
        follow.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.input_id == "follow-r4"
                && admission.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp
    )));

    runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
    let report = runtime
        .wait_agent_input_completion("session-1".into(), "root".into(), "follow-r4".into())
        .await
        .unwrap();
    assert_eq!(report.root_input_id, "follow-r4");
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputProcessingStarted { input, .. }
            if input.input_id == "follow-r4"
    )));
}

/// R5: follow-up on an idle agent starts immediately as one root.
#[tokio::test]
async fn follow_up_while_idle_starts() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("started").await;
    let receipt = runtime
        .submit_agent_input(race_input(
            "follow-r5",
            AgentInputDelivery::FollowUp,
            "go",
        ))
        .await
        .unwrap();
    assert_eq!(
        receipt.disposition,
        piko_protocol::AgentInputDisposition::AppliedAsRoot
    );
    let report = runtime
        .wait_agent_input_completion("session-1".into(), "root".into(), "follow-r5".into())
        .await
        .unwrap();
    assert_eq!(report.root_input_id, "follow-r5");
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputProcessingStarted { input, .. }
            if input.input_id == "follow-r5"
    )));
}

/// R6: cancel a pending follow-up before it advances; it never becomes a root.
#[tokio::test]
async fn cancel_before_advance() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .submit_agent_input(race_input(
            "root-r6",
            AgentInputDelivery::StartWhenIdle,
            "active",
        ))
        .await
        .unwrap();
    wait_model_calls(&model, 1).await;
    runtime
        .submit_agent_input(race_input(
            "follow-r6",
            AgentInputDelivery::FollowUp,
            "queued",
        ))
        .await
        .unwrap();

    let cancelled = runtime
        .cancel_agent_input("session-1".into(), "root".into(), "follow-r6".into())
        .await
        .unwrap();
    assert!(cancelled.accepted);
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputDispositionChanged { change }
            if change.input_id == "follow-r6"
                && change.disposition == piko_protocol::AgentInputDisposition::Cancelled
    )));
    assert!(!commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputProcessingStarted { input, .. }
            if input.input_id == "follow-r6"
    )));
    runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
}

/// R7: cancel after the follow-up has advanced cannot interrupt the new root.
#[tokio::test]
async fn cancel_after_advance() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .submit_agent_input(race_input(
            "root-r7",
            AgentInputDelivery::StartWhenIdle,
            "first",
        ))
        .await
        .unwrap();
    wait_model_calls(&model, 1).await;
    runtime
        .submit_agent_input(race_input(
            "follow-r7",
            AgentInputDelivery::FollowUp,
            "second",
        ))
        .await
        .unwrap();
    runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
    wait_model_calls(&model, 2).await;

    let cancelled = runtime
        .cancel_agent_input("session-1".into(), "root".into(), "follow-r7".into())
        .await
        .unwrap();
    assert!(!cancelled.accepted);
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputProcessingStarted { input, .. }
            if input.input_id == "follow-r7"
    )));
    runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
}

/// R8: idle interrupt is unaccepted and cannot name a later root.
#[tokio::test]
async fn interrupt_idle_is_unaccepted() {
    let (runtime, commits, _model) = attached_runtime().await;
    let receipt = runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
    assert!(!receipt.accepted);
    assert!(!commits.commands.lock().await.iter().any(|command| {
        matches!(command, AgentDurableCommand::InterruptRequested { .. })
    }));
}
