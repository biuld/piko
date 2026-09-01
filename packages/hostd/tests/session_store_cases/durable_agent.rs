#[tokio::test]
async fn recovery_marks_accepted_execution_interrupted() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "request-interrupted".into(),
                request_id: "request-interrupted".into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-interrupted".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    "request-interrupted",
                    None,
                    "interrupted input",
                    1,
                ),
                input_message_id: "input-interrupted".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 1,
            },
        )
        .await
        .unwrap();
    store
        .commit_message(
            piko_protocol::agent_work::MessageCommit {
                session_id: "session-1".into(),
                root_input_id: "request-interrupted".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "input-interrupted".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String("interrupted input".into()),
                    timestamp: Some(1),
                },
                committed_at: 1,
            },
            "main",
        )
        .unwrap();

    let active_work = store
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        active_work
            .active_work
            .as_ref()
            .map(|work| work.root_input_id.as_str()),
        Some("request-interrupted")
    );

    assert_eq!(store.interrupt_incomplete_agent_work().unwrap(), 1);
    assert_eq!(store.interrupt_incomplete_agent_work().unwrap(), 0);
    let projection = store.load_projection().unwrap();
    let execution = projection.root_inputs.get("request-interrupted").unwrap();
    assert_eq!(execution.status, piko_protocol::AgentWorkProcessingStatus::Cancelled);
    assert!(matches!(
        execution.report.as_ref().map(|report| &report.outcome),
        Some(piko_protocol::AgentWorkOutcome::Cancelled { .. })
    ));

    // Recovery also appends the durable, model-visible abort marker after the
    // last committed message, with a stable id so re-recovery is idempotent.
    let recovered = store
        .load_agent("session-1", &root.agent_instance_id)
        .unwrap();
    let marker_id = piko_protocol::agent_work_abort_marker_message_id("request-interrupted");
    assert_eq!(recovered.transcript.len(), 2);
    assert_eq!(recovered.transcript[0].id, "input-interrupted");
    assert_eq!(recovered.transcript[1].id, marker_id);
    assert_eq!(
        recovered.transcript[1].parent_id.as_deref(),
        Some("input-interrupted")
    );
    assert_eq!(recovered.head_message_id.as_deref(), Some(marker_id.as_str()));
    assert!(matches!(
        &recovered.transcript[1].message,
        piko_protocol::Message::Context {
            trust: piko_protocol::ContentTrust::Trusted,
            ..
        }
    ));
}

#[tokio::test]
async fn pending_action_and_interrupt_replay_into_authoritative_work_snapshot() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let root_input_id = "input-control";
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                request_id: root_input_id.into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-control".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    root_input_id,
                    None,
                    "control input",
                    1,
                ),
                input_message_id: "message-control".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 1,
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::PendingActionRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                action: piko_protocol::PendingActionSummary {
                    action_id: "approval-1".into(),
                    kind: "approval".into(),
                    summary: Some("shell".into()),
                },
                requested_at: 2,
            },
        )
        .await
        .unwrap();

    let reopened = SessionStore::new(temp.path());
    let snapshot = reopened
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.foreground, piko_protocol::AgentForeground::RequiresAction);
    assert_eq!(snapshot.pending_action.unwrap().action_id, "approval-1");

    reopened
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::InterruptRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                requested_at: 3,
            },
        )
        .await
        .unwrap();
    let snapshot = reopened
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.foreground, piko_protocol::AgentForeground::RequiresAction);
    assert_eq!(
        snapshot.active_work.unwrap().state,
        piko_protocol::AgentWorkViewState::Cancelling
    );

    reopened
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::PendingActionResolved {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: root_input_id.into(),
                action_id: "approval-1".into(),
                resolved_at: 4,
            },
        )
        .await
        .unwrap();
    let snapshot = reopened
        .agent_work_snapshot(&root.agent_instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.foreground, piko_protocol::AgentForeground::Cancelling);
    assert!(snapshot.pending_action.is_none());
}


