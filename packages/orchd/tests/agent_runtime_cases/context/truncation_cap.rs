// ---- F-05 transcript-cap settings wiring e2e ----

#[tokio::test]
async fn transcript_max_tool_output_tokens_reaches_the_model_view() {
    let model = Arc::new(FauxProvider::new());

    let mut agents = std::collections::HashMap::new();
    let mut agent = test_agent();
    agent.tool_set_ids = vec!["bloat".into()];
    agents.insert("main".into(), agent);

    let mut config = test_orchd_config();
    // Tiny cap: ~300 characters of retained text, far below the 24k default.
    config.transcript_max_tool_output_tokens = 100;
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        config,
    )
    .await;
    runtime
        .register_tool_provider(Box::new(BloatProvider::new(200_000)))
        .await;
    runtime
        .register_tool_set(ToolSet {
            id: "bloat".into(),
            name: "Bloat".into(),
            description: None,
            feature: None,
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "bloat".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        })
        .await;

    let agents_port = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-transcript-cap".into(),
            root: AgentInstanceIdentity {
                session_id: "session-transcript-cap".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents_port as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    model
        .push_response(CannedResponse::tool_calls(vec![ToolCall {
            id: "call-bloat".into(),
            name: "bloat_emit".into(),
            arguments: serde_json::json!({}),
            partial_json: None,
        }]))
        .await;
    model.push_text("done").await;

    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "transcript-cap-run".into(),
            session_id: "session-transcript-cap".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "transcript-cap-message".into(),
            content: MessageContent::String("run".into()),
            delivery: piko_protocol::AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    let requests = model.requests().await;
    let second = &requests[1];
    let marker = second
        .conversation
        .items
        .iter()
        .find_map(|item| match &item.kind {
            piko_llmd::gateway::ConversationItemKind::ToolResult { content, .. } => content.iter().find_map(|block| match block {
                ContentBlock::Text { text } if text.contains("Tool output truncated") => {
                    Some(text.clone())
                }
                _ => None,
            }),
            _ => None,
        })
        .expect("second model request must contain a truncation marker");
    assert!(marker.contains("of 200000 characters"), "{marker}");
    // Cap 100 tokens ≈ 300 bytes of retained text, far below the 24k default
    // (which would retain ~72,000 characters).
    assert!(
        marker.contains("retained ") && marker.len() < 1_000,
        "configured cap must reach the model view; marker={marker}"
    );
}
