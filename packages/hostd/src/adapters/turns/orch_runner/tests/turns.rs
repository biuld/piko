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
    let run = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-direct".into(),
            agent_instance_id: child_id.into(),
            prompt: "follow up".into(),
            source_turn_id: Some("run-direct".into()),
            prompt_resources: None,
            cwd: cwd.clone(),
            active_tool_names: Some(Vec::new()),
            session_dir: session_dir.clone(),
            resume_agent: None,
        })
        .await
        .unwrap();
    AgentRunRunner::finish_agent_run(
        &runner,
        &AgentOperationAddress {
            session_id: "session-direct".into(),
            operation_id: "stale-run-id".into(),
            agent_instance_id: child_id.into(),
        },
        &piko_protocol::agent_runtime::SessionCursor {
            epoch: "stale".into(),
            seq: 0,
        },
    )
    .await;
    let duplicate = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-duplicate".into(),
            agent_instance_id: child_id.into(),
            prompt: "duplicate".into(),
            source_turn_id: Some("run-duplicate".into()),
            prompt_resources: None,
            cwd: cwd.clone(),
            active_tool_names: Some(Vec::new()),
            session_dir: session_dir.clone(),
            resume_agent: None,
        })
        .await
        .unwrap();
    assert_eq!(
        duplicate.receipt.disposition,
        piko_protocol::InputDisposition::Queued
    );
    let second = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-second-child".into(),
            agent_instance_id: "agent-child-two".into(),
            prompt: "parallel".into(),
            source_turn_id: Some("run-second-child".into()),
            prompt_resources: None,
            cwd,
            active_tool_names: Some(Vec::new()),
            session_dir,
            resume_agent: None,
        })
        .await
        .expect("different AgentInstances may run concurrently");
    let completed = run.process.wait_completion().await.unwrap();
    let duplicate_completed = duplicate.process.wait_completion().await.unwrap();
    let second_completed = second.process.wait_completion().await.unwrap();
    assert_eq!(completed.address.agent_instance_id, child_id);
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