#[tokio::test]
async fn recovery_completes_declared_tool_calls_without_rerunning_the_model_step() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "request-with-tool-call".into(),
                request_id: "request-with-tool-call".into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-with-tool-call".into(),
                started_at: 1,
                input: root_input(
                    &root.agent_instance_id,
                    "request-with-tool-call",
                    Some("turn-with-tool-call"),
                    "tool input",
                    1,
                ),
                input_message_id: "input-with-tool-call".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 1,
            },
        )
        .await
        .unwrap();

    let model_step = piko_protocol::agent_work::ModelStepCommit {
                session_id: "session-1".into(),
            root_input_id: "request-with-tool-call".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                model_step_id: "step-with-tool-call".into(),
                step_index: 1,
                started_at: 2,
                finished_at: 3,
                outcome: piko_protocol::ModelStepOutcome::ToolCalls,
                assistant: piko_protocol::agent_work::MessageCommit {
                    session_id: "session-1".into(),
                root_input_id: "request-with-tool-call".into(),
                    agent_instance_id: root.agent_instance_id.clone(),
                    message_id: "assistant-with-tool-call".into(),
                    parent_message_id: Some("input-with-tool-call".into()),
                    tree_parent_entry_id: None,
                    message: piko_protocol::Message::Assistant {
                        content: vec![piko_protocol::ContentBlock::Text {
                            text: "I will inspect the project".into(),
                        }],
                        checkpoint: None,
                        provider: "test".into(),
                        model: "model".into(),
                        usage: None,
                        stop_reason: Some("tool_calls".into()),
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                tool_calls: vec![piko_protocol::agent_work::MessageCommit {
                    session_id: "session-1".into(),
                root_input_id: "request-with-tool-call".into(),
                    agent_instance_id: root.agent_instance_id.clone(),
                    message_id: "tool-call-message".into(),
                    parent_message_id: Some("assistant-with-tool-call".into()),
                    tree_parent_entry_id: None,
                    message: piko_protocol::Message::ToolCall {
                        id: "call-with-tool-call".into(),
                        name: "read".into(),
                        arguments: serde_json::json!({"path": "README.md"}),
                        model: None,
                        provider: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                }],
            };
    store
        .commit_model_step(model_step.clone(), "main")
        .unwrap();
    store
        .commit_model_step(model_step.clone(), "main")
        .expect("an exact model-step retry is idempotent");
    let mut conflicting = model_step;
    let piko_protocol::Message::Assistant { content, .. } =
        &mut conflicting.assistant.message
    else {
        unreachable!("fixture assistant message")
    };
    content.push(piko_protocol::ContentBlock::Text {
        text: "conflicting retry".into(),
    });
    assert_eq!(
        store.commit_model_step(conflicting, "main"),
        Err(piko_protocol::CommitError::IdempotencyConflict)
    );

    assert_eq!(store.interrupt_incomplete_agent_work().unwrap(), 1);
    let recovered = store
        .load_agent("session-1", &root.agent_instance_id)
        .unwrap();
    assert_eq!(recovered.transcript.len(), 5);
    assert!(matches!(
        &recovered.transcript[3].message,
        piko_protocol::Message::ToolResult {
            tool_call_id,
            tool_name: Some(tool_name),
            is_error: Some(true),
            ..
        } if tool_call_id == "call-with-tool-call" && tool_name == "read"
    ));
    assert_eq!(
        recovered.transcript[3].parent_id.as_deref(),
        Some("tool-call-message")
    );
    let marker_id = piko_protocol::agent_work_abort_marker_message_id("request-with-tool-call");
    assert_eq!(recovered.transcript[4].id, marker_id);
    assert_eq!(
        recovered.transcript[4].parent_id.as_deref(),
        Some(recovered.transcript[3].id.as_str())
    );

    let projection = store.load_projection().unwrap();
    let execution = projection
        .root_inputs
        .get("request-with-tool-call")
        .unwrap();
    assert_eq!(execution.model_steps.len(), 1);
    assert_eq!(execution.model_steps[0].model_step_id, "step-with-tool-call");
}

#[tokio::test]
async fn detached_delivery_recovery_is_pending_until_idempotent_inbox_commit() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child = AgentInstanceIdentity {
        session_id: "session-1".into(),
        agent_instance_id: "child".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: Some(root.agent_instance_id.clone()),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::Create {
                identity: child.clone(),
                spec: test_agent_spec("main"),
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: child.agent_instance_id.clone(),
                root_input_id: "request-detached".into(),
                request_id: "request-detached".into(),
                detached_recipient_agent_instance_id: Some(root.agent_instance_id.clone()),
                prompt_assembly_version: 1,
                prompt_digest: "prompt-detached".into(),
                started_at: 2,
                input: piko_protocol::AgentInput {
                    detached_recipient_agent_instance_id: Some(root.agent_instance_id.clone()),
                    ..root_input(
                        &child.agent_instance_id,
                        "request-detached",
                        None,
                        "detached input",
                        2,
                    )
                },
                input_message_id: "input-detached".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 2,
            },
        )
        .await
        .unwrap();
    let report = AgentWorkReport {
        agent_instance_id: child.agent_instance_id.clone(),
        root_input_id: "request-detached".into(),
        report_id: "report-detached".into(),
        outcome: piko_protocol::AgentWorkOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "detached result".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingFinished {
                agent_instance_id: report.clone().agent_instance_id.clone(),
                root_input_id: report.clone().root_input_id.clone(),
                report: report.clone(),
                finished_at: 3,
            },
        )
        .await
        .unwrap();

    let pending = store
        .pending_detached_deliveries(&child.agent_instance_id)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].report, report);

    for _ in 0..2 {
        store
            .commit_agent_command(
                "session-1",
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id: root.agent_instance_id.clone(),
                    report: report.clone(),
                },
            )
            .await
            .unwrap();
    }

    assert!(
        store
            .pending_detached_deliveries(&child.agent_instance_id)
            .unwrap()
            .is_empty()
    );
    let inbox = store.agent_inbox(&root.agent_instance_id).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].report, report);
}

