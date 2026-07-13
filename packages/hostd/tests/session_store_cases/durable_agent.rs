#[tokio::test]
async fn recovery_marks_accepted_execution_interrupted() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                run_id: "exec-interrupted".into(),
                internal_execution_id: "exec-interrupted".into(),
                request_id: "request-interrupted".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: None,
                started_at: 1,
            },
        )
        .await
        .unwrap();

    assert_eq!(store.interrupt_incomplete_agent_executions().unwrap(), 1);
    assert_eq!(store.interrupt_incomplete_agent_executions().unwrap(), 0);
    let manifest = store.load_manifest().unwrap();
    let execution = manifest.agent_executions.get("exec-interrupted").unwrap();
    assert_eq!(execution.status, piko_protocol::ExecutionStatus::Cancelled);
    assert!(matches!(
        execution.report.as_ref().map(|report| &report.outcome),
        Some(piko_protocol::ExecutionOutcome::Cancelled { .. })
    ));
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
            AgentDurableCommand::RunStarted {
                agent_instance_id: child.agent_instance_id.clone(),
                run_id: "run-detached".into(),
                internal_execution_id: "exec-detached".into(),
                request_id: "request-detached".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: Some(root.agent_instance_id.clone()),
                started_at: 2,
            },
        )
        .await
        .unwrap();
    let report = AgentExecutionReport {
        agent_instance_id: child.agent_instance_id.clone(),
        report_id: "report-detached".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "detached result".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::RunTerminal {
                run_id: "run-detached".into(),
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
    let start = AgentDurableCommand::RunStarted {
        agent_instance_id: root.agent_instance_id.clone(),
        run_id: "run-idempotent".into(),
        internal_execution_id: "exec-idempotent".into(),
        request_id: "request-idempotent".into(),
        source_turn_id: None,
        detached_recipient_agent_instance_id: None,
        started_at: 1,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", start.clone())
            .await
            .unwrap();
    }
    let report = AgentExecutionReport {
        agent_instance_id: root.agent_instance_id.clone(),
        report_id: "report-idempotent".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    let terminal = AgentDurableCommand::RunTerminal {
        run_id: "run-idempotent".into(),
        report: report.clone(),
        finished_at: 2,
    };
    for _ in 0..2 {
        store
            .commit_agent_command("session-1", terminal.clone())
            .await
            .unwrap();
    }
    let manifest = store.load_manifest().unwrap();
    assert_eq!(manifest.agent_executions.len(), 1);
    assert_eq!(
        manifest
            .agent_executions
            .get("run-idempotent")
            .unwrap()
            .report
            .as_ref(),
        Some(&report)
    );
}

#[tokio::test]
async fn follow_up_queue_is_durable_and_advances_atomically_into_a_run() {
    let temp = tempdir().unwrap();
    let store = SessionStore::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
        .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let queued = piko_protocol::DurableAgentInput {
        queued_input_id: "queued-1".into(),
        request: piko_protocol::SendAgentInputRequest {
            request_id: "queued-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: root.agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-queued".into(),
            content: MessageContent::String("follow up".into()),
            delivery: piko_protocol::AgentInputDelivery::FollowUp,
        },
        detached_recipient_agent_instance_id: None,
    };
    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::InputQueued {
                agent_instance_id: root.agent_instance_id.clone(),
                queued_input: queued.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.agent_queued_inputs(&root.agent_instance_id).unwrap(),
        vec![queued]
    );

    store
        .commit_agent_command(
            "session-1",
            AgentDurableCommand::QueuedInputStarted {
                agent_instance_id: root.agent_instance_id.clone(),
                queued_input_id: "queued-1".into(),
                run_id: "run-queued".into(),
                internal_execution_id: "exec-queued".into(),
                request_id: "queued-1".into(),
                source_turn_id: None,
                detached_recipient_agent_instance_id: None,
                started_at: 2,
            },
        )
        .await
        .unwrap();
    let manifest = store.load_manifest().unwrap();
    assert!(manifest.agent_input_queue.is_empty());
    assert_eq!(
        manifest
            .agent_executions
            .get("run-queued")
            .unwrap()
            .execution_id,
        "exec-queued"
    );
}


