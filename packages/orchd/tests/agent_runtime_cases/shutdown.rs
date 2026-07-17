#[tokio::test]
async fn cancelling_attached_spawn_cancels_child_execution() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let cancellation = tokio_util::sync::CancellationToken::new();
    let context = ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-cancel".into(),
        cancellation: Some(cancellation.clone()),
        agent_id: "main".into(),
        tool_set_ids: Vec::new(),
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
    };
    let spawned = tokio::spawn(async move {
        provider
            .execute(
                piko_protocol::ToolCall {
                    id: "call-cancel".into(),
                    name: "spawn_agent".into(),
                    arguments: serde_json::json!({
                        "agent_spec_id": "main",
                        "prompt": "wait"
                    }),
                    partial_json: None,
                },
                context,
            )
            .await
    });
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    cancellation.cancel();
    let result = spawned.await.unwrap();
    assert!(!result.ok);
    assert_eq!(result.error.unwrap().message, "operation cancelled");

    for _ in 0..100 {
        let child = runtime
            .list_agents("session-1".into())
            .await
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.identity.parent_agent_instance_id.as_deref() == Some("root"))
            .unwrap();
        if let Some(report) = child.latest_report {
            assert!(matches!(
                report.outcome,
                piko_protocol::ExecutionOutcome::Cancelled { .. }
            ));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("cancelled child did not reach a terminal report");
}

#[tokio::test]
async fn session_detach_cancels_and_drains_active_executions() {
    let (runtime, _commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "shutdown-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-shutdown".into(),
            content: MessageContent::String("wait".into()),
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
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        runtime.detach_agent_session("session-1".into()),
    )
    .await
    .expect("detach must be bounded")
    .expect("detach must drain");
    assert_eq!(
        runtime.list_agents("session-1".into()).await.unwrap_err(),
        orchd_api::AgentApiError::SessionNotAttached
    );
}
