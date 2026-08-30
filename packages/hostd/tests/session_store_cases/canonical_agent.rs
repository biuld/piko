#[tokio::test]
async fn canonical_agent_inputs_replay_and_project_work_state() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let input = piko_protocol::AgentInput {
        input_id: "input-follow-up".into(),
        request_id: "request-follow-up".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        content: piko_protocol::MessageContent::String("continue the task".into()),
        submitted_at: 10,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let admission = piko_protocol::AgentInputAdmission {
        input: input.clone(),
        disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
        root_input_id: None,
        admitted_at: 11,
    };
    let first = store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted { admission },
        )
        .await
        .unwrap();
    let duplicate = store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: input.clone(),
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 11,
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(first.revision, duplicate.revision);

    let queued = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(queued.foreground, piko_protocol::AgentForeground::Queued);
    assert_eq!(queued.queued_inputs[0].input_id, "input-follow-up");
    assert_eq!(queued.queued_inputs[0].preview, "continue the task");

    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputDispositionChanged {
                change: piko_protocol::AgentInputDispositionChange {
                    agent_instance_id: root.agent_instance_id.clone(),
                    input_id: "input-follow-up".into(),
                    disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
                    root_input_id: Some("input-follow-up".into()),
                    model_step_id: None,
                    changed_at: 20,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                run_id: "run-follow-up".into(),
                internal_execution_id: "execution-follow-up".into(),
                request_id: "request-follow-up".into(),
                source_turn_id: Some("turn-follow-up".into()),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "digest".into(),
                started_at: 21,
                input,
            },
        )
        .await
        .unwrap();

    let active = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(active.foreground, piko_protocol::AgentForeground::Running);
    assert_eq!(
        active.active_work.as_ref().unwrap().root_input_id,
        "input-follow-up"
    );
    assert!(active.queued_inputs.is_empty());

    let current: serde_json::Value = serde_json::from_slice(
        &std::fs::read(temp.path().join("readmodels/current.json")).unwrap(),
    )
    .unwrap();
    let published = &current["aggregate"]["agent_work"][&root.agent_instance_id];
    assert_eq!(
        published["activeWork"]["rootInputId"],
        serde_json::json!("input-follow-up")
    );
    assert!(published.get("activeRun").is_none());

    let reopened = SessionStore::new(temp.path());
    let replayed = reopened
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(replayed, active);
}

#[tokio::test]
async fn run_start_commit_admits_and_binds_root_input_atomically() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let request = piko_protocol::SendAgentInputRequest {
        request_id: "request-root".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        caller_agent_instance_id: None,
        source_turn_id: Some("turn-root".into()),
        message_id: "message-root".into(),
        content: piko_protocol::MessageContent::String("start work".into()),
        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
    };
    let mut input = piko_protocol::AgentInput::from_request(&request, 10);
    input.input_id = "input-root".into();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                run_id: "run-root".into(),
                internal_execution_id: "execution-root".into(),
                request_id: request.request_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "digest-root".into(),
                started_at: 10,
                input: input.clone(),
            },
        )
        .await
        .unwrap();

    let snapshot = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    let work = snapshot.active_work.unwrap();
    assert_eq!(work.root_input_id, input.input_id);
    assert!(snapshot.queued_inputs.is_empty());
    assert!(snapshot.pending_steers.is_empty());
}

