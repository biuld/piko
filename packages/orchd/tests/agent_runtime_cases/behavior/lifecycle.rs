use super::*;

#[tokio::test]
async fn follow_up_queue_rejects_past_its_fixed_cap_with_overload() {
    let (runtime, _commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "cap-active-run".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "cap-active-message".into(),
            content: MessageContent::String("active".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    for index in 0..64 {
        let receipt = runtime
            .send_agent_input(SendAgentInputRequest {
                request_id: format!("cap-queued-{index}"),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                source_turn_id: None,
                message_id: format!("cap-queued-message-{index}"),
                content: MessageContent::String("queued".into()),
                delivery: AgentInputDelivery::FollowUp,
                prompt_resources: None,
                active_tool_names: None,
            })
            .await
            .expect("follow-ups up to the cap must queue");
        assert_eq!(
            receipt.disposition,
            piko_protocol::AgentInputDisposition::PendingFollowUp
        );
    }
    let overloaded = match runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "cap-overflow".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "cap-overflow-message".into(),
            content: MessageContent::String("overflow".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
    {
        Ok(_) => panic!("a follow-up past the cap must overload"),
        Err(error) => error,
    };
    assert_eq!(overloaded, piko_orchd_api::AgentApiError::Overload);

    runtime
        .cancel_agent_input(
            "session-1".into(),
            "root".into(),
            "cap-queued-0".into(),
        )
        .await
        .unwrap();
    let freed = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "cap-freed-slot".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "cap-freed-message".into(),
            content: MessageContent::String("freed".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .expect("a cancelled queued input frees its slot");
    assert_eq!(
        freed.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
}

#[tokio::test]
async fn cancelled_run_commits_a_durable_abort_marker() {
    let model = Arc::new(FauxProvider::new());
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-cancel-marker".into(),
            root: AgentInstanceIdentity {
                session_id: "session-cancel-marker".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "cancel-marker-run".into(),
            session_id: "session-cancel-marker".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "cancel-marker-message".into(),
            content: MessageContent::String("run then cancel".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    runtime
        .cancel_agent_run("session-cancel-marker".into(), "root".into())
        .await
        .unwrap();
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.wait_agent_input_completion(
            "session-cancel-marker".into(),
            "root".into(),
            "cancel-marker-run".into(),
        ),
    )
    .await
    .expect("cancelled run terminates")
    .unwrap();
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Cancelled { .. }
    ));

    let execution_id = piko_orchd_api::stable_internal_id(
        "exec",
        &["session-cancel-marker", "root", "cancel-marker-run"],
    );
    let marker_id = piko_protocol::turn_abort_marker_message_id(&execution_id);
    let messages = executions.messages();
    let markers: Vec<_> = messages
        .iter()
        .filter(|commit| commit.message_id == marker_id)
        .collect();
    assert_eq!(markers.len(), 1, "abort marker must be committed exactly once");
    assert!(matches!(
        &markers[0].message,
        piko_protocol::Message::Context {
            trust: piko_protocol::ContentTrust::Trusted,
            ..
        }
    ));
    let execution_messages: Vec<_> = messages
        .iter()
        .filter(|commit| commit.execution_id == execution_id)
        .collect();
    assert_eq!(
        execution_messages.last().map(|commit| &commit.message_id),
        Some(&marker_id),
        "the abort marker must be the last committed message of the run"
    );
}

#[tokio::test]
async fn startup_cancel_commits_a_durable_abort_marker() {
    let model = Arc::new(FauxProvider::new());
    let runtime = Arc::new(AgentRuntime::new(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>
    ));
    runtime.register_agent(test_agent()).await;
    let collected = Arc::new(CollectingAgentCommitPort::default());
    let blocking = Arc::new(BlockingRunStartCommitPort {
        inner: collected.clone(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-start-cancel-marker".into(),
            root: AgentInstanceIdentity {
                session_id: "session-start-cancel-marker".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: blocking.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    let running = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            runtime
                .wait_sent_agent(SendAgentInputRequest {
                    request_id: "start-cancel-marker".into(),
                    session_id: "session-start-cancel-marker".into(),
                    agent_instance_id: "root".into(),
                    caller_agent_instance_id: None,
                    source_turn_id: None,
                    message_id: "message-start-cancel-marker".into(),
                    content: MessageContent::String("cancel during startup".into()),
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
                .cancel_agent_run("session-start-cancel-marker".into(), "root".into())
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

    let execution_id = piko_orchd_api::stable_internal_id(
        "exec",
        &[
            "session-start-cancel-marker",
            "root",
            "start-cancel-marker",
        ],
    );
    let marker_id = piko_protocol::turn_abort_marker_message_id(&execution_id);
    let messages = executions.messages();
    let markers: Vec<_> = messages
        .iter()
        .filter(|commit| commit.message_id == marker_id)
        .collect();
    assert_eq!(markers.len(), 1, "startup-cancel marker must be committed once");
}

#[tokio::test]
async fn agent_reuses_private_transcript_across_executions() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("first answer").await;
    model.push_text("second answer").await;

    for (request_id, message_id, content) in [
        ("input-1", "message-1", "first question"),
        ("input-2", "message-2", "second question"),
    ] {
        runtime
            .send_agent_input(SendAgentInputRequest {
                request_id: request_id.into(),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                source_turn_id: None,
                message_id: message_id.into(),
                content: MessageContent::String(content.into()),
                delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
})
            .await
            .expect("start agent execution");
        wait_until_idle(&runtime).await;
    }

    let requests = model.requests().await;
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1].conversation.items.iter().any(|item| matches!(
            &item.kind,
            piko_llmd::gateway::ConversationItemKind::Assistant { content }
                if content.iter().any(|block| matches!(
                    block,
                    piko_protocol::ContentBlock::Text { text } if text == "first answer"
                ))
        )),
        "second Execution must receive the first Execution's private transcript"
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::RunTerminal { report, .. }
            if report.summary == "second answer"
    )));
}
