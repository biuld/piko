#[tokio::test]
async fn lifecycle_and_activity_are_independent() {
    let (runtime, _commits, model) = attached_runtime().await;
    let closed = runtime
        .close_agent(AgentLifecycleRequest {
            request_id: "close-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("close root");
    assert_eq!(closed.lifecycle, AgentInstanceLifecycle::Closed);

    let snapshot = runtime
        .agent_snapshot("session-1".into(), "root".into())
        .await
        .expect("snapshot")
        .expect("root snapshot");
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Closed);
    assert!(matches!(
        snapshot.activity,
        piko_protocol::AgentActivity::Idle
    ));
    let rejected = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "closed-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "closed-message".into(),
            content: MessageContent::String("must reject".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .expect_err("closed AgentInstance must reject input");
    assert_eq!(rejected, orchd_api::AgentApiError::AgentClosed);

    let reopened = runtime
        .reopen_agent(AgentLifecycleRequest {
            request_id: "reopen-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("reopen root");
    assert_eq!(reopened.lifecycle, AgentInstanceLifecycle::Open);
    model.push_text("reused after reopen").await;
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "reopened-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "reopened-message".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
}

#[tokio::test]
async fn each_run_gets_one_fresh_prompt_from_its_resource_snapshot() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("first").await;
    model.push_text("second").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let prompts = Arc::new(RecordingPromptAssemblyPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-prompt-refresh".into(),
            root: AgentInstanceIdentity {
                session_id: "session-prompt-refresh".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                )
                .with_prompt(prompts.clone() as Arc<dyn PromptAssemblyPort>),
            },
        })
        .await
        .unwrap();

    for (suffix, context) in [("first", "day one"), ("second", "day two")] {
        runtime
            .run_agent(SendAgentInputRequest {
                request_id: format!("request-{suffix}"),
                session_id: "session-prompt-refresh".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                source_turn_id: None,
                message_id: format!("message-{suffix}"),
                content: MessageContent::String(suffix.into()),
                delivery: AgentInputDelivery::Auto,
                prompt_resources: Some(piko_protocol::PromptResourceSnapshot {
                    context_section: context.into(),
                    ..Default::default()
                }),
                active_tool_names: None,
            })
            .await
            .unwrap();
    }

    let requests = model.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].system_prompt, "test|day one");
    assert_eq!(requests[1].system_prompt, "test|day two");
    assert_eq!(prompts.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn cross_session_or_missing_parent_is_rejected_before_commit() {
    let (runtime, commits, _model) = attached_runtime().await;
    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-bad".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "not-in-session".into(),
            agent_spec_id: "coder".into(),
            requested_agent_instance_id: None,
            origin_tool_call_id: None,
        })
        .await
        .expect_err("missing parent must fail");
    assert_eq!(error, orchd_api::AgentApiError::AgentNotFound);
    assert_eq!(commits.commands.lock().await.len(), 1);
}

