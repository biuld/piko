#[tokio::test]
async fn recovered_pending_detached_delivery_does_not_rerun_source_agent() {
    let model = Arc::new(FauxProvider::new());
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-delivery-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    let child = AgentInstanceIdentity {
        session_id: "session-delivery-recovery".into(),
        agent_instance_id: "child".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: Some("root".into()),
    };
    let report = piko_protocol::AgentExecutionReport {
        agent_instance_id: "child".into(),
        report_id: "report-recovered-detached".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "recovered detached report".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-delivery-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![
                AgentRecoveryState {
                    identity: root,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: Vec::new(),
                    latest_report: None,
                    execution_reports: Vec::new(),
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
                AgentRecoveryState {
                    identity: child,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: Vec::new(),
                    latest_report: Some(report.clone()),
                    execution_reports: vec![orchd_api::RecoveredExecutionReport {
                        internal_execution_id: "exec-recovered-detached".into(),
                        report: report.clone(),
                    }],
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: vec![orchd_api::RecoveredDetachedDelivery {
                        recipient_agent_instance_id: "root".into(),
                        report,
                    }],
                },
            ],
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    for _ in 0..100 {
        let inbox = runtime
            .agent_inbox("session-delivery-recovery".into(), "root".into())
            .await
            .unwrap();
        if inbox.items.len() == 1 {
            assert_eq!(inbox.items[0].report.summary, "recovered detached report");
            assert_eq!(model.call_count().await, 0);
            assert_eq!(
                agents
                    .commands
                    .lock()
                    .await
                    .iter()
                    .filter(|command| matches!(command, AgentDurableCommand::CommitReport { .. }))
                    .count(),
                1
            );
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("recovered detached report was not delivered");
}

#[tokio::test]
async fn recovered_child_restores_private_transcript_and_inbox() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("after recovery").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    let child = AgentInstanceIdentity {
        session_id: "session-recovery".into(),
        agent_instance_id: "child".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: Some("root".into()),
    };
    let old_report = piko_protocol::AgentExecutionReport {
        agent_instance_id: "child".into(),
        report_id: "report-old".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "old answer".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![
                AgentRecoveryState {
                    identity: root,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: vec![piko_protocol::AgentInboxItem {
                        report_id: "report-old".into(),
                        recipient_agent_instance_id: "root".into(),
                        source_agent_instance_id: "child".into(),
                        report: old_report.clone(),
                        committed_at: 1,
                        consumed_at: None,
                    }],
                    latest_report: None,
                    execution_reports: Vec::new(),
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
                AgentRecoveryState {
                    identity: child,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: vec![
                        piko_protocol::Message::User {
                            content: MessageContent::String("before recovery".into()),
                            timestamp: Some(1),
                        },
                        piko_protocol::Message::Assistant {
                            content: vec![piko_protocol::ContentBlock::Text {
                                text: "old answer".into(),
                            }],
                            api: "test".into(),
                            provider: "test".into(),
                            model: "test".into(),
                            usage: None,
                            stop_reason: Some("stop".into()),
                            error_message: None,
                            timestamp: Some(2),
                        },
                    ],
                    head_message_id: Some("old-head".into()),
                    inbox: Vec::new(),
                    latest_report: Some(old_report),
                    execution_reports: vec![orchd_api::RecoveredExecutionReport {
                        internal_execution_id: recovered_execution_id(
                            "session-recovery",
                            "child",
                            "replayed-old-execution",
                        ),
                        report: piko_protocol::AgentExecutionReport {
                            agent_instance_id: "child".into(),
                            report_id: "report-old".into(),
                            outcome: piko_protocol::ExecutionOutcome::Succeeded {
                                usage: Default::default(),
                            },
                            summary: "old answer".into(),
                            usage: Default::default(),
                            artifacts: Vec::new(),
                        },
                    }],
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
            ],
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    let inbox = runtime
        .agent_inbox("session-recovery".into(), "root".into())
        .await
        .unwrap();
    assert_eq!(inbox.items.len(), 1);
    let duplicate = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "replayed-old-execution".into(),
            session_id: "session-recovery".into(),
            agent_instance_id: "child".into(),
            caller_agent_instance_id: Some("root".into()),
            source_turn_id: None,
            message_id: "replayed-old-message".into(),
            content: MessageContent::String("must not rerun".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert_eq!(
        duplicate.disposition,
        piko_protocol::InputDisposition::Duplicate
    );
    assert_eq!(model.call_count().await, 0);
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "after-recovery".into(),
            session_id: "session-recovery".into(),
            agent_instance_id: "child".into(),
            caller_agent_instance_id: Some("root".into()),
            source_turn_id: None,
            message_id: "message-new".into(),
            content: MessageContent::String("continue".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert!(
        model.requests().await[0]
            .transcript
            .iter()
            .any(|message| matches!(
                message,
                piko_protocol::Message::Assistant { content, .. }
                    if content.iter().any(|block| matches!(
                        block,
                        piko_protocol::ContentBlock::Text { text } if text == "old answer"
                    ))
            ))
    );
}

fn recovered_execution_id(session_id: &str, agent_instance_id: &str, request_id: &str) -> String {
    orchd_api::stable_internal_id("exec", &[session_id, agent_instance_id, request_id])
}

#[tokio::test]
async fn recovered_durable_follow_up_starts_without_new_input() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("recovered follow-up").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-queued-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-queued-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![AgentRecoveryState {
                identity: root,
                spec: test_agent(),
                lifecycle: AgentInstanceLifecycle::Open,
                transcript: Vec::new(),
                head_message_id: None,
                inbox: Vec::new(),
                latest_report: None,
                execution_reports: Vec::new(),
                queued_inputs: vec![piko_protocol::DurableAgentInput {
                    queued_input_id: "queued-recovery".into(),
                    request: SendAgentInputRequest {
                        request_id: "queued-recovery".into(),
                        session_id: "session-queued-recovery".into(),
                        agent_instance_id: "root".into(),
                        caller_agent_instance_id: None,
                        source_turn_id: None,
                        message_id: "message-queued-recovery".into(),
                        content: MessageContent::String("continue".into()),
                        delivery: AgentInputDelivery::FollowUp,
                    },
                    detached_recipient_agent_instance_id: None,
                }],
                pending_detached_deliveries: Vec::new(),
            }],
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    for _ in 0..200 {
        let snapshot = runtime
            .agent_snapshot("session-queued-recovery".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        if snapshot
            .latest_report
            .as_ref()
            .is_some_and(|report| report.summary == "recovered follow-up")
        {
            assert!(agents.commands.lock().await.iter().any(|command| matches!(
                command,
                AgentDurableCommand::QueuedInputStarted { queued_input_id, .. }
                    if queued_input_id == "queued-recovery"
            )));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("recovered durable follow-up did not start");
}
