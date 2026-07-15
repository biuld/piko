#[tokio::test]
async fn root_and_child_are_committed_before_they_are_routable() {
    let (runtime, commits, _model) = attached_runtime().await;
    let child = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-1".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "coder".into(),
            requested_agent_instance_id: Some("coder-1".into()),
            origin_tool_call_id: Some("call-1".into()),
        })
        .await
        .expect("create child");

    assert_eq!(child.identity.agent_instance_id, "coder-1");
    assert_eq!(
        child.identity.parent_agent_instance_id.as_deref(),
        Some("root")
    );
    let snapshots = runtime
        .list_agents("session-1".into())
        .await
        .expect("list agents");
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].identity.agent_instance_id, "root");
    assert_eq!(snapshots[1].identity.agent_instance_id, "coder-1");

    let commands = commits.commands.lock().await;
    assert_eq!(commands.len(), 2);
    assert!(matches!(commands[0], AgentDurableCommand::Create { .. }));
    assert!(matches!(commands[1], AgentDurableCommand::Create { .. }));
}

#[tokio::test]
async fn failed_run_start_commit_rolls_back_execution_reservation() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("runs after retry").await;
    commits.fail_next_run_start();

    let request = SendAgentInputRequest {
        request_id: "atomic-start-fails".into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        caller_agent_instance_id: None,
        source_turn_id: None,
        message_id: "message-atomic-start-fails".into(),
        content: MessageContent::String("first attempt".into()),
        delivery: AgentInputDelivery::StartWhenIdle,
    prompt_resources: None,
    active_tool_names: None,
};
    assert!(matches!(
        runtime.run_agent(request).await,
        Err(orchd_api::AgentApiError::PersistenceFailed(_))
    ));
    assert_eq!(model.call_count().await, 0);

    let report = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "atomic-start-retry".into(),
            message_id: "message-atomic-start-retry".into(),
            content: MessageContent::String("retry".into()),
            ..SendAgentInputRequest {
                request_id: String::new(),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                source_turn_id: None,
                message_id: String::new(),
                content: MessageContent::String(String::new()),
                delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
}
        })
        .await
        .unwrap();
    assert_eq!(report.summary, "runs after retry");
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn cancellation_during_durable_start_converges_without_model_call() {
    let model = Arc::new(FauxProvider::new());
    let runtime = Arc::new(AgentRuntime::new(
        model.clone() as Arc<dyn llmd::gateway::LlmGateway>
    ));
    runtime.register_agent(test_agent()).await;
    let collected = Arc::new(CollectingAgentCommitPort::default());
    let blocking = Arc::new(BlockingRunStartCommitPort {
        inner: collected.clone(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-start-cancel".into(),
            root: AgentInstanceIdentity {
                session_id: "session-start-cancel".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: blocking.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                )),
            },
        })
        .await
        .unwrap();

    let running = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .run_agent(SendAgentInputRequest {
                    request_id: "start-cancel".into(),
                    session_id: "session-start-cancel".into(),
                    agent_instance_id: "root".into(),
                    caller_agent_instance_id: None,
                    source_turn_id: None,
                    message_id: "message-start-cancel".into(),
                    content: MessageContent::String("cancel before activation".into()),
                    delivery: AgentInputDelivery::StartWhenIdle,
                prompt_resources: None,
                active_tool_names: None,
})
                .await
        })
    };
    blocking
        .entered
        .acquire()
        .await
        .expect("run start was never entered")
        .forget();
    let cancelling = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .cancel_agent_run("session-start-cancel".into(), "root".into())
                .await
        })
    };
    tokio::task::yield_now().await;
    blocking.release.add_permits(1);

    let report = running.await.unwrap().unwrap();
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Cancelled { .. }
    ));
    assert!(cancelling.await.unwrap().unwrap().accepted);
    assert_eq!(model.call_count().await, 0);
    let commands = collected.commands.lock().await;
    let start = commands
        .iter()
        .position(|command| matches!(command, AgentDurableCommand::RunStarted { .. }))
        .unwrap();
    let terminal = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                AgentDurableCommand::RunTerminal { report, .. }
                    if matches!(report.outcome, piko_protocol::ExecutionOutcome::Cancelled { .. })
            )
        })
        .unwrap();
    assert!(start < terminal);
}

