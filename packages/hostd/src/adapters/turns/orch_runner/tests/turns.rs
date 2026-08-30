use super::*;

#[tokio::test]
async fn agent_projection_is_emitted_only_after_durable_ack() {
    let hub = Arc::new(piko_orchd::events::SessionOutputHub::new(
        "session-1".into(),
        "epoch".into(),
        4,
    ));
    let event_router =
        Arc::new(super::super::observation_router::SessionObservationRouter::default());
    event_router.register("session-1", "operation", "child", true, Arc::clone(&hub));
    let cursor = hub.cursor();
    let subscription = hub.subscribe(&cursor).await.unwrap();
    let mut output = piko_orchd::events::merged_output_stream(subscription, cursor);
    let committing = ProjectingAgentCommitPort::new(
        Arc::new(EphemeralAgentCommitPort::default()),
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
    );
    committing
        .commit_agent_command("session", create_command())
        .await
        .unwrap();
    let envelope = output.next().await.unwrap().unwrap();
    assert!(matches!(
        envelope.output,
        piko_protocol::agent_runtime::SessionOutput::Event(event)
            if matches!(&event.event,
                piko_protocol::agent_runtime::SessionEvent::AgentChanged { agent }
                    if agent.agent_instance_id == "child")
    ));
    let cursor_after_success = hub.cursor();

    let failing = ProjectingAgentCommitPort::new(
        Arc::new(FailingAgentCommitPort),
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
    );
    assert!(
        failing
            .commit_agent_command("session", create_command())
            .await
            .is_err()
    );
    assert_eq!(hub.cursor(), cursor_after_success);
}

#[tokio::test]
async fn direct_input_runs_the_addressed_recovered_child_agent() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    let agents_dir = workspace.join(".piko/agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/agents/main.toml"),
        agents_dir.join("main.toml"),
    )
    .unwrap();
    let cwd = workspace.to_string_lossy().into_owned();
    let session_dir = temp.path().join("session");
    let store = SessionStore::create_session(&session_dir, "session-direct".into(), cwd.clone(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child_id = "agent-child";
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: child_id.into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    version: "1".into(),
                    provenance: piko_protocol::PromptSource::new("test", "worker"),
                    name: "Worker".into(),
                    role: "worker".into(),
                    kind: piko_protocol::AgentKind::Supervisor,
                    description: None,
                    base_instructions: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: "agent-child-two".into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    version: "1".into(),
                    provenance: piko_protocol::PromptSource::new("test", "worker"),
                    name: "Worker".into(),
                    role: "worker".into(),
                    kind: piko_protocol::AgentKind::Supervisor,
                    description: None,
                    base_instructions: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();

    let runner =
        super::super::OrchAgentRunRunner::new(Arc::new(DirectInputGateway), "test", "test-model")
            .await;
    runner
        .ensure_session_runtime("session-direct", &cwd, &session_dir, None)
        .await
        .unwrap();
    let run = submit_direct(
        &runner,
        "session-direct",
        child_id,
        "run-direct",
        "follow up",
        &session_dir,
    )
    .await;
    let _ = run;
    AgentRunRunner::finish_agent_run(&runner, "session-direct", child_id, "stale-run-id").await;
    let duplicate = submit_direct(
        &runner,
        "session-direct",
        child_id,
        "run-duplicate",
        "duplicate",
        &session_dir,
    )
    .await;
    assert_eq!(
        duplicate.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    let second = submit_direct(
        &runner,
        "session-direct",
        "agent-child-two",
        "run-second-child",
        "parallel",
        &session_dir,
    )
    .await;
    let _ = second;
    let completed = wait_completion(&runner, "session-direct", child_id, "run-direct").await;
    let duplicate_completed =
        wait_completion(&runner, "session-direct", child_id, "run-duplicate").await;
    let second_completed = wait_completion(
        &runner,
        "session-direct",
        "agent-child-two",
        "run-second-child",
    )
    .await;
    assert_eq!(completed.input_id, "run-direct");
    assert!(completed.result.is_ok());
    assert!(
        duplicate_completed.result.is_ok(),
        "queued run failed: {:?}",
        duplicate_completed.result
    );
    assert!(
        matches!(
            duplicate_completed.result.as_ref().unwrap().outcome,
            piko_protocol::ExecutionOutcome::Succeeded { .. }
        ),
        "queued run did not succeed: {:?}",
        duplicate_completed.result
    );
    assert!(second_completed.result.is_ok());

    let recovered = store.load_agent("session-direct", child_id).unwrap();
    assert_eq!(recovered.transcript.len(), 4);
    assert!(matches!(
        &recovered.transcript[0].message,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String(text),
            ..
        } if text == "follow up"
    ));
    assert!(matches!(
        &recovered.transcript[2].message,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String(text),
            ..
        } if text == "duplicate"
    ));
}

async fn submit_direct(
    runner: &super::super::OrchAgentRunRunner,
    session_id: &str,
    agent_instance_id: &str,
    input_id: &str,
    text: &str,
    session_dir: &std::path::Path,
) -> piko_protocol::AgentInputReceipt {
    let request = piko_protocol::SendAgentInputRequest {
        request_id: input_id.to_string(),
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        caller_agent_instance_id: None,
        root_input_id: Some(input_id.to_string()),
        message_id: format!("msg_{input_id}"),
        content: piko_protocol::MessageContent::String(text.to_string()),
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        prompt_resources: None,
        active_tool_names: Some(Vec::new()),
    };
    let canonical = piko_protocol::AgentInput::from_request(&request, crate::util::now_ms());
    let runtime = piko_orchd_api::AgentInputRuntime {
        prompt_resources: None,
        active_tool_names: Some(Vec::new()),
        root_input_id: Some(input_id.to_string()),
        message_id: Some(request.message_id),
    };
    let _ = session_dir;
    AgentRunRunner::submit_agent_input(runner, canonical, runtime)
        .await
        .unwrap()
}

async fn wait_completion(
    runner: &super::super::OrchAgentRunRunner,
    session_id: &str,
    agent_instance_id: &str,
    input_id: &str,
) -> crate::ports::AgentRunCompletion {
    AgentRunRunner::wait_agent_input_completion(runner, session_id, agent_instance_id, input_id)
        .await
        .unwrap()
}
