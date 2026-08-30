use super::*;

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
            root_input_id: None,
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
            root_input_id: None,
            message_id: "message-follow-up".into(),
            content: MessageContent::String("follow up".into()),
            delivery: AgentInputDelivery::FollowUp,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
    assert_eq!(first.disposition, piko_protocol::AgentInputDisposition::AppliedAsRoot);
    assert_eq!(
        follow_up.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.input_id == "follow-up-run"
                && admission.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp
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
                        AgentDurableCommand::AgentInputProcessingStarted { input, .. }
                            if input.input_id == "follow-up-run"
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
async fn canonical_follow_up_retry_preserves_distinct_input_identity() {
    let (runtime, _commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "canonical-follow-up-active".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "canonical-follow-up-active-message".into(),
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

    let input = piko_protocol::AgentInput {
        input_id: "canonical-follow-up-input".into(),
        request_id: "canonical-follow-up-request".into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: AgentInputDelivery::FollowUp,
        content: MessageContent::String("retry this follow-up".into()),
        submitted_at: 10,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let first = runtime.submit_agent_input(input.clone()).await.unwrap();
    assert_eq!(
        first.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    assert_eq!(first.input_id, input.input_id);

    let retry = runtime.submit_agent_input(input).await.unwrap();
    assert_eq!(retry.disposition, piko_protocol::AgentInputDisposition::PendingFollowUp);
    assert_eq!(retry.input_id, "canonical-follow-up-input");

    let cancelled = runtime
        .cancel_agent_input(
            "session-1".into(),
            "root".into(),
            "canonical-follow-up-input".into(),
        )
        .await
        .unwrap();
    assert!(cancelled.accepted);
    assert_eq!(cancelled.input_id, "canonical-follow-up-input");
    assert_eq!(cancelled.request_id, "canonical-follow-up-request");
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
}

#[tokio::test]
async fn queued_follow_up_keeps_root_identity_when_it_becomes_active() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        runtime.send_agent_input(SendAgentInputRequest {
            request_id: "active-before-queue".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "active-before-queue-message".into(),
            content: MessageContent::String("first".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        }),
    )
    .await
    .expect("active-before-queue admission timed out")
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while model.call_count().await < 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("first root did not start: {:?}", commits.commands));
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "queued-root-request".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "queued-root-message".into(),
            content: MessageContent::String("second".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while model.call_count().await < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("queued root did not advance: {:?}", commits.commands));

    let steer = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "steer-queued-root".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "steer-queued-root-message".into(),
            content: MessageContent::String("steer second".into()),
            delivery: AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .expect("the active queued follow-up must accept steer");
    assert_eq!(steer.disposition, piko_protocol::AgentInputDisposition::PendingSteer);
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
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
            root_input_id: None,
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
        .send_agent_input(SendAgentInputRequest {
            request_id: "cancel-queued-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: Some("turn-cancel-queued".into()),
            message_id: "cancel-queued-message".into(),
            content: MessageContent::String("never run".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    assert_eq!(
        queued.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    let (started, cancelled) = tokio::join!(
        runtime.wait_agent_input_started(
            "session-1".into(),
            "root".into(),
            "cancel-queued-input".into(),
        ),
        async {
            tokio::task::yield_now().await;
            runtime
                .cancel_agent_input(
                    "session-1".into(),
                    "root".into(),
                    "cancel-queued-input".into(),
                )
                .await
        }
    );
    let cancelled = cancelled.unwrap();
    assert!(cancelled.accepted);
    assert!(matches!(started, Err(piko_orchd_api::AgentApiError::Cancelled)));
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::AgentInputDispositionChanged { change }
            if change.input_id == "cancel-queued-input"
                && change.disposition == piko_protocol::AgentInputDisposition::Cancelled
    )));
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();
    assert_eq!(model.call_count().await, 1);
}