#[tokio::test]
async fn duplicate_run_start_and_terminal_are_idempotent() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let start = AgentDurableCommand::AgentInputProcessingStarted {
        agent_instance_id: root.agent_instance_id.clone(),
        root_input_id: "request-idempotent".into(),
        request_id: "request-idempotent".into(),
        detached_recipient_agent_instance_id: None,
        prompt_assembly_version: 1,
        prompt_digest: "prompt-idempotent".into(),
        started_at: 1,
        input: root_input(
            &root.agent_instance_id,
            "request-idempotent",
            None,
            "idempotent input",
            1,
        ),
        input_message_id: "input-idempotent".into(),
        input_parent_message_id: None,
        input_tree_parent_entry_id: None,
        input_committed_at: 1,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", start.clone())
            .await
            .unwrap();
    }
    let report = AgentWorkReport {
        agent_instance_id: root.agent_instance_id.clone(),
        root_input_id: "request-idempotent".into(),
        report_id: "report-idempotent".into(),
        outcome: piko_protocol::AgentWorkOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    let terminal = AgentDurableCommand::AgentInputProcessingFinished {
        agent_instance_id: report.clone().agent_instance_id.clone(),
        root_input_id: report.clone().root_input_id.clone(),
        report: report.clone(),
        finished_at: 2,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", terminal.clone())
            .await
            .unwrap();
    }
    let projection = store.load_projection().unwrap();
    assert_eq!(projection.root_inputs.len(), 1);
    let execution = projection.root_inputs.get("request-idempotent").unwrap();
    assert_eq!(execution.report.as_ref(), Some(&report));
    assert_eq!(execution.prompt_assembly_version, 1);
    assert_eq!(execution.prompt_digest, "prompt-idempotent");
}

#[tokio::test]
async fn follow_up_queue_is_durable_and_advances_atomically_into_a_run() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let queued = piko_protocol::AgentInput {
        input_id: "queued-1".into(),
        request_id: "queued-1".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        origin: piko_protocol::AgentInputOrigin::System,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        content: MessageContent::String("follow up".into()),
        submitted_at: 2,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: queued.clone(),
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 2,
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.agent_queued_inputs(&root.agent_instance_id).unwrap(),
        vec![queued.clone()]
    );

    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "queued-1".into(),
                request_id: "queued-1".into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-queued".into(),
                started_at: 2,
                input: queued,
                input_message_id: "input-queued-1".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 2,
            },
        )
        .await
        .unwrap();
    let projection = store.load_projection().unwrap();
    assert!(projection.agent_input_queue.is_empty());
    assert_eq!(
        projection
            .root_inputs
            .get("queued-1")
            .unwrap()
            .root_input_id,
        "queued-1"
    );

    let cancelled = piko_protocol::AgentInput {
        input_id: "queued-cancelled".into(),
        request_id: "queued-cancelled".into(),
        session_id: "session-1".into(),
        agent_instance_id: root.agent_instance_id.clone(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        content: MessageContent::String("cancel me".into()),
        submitted_at: 3,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: cancelled,
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 3,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::AgentInputDispositionChanged {
                change: piko_protocol::AgentInputDispositionChange {
                    agent_instance_id: root.agent_instance_id.clone(),
                    input_id: "queued-cancelled".into(),
                    disposition: piko_protocol::AgentInputDisposition::Cancelled,
                    root_input_id: None,
                    model_step_id: None,
                    changed_at: 3,
                },
            },
        )
        .await
        .unwrap();
    assert!(store.agent_queued_inputs(&root.agent_instance_id).unwrap().is_empty());
}
fn root_input(
    agent_instance_id: &str,
    request_id: &str,
    root_input_id: Option<&str>,
    content: &str,
    submitted_at: i64,
) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: request_id.into(),
        request_id: request_id.into(),
        session_id: "session-1".into(),
        agent_instance_id: agent_instance_id.into(),
        origin: root_input_id.map_or(
            piko_protocol::AgentInputOrigin::System,
            |_| piko_protocol::AgentInputOrigin::User,
        ),
        delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
        content: piko_protocol::MessageContent::String(content.into()),
        submitted_at,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}