#[tokio::test]
async fn create_and_input_requests_are_idempotent() {
    let (runtime, commits, model) = attached_runtime().await;
    let create = CreateAgentRequest {
        request_id: "create-idempotent".into(),
        session_id: "session-1".into(),
        parent_agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        requested_agent_instance_id: Some("child-idempotent".into()),
        origin_tool_call_id: None,
    };
    let first = runtime.create_agent(create.clone()).await.unwrap();
    let second = runtime.create_agent(create).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(commits.commands.lock().await.len(), 2, "root + one child");

    model.push_text("one execution").await;
    let input = SendAgentInputRequest {
        request_id: "input-idempotent".into(),
        session_id: "session-1".into(),
        agent_instance_id: "child-idempotent".into(),
        caller_agent_instance_id: Some("root".into()),
        source_turn_id: None,
        message_id: "message-idempotent".into(),
        content: MessageContent::String("run once".into()),
        delivery: AgentInputDelivery::StartWhenIdle,
    prompt_resources: None,
    active_tool_names: None,
};
    let first_report = runtime
        .run_agent(input.clone())
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    let duplicate_report = runtime
        .run_agent(input)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(first_report.report_id, duplicate_report.report_id);
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn duplicate_detached_input_delivers_the_completed_report_without_rerun() {
    let (runtime, _commits, model) = attached_runtime().await;
    model.push_text("completed once").await;
    let input = SendAgentInputRequest {
        request_id: "input-completed-detached".into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        caller_agent_instance_id: None,
        source_turn_id: None,
        message_id: "message-completed-detached".into(),
        content: MessageContent::String("run once".into()),
        delivery: AgentInputDelivery::StartWhenIdle,
    prompt_resources: None,
    active_tool_names: None,
};
    let report = runtime
        .run_agent(input.clone())
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();

    let receipt = runtime
        .send_agent_input_detached(input, "root".into())
        .await
        .unwrap();
    assert_eq!(
        receipt.disposition,
        piko_protocol::InputDisposition::Duplicate
    );
    let inbox = runtime
        .agent_inbox("session-1".into(), "root".into())
        .await
        .unwrap();
    assert_eq!(inbox.items.len(), 1);
    assert_eq!(inbox.items[0].report.report_id, report.report_id);
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn sibling_messaging_is_rejected_by_runtime_policy() {
    let (runtime, _commits, _model) = attached_runtime().await;
    for child in ["child-a", "child-b"] {
        runtime
            .create_agent(CreateAgentRequest {
                request_id: format!("create-{child}"),
                session_id: "session-1".into(),
                parent_agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                requested_agent_instance_id: Some(child.into()),
                origin_tool_call_id: None,
            })
            .await
            .unwrap();
    }

    let error = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "sibling-message".into(),
            session_id: "session-1".into(),
            agent_instance_id: "child-b".into(),
            caller_agent_instance_id: Some("child-a".into()),
            source_turn_id: None,
            message_id: "sibling-message".into(),
            content: MessageContent::String("not allowed".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .expect_err("siblings must not acquire arbitrary routing capability");
    assert_eq!(error, orchd_api::AgentApiError::AgentUnauthorized);
}

#[tokio::test]
async fn existing_agent_keeps_resolved_spec_snapshot_after_registry_update() {
    let (runtime, _commits, model) = attached_runtime().await;
    runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-snapshot".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("snapshot-child".into()),
            origin_tool_call_id: None,
        })
        .await
        .unwrap();
    let mut updated = test_agent();
    updated.base_system_prompt = "updated globally".into();
    runtime.register_agent(updated).await;
    model.push_text("done").await;
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "run-snapshot".into(),
            session_id: "session-1".into(),
            agent_instance_id: "snapshot-child".into(),
            caller_agent_instance_id: Some("root".into()),
            source_turn_id: None,
            message_id: "message-snapshot".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
    assert_eq!(model.requests().await[0].system_prompt, "test");
}

#[tokio::test]
async fn follow_up_runs_as_a_later_execution_on_the_same_agent() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model.push_text("follow-up run").await;

    let first = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "first-run".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-first".into(),
            content: MessageContent::String("first".into()),
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
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let follow_up = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "follow-up-run".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-follow-up".into(),
            content: MessageContent::String("follow up".into()),
            delivery: AgentInputDelivery::FollowUp,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
    assert_eq!(first.disposition, piko_protocol::InputDisposition::Accepted);
    assert_eq!(
        follow_up.disposition,
        piko_protocol::InputDisposition::Queued
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::InputQueued { queued_input, .. }
            if queued_input.queued_input_id == "follow-up-run"
    )));
    commits.fail_next_queued_start();
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();

    for _ in 0..200 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        if snapshot
            .latest_report
            .as_ref()
            .is_some_and(|report| report.summary == "follow-up run")
            && matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
        {
            assert_eq!(model.call_count().await, 2);
            assert_eq!(
                commits
                    .commands
                    .lock()
                    .await
                    .iter()
                    .filter(|command| matches!(
                        command,
                        AgentDurableCommand::QueuedInputStarted { queued_input_id, .. }
                            if queued_input_id == "follow-up-run"
                    ))
                    .count(),
                1
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("follow-up Execution did not complete");
}

#[tokio::test]
async fn queued_follow_up_can_be_cancelled_before_it_starts() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "cancel-queue-active".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "cancel-queue-active-message".into(),
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
    let queued = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "cancel-queued-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: Some("turn-cancel-queued".into()),
            message_id: "cancel-queued-message".into(),
            content: MessageContent::String("never run".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    assert_eq!(
        queued.receipt.disposition,
        piko_protocol::InputDisposition::Queued
    );
    let cancelled = runtime
        .cancel_agent_input(
            "session-1".into(),
            "root".into(),
            "cancel-queued-input".into(),
        )
        .await
        .unwrap();
    assert!(cancelled.accepted);
    assert!(matches!(queued.wait().await, Err(orchd_api::AgentApiError::Cancelled)));
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::QueuedInputCancelled { queued_input_id, .. }
            if queued_input_id == "cancel-queued-input"
    )));
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
    assert_eq!(model.call_count().await, 1);
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
        requests[1].transcript.iter().any(|message| matches!(
            message,
            piko_protocol::Message::Assistant { content, .. }
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

async fn wait_until_idle(runtime: &AgentRuntime) {
    for _ in 0..100 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .expect("snapshot")
            .expect("root");
        if matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
            && snapshot.latest_report.is_some()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("AgentActor did not return to Idle");
}
