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
    assert!(matches!(queued.wait().await, Err(piko_orchd_api::AgentApiError::Cancelled)));
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

