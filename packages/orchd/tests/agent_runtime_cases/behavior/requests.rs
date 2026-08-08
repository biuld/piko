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
    assert_eq!(gateway_prompt_text(&model.requests().await[0]), "test");
}