#[tokio::test]
async fn steer_message_and_application_are_committed_as_one_step_relation() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let root_request = piko_protocol::SendAgentInputRequest {
        request_id: "request-root-steer".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        caller_agent_instance_id: None,
        source_turn_id: Some("turn-root-steer".into()),
        message_id: "message-root-steer".into(),
        content: piko_protocol::MessageContent::String("start".into()),
        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                run_id: "run-steer".into(),
                internal_execution_id: "execution-steer".into(),
                request_id: root_request.request_id.clone(),
                source_turn_id: root_request.source_turn_id.clone(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "digest".into(),
                started_at: 10,
                input: piko_protocol::AgentInput::from_request(&root_request, 10),
            },
        )
        .await
        .unwrap();
    store
        .commit_message(
            piko_protocol::execution::MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: root_request.source_turn_id.clone(),
                execution_id: "execution-steer".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: root_request.message_id.clone(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: piko_protocol::Message::User {
                    content: root_request.content.clone(),
                    timestamp: Some(10),
                },
                committed_at: 10,
            },
            "main",
        )
        .unwrap();

    let steer_request = piko_protocol::SendAgentInputRequest {
        request_id: "request-steer".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        caller_agent_instance_id: None,
        source_turn_id: Some("turn-root-steer".into()),
        message_id: "message-steer".into(),
        content: piko_protocol::MessageContent::String("change direction".into()),
        delivery: piko_protocol::AgentInputDelivery::SteerActive,
        prompt_resources: None,
        active_tool_names: None,
    };
    let steer_admission = piko_protocol::AgentInputAdmission {
        input: piko_protocol::AgentInput::from_request(&steer_request, 20),
        disposition: piko_protocol::AgentInputDisposition::PendingSteer,
        root_input_id: Some(root_request.request_id.clone()),
        admitted_at: 20,
    };
    let first_steer = store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: steer_admission.clone(),
            },
        )
        .await
        .unwrap();
    let retry_steer = store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: steer_admission,
            },
        )
        .await
        .unwrap();
    assert_eq!(first_steer.revision, retry_steer.revision);
    let shifted_steer = store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput::from_request(&steer_request, 21),
                    disposition: piko_protocol::AgentInputDisposition::PendingSteer,
                    root_input_id: Some(root_request.request_id.clone()),
                    admitted_at: 21,
                },
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        shifted_steer,
        piko_protocol::CommitError::IdempotencyConflict
    );

    store
        .commit_steer(
            piko_protocol::execution::MessageCommit {
                session_id: "session-1".into(),
                source_turn_id: steer_request.source_turn_id.clone(),
                execution_id: "execution-steer".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: steer_request.message_id.clone(),
                parent_message_id: Some(root_request.message_id.clone()),
                tree_parent_entry_id: None,
                message: piko_protocol::Message::User {
                    content: steer_request.content.clone(),
                    timestamp: Some(20),
                },
                committed_at: 20,
            },
            "main",
            piko_protocol::AgentInputDispositionChange {
                agent_instance_id: root.agent_instance_id.clone(),
                input_id: steer_request.request_id.clone(),
                disposition: piko_protocol::AgentInputDisposition::AppliedToStep,
                root_input_id: Some(root_request.request_id.clone()),
                model_step_id: Some("execution-steer:step_1".into()),
                changed_at: 20,
            },
        )
        .unwrap();

    let after_steer = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert!(after_steer.pending_steers.is_empty());
    assert_eq!(
        after_steer
            .active_work
            .as_ref()
            .and_then(|work| work.active_model_step_id.as_deref()),
        Some("execution-steer:step_1")
    );

    store
        .commit_model_step(
            piko_protocol::execution::ModelStepCommit {
                session_id: "session-1".into(),
                source_turn_id: Some("turn-root-steer".into()),
                run_id: "run-steer".into(),
                execution_id: "execution-steer".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                model_step_id: "execution-steer:step_1".into(),
                step_index: 1,
                started_at: 21,
                finished_at: 22,
                outcome: piko_protocol::ModelStepOutcome::Completed,
                assistant: piko_protocol::execution::MessageCommit {
                    session_id: "session-1".into(),
                    source_turn_id: Some("turn-root-steer".into()),
                    execution_id: "execution-steer".into(),
                    agent_instance_id: root.agent_instance_id.clone(),
                    message_id: "assistant-steer".into(),
                    parent_message_id: Some("message-steer".into()),
                    tree_parent_entry_id: None,
                    message: piko_protocol::Message::Assistant {
                        content: vec![piko_protocol::ContentBlock::Text {
                            text: "done".into(),
                        }],
                        checkpoint: None,
                        provider: "test".into(),
                        model: "test".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(22),
                    },
                    committed_at: 22,
                },
                tool_calls: Vec::new(),
            },
            "main",
        )
        .unwrap();
    let projection = store.load_projection().unwrap();
    assert!(
        projection.agent_work[&root.agent_instance_id]
            .active_work
            .as_ref()
            .unwrap()
            .active_model_step_id
            .is_none()
    );
    assert_eq!(
        projection.agent_executions["run-steer"].model_steps[0].model_step_id,
        "execution-steer:step_1"
    );
}