#[tokio::test]
async fn terminal_report_is_not_published_until_retry_commits() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model.push_text("durable terminal").await;
    commits.fail_next_run_terminal();

    let mut running = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .run_agent(SendAgentInputRequest {
                    request_id: "terminal-retry".into(),
                    session_id: "session-1".into(),
                    agent_instance_id: "root".into(),
                    caller_agent_instance_id: None,
                    source_turn_id: None,
                    message_id: "message-terminal-retry".into(),
                    content: MessageContent::String("run".into()),
                    delivery: AgentInputDelivery::StartWhenIdle,
                prompt_resources: None,
                active_tool_names: None,
})
                .await
        })
    };
    for _ in 0..100 {
        if commits.terminal_attempts.load(Ordering::SeqCst) >= 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut running)
            .await
            .is_err(),
        "waiter resolved before terminal retry committed"
    );
    assert!(
        runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .unwrap()
            .unwrap()
            .latest_report
            .is_none()
    );
    let report = tokio::time::timeout(std::time::Duration::from_secs(1), running)
        .await
        .expect("terminal persistence retry must be bounded")
        .unwrap()
        .unwrap();
    assert_eq!(report.summary, "durable terminal");
    assert_eq!(
        commits
            .commands
            .lock()
            .await
            .iter()
            .filter(|command| matches!(command, AgentDurableCommand::RunTerminal { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn cancellation_during_finalizing_preserves_the_selected_terminal() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model.push_text("selected terminal").await;
    commits.fail_next_run_terminal();
    let running = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .run_agent(SendAgentInputRequest {
                    request_id: "finalizing-cancel".into(),
                    session_id: "session-1".into(),
                    agent_instance_id: "root".into(),
                    caller_agent_instance_id: None,
                    source_turn_id: None,
                    message_id: "message-finalizing-cancel".into(),
                    content: MessageContent::String("run".into()),
                    delivery: AgentInputDelivery::StartWhenIdle,
                prompt_resources: None,
                active_tool_names: None,
})
                .await
        })
    };
    for _ in 0..100 {
        if commits.terminal_attempts.load(Ordering::SeqCst) >= 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    let finalizing = runtime
        .agent_snapshot("session-1".into(), "root".into())
        .await
        .unwrap()
        .unwrap();
    assert!(finalizing.latest_report.is_none());
    let cancelled = runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
    assert!(cancelled.accepted);
    let report = running.await.unwrap().unwrap();
    assert_eq!(report.summary, "selected terminal");
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Succeeded { .. }
    ));
}

#[tokio::test]
async fn permanent_terminal_conflict_publishes_no_report_and_marks_agent_unavailable() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("must remain uncommitted").await;
    commits.conflict_next_run_terminal();

    let result = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "terminal-conflict".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-terminal-conflict".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
})
        .await;
    assert!(matches!(
        result,
        Err(orchd_api::AgentApiError::PersistenceFailed(_))
    ));
    let snapshot = runtime
        .agent_snapshot("session-1".into(), "root".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Unavailable);
    assert!(snapshot.latest_report.is_none());
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn execution_panic_after_durable_start_converges_to_one_failed_terminal() {
    let runtime = AgentRuntime::new(Arc::new(PanicGateway));
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-panic".into(),
            root: AgentInstanceIdentity {
                session_id: "session-panic".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                )),
            },
        })
        .await
        .unwrap();
    let report = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "panic".into(),
            session_id: "session-panic".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-panic".into(),
            content: MessageContent::String("panic".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Failed { .. }
    ));
    assert_eq!(
        agents
            .commands
            .lock()
            .await
            .iter()
            .filter(|command| matches!(command, AgentDurableCommand::RunTerminal { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn failed_message_commit_never_advances_reusable_agent_transcript() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("must not become durable context").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-message-atomicity".into(),
            root: AgentInstanceIdentity {
                session_id: "session-message-atomicity".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(FailingMessageCommitPort {
                    attempt: AtomicU64::new(0),
                    fail_at: 2,
                })),
            },
        })
        .await
        .unwrap();

    let report = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "message-atomicity".into(),
            session_id: "session-message-atomicity".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-input-atomicity".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Failed { .. }
    ));
    assert!(report.summary.is_empty());
}

