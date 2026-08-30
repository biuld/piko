use super::*;


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
    assert_eq!(error, piko_orchd_api::AgentApiError::AgentNotFound);
    assert_eq!(commits.commands.lock().await.len(), 1);
}

#[tokio::test]
async fn configured_agent_count_limit_applies_to_new_children() {
    let (runtime, commits, _model) = attached_bootstrapped_runtime_with_limits(2, 8).await;
    runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-count-first".into(),
            session_id: "session-limits".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("count-child".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect("first child is within the configured count");

    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-count-second".into(),
            session_id: "session-limits".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("count-child-2".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect_err("the configured total includes the root");
    assert_eq!(error, piko_orchd_api::AgentApiError::AgentCountLimitExceeded);
    assert_eq!(commits.commands.lock().await.len(), 2, "root + one child");
}

#[tokio::test]
async fn configured_agent_depth_limit_applies_to_new_children() {
    let (runtime, commits, _model) = attached_bootstrapped_runtime_with_limits(8, 2).await;
    runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-depth-first".into(),
            session_id: "session-limits".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("depth-child".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect("first child is within the configured depth");

    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-depth-second".into(),
            session_id: "session-limits".into(),
            parent_agent_instance_id: "depth-child".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("depth-grandchild".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect_err("the configured depth includes the root");
    assert_eq!(error, piko_orchd_api::AgentApiError::AgentDepthLimitExceeded);
    assert_eq!(commits.commands.lock().await.len(), 2, "root + one child");
}

#[tokio::test]
async fn worker_agent_cannot_create_children_but_can_be_spawned() {
    let (runtime, commits, _model) = attached_runtime().await;
    let mut scout = test_agent();
    scout.id = "scout".into();
    scout.name = "Scout".into();
    scout.kind = piko_protocol::AgentKind::Worker;
    runtime.register_agent(scout).await;

    runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-worker".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "scout".into(),
            requested_agent_instance_id: Some("scout-child".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect("supervisors may spawn worker agents");

    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-from-worker".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "scout-child".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("grandchild".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect_err("worker agents cannot create children");
    assert_eq!(
        error,
        piko_orchd_api::AgentApiError::AgentCannotSpawnChildren
    );
    assert_eq!(commits.commands.lock().await.len(), 2, "root + worker child");
    assert!(matches!(
        commits.commands.lock().await.last(),
        Some(AgentDurableCommand::Create { spec, .. })
            if spec.id == "scout" && spec.kind == piko_protocol::AgentKind::Worker
    ));
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
        .wait_sent_agent(input.clone())
        .await
        .unwrap();
    let duplicate_report = runtime
        .wait_sent_agent(input)
        .await
        .unwrap();
    assert_eq!(first_report.report_id, duplicate_report.report_id);
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn duplicate_detached_input_delivers_the_completed_report_without_rerun() {
    let (runtime, commits, model) = attached_runtime().await;
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
        .wait_sent_agent(input.clone())
        .await
        .unwrap();

    let canonical = commits
        .commands
        .lock()
        .await
        .iter()
        .find_map(|command| match command {
            piko_protocol::AgentDurableCommand::AgentInputProcessingStarted { input, .. }
                if input.request_id == "input-completed-detached" =>
            {
                Some(input.clone())
            }
            _ => None,
        })
        .expect("root admission must retain its canonical input");
    let receipt = runtime
        .submit_agent_input_detached(canonical, "root".into())
        .await
        .unwrap();
    assert_eq!(
        receipt.disposition,
        piko_protocol::AgentInputDisposition::AppliedAsRoot
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
    assert_eq!(error, piko_orchd_api::AgentApiError::AgentUnauthorized);
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
    updated.base_instructions = "updated globally".into();
    runtime.register_agent(updated).await;
    model.push_text("done").await;
    runtime
        .send_agent_input(SendAgentInputRequest {
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
    assert_eq!(gateway_prompt_text(&model.requests().await[0]), "test");
}
