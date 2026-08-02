// ---- F-05 context tools: get_context_remaining / new_context_window e2e ----

#[tokio::test]
async fn context_budget_tools_report_remaining_and_request_fresh_window() {
    let model = Arc::new(FauxProvider::new());

    let mut agents = std::collections::HashMap::new();
    let mut agent = test_agent();
    agent.tool_set_ids = vec!["context".into()];
    agents.insert("main".into(), agent);

    let mut config = piko_protocol::config::OrchdConfig::single_provider("faux", "test", "faux-1");
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::LlmGateway>,
        config,
    )
    .await;

    let new_windows = Arc::new(Mutex::new(Vec::new()));
    let callback_arc = Arc::clone(&new_windows);
    runtime
        .context_tools()
        .set_callbacks(piko_orchd::tools::ContextToolsCallbacks {
            new_context_window: Some(Arc::new(move |session_id, agent_instance_id| {
                let callback_arc = Arc::clone(&callback_arc);
                Box::pin(async move {
                    callback_arc
                        .lock()
                        .await
                        .push(format!("{session_id}/{agent_instance_id}"));
                    Ok(())
                })
            })),
        })
        .await;

    let agents_port = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-context-tools".into(),
            root: AgentInstanceIdentity {
                session_id: "session-context-tools".into(),
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
            id: "call-remaining".into(),
            name: "get_context_remaining".into(),
            arguments: serde_json::json!({}),
            partial_json: None,
        }]))
        .await;
    model
        .push_response(CannedResponse::tool_calls(vec![ToolCall {
            id: "call-window".into(),
            name: "new_context_window".into(),
            arguments: serde_json::json!({}),
            partial_json: None,
        }]))
        .await;
    model.push_text("done").await;

    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "context-tools-run".into(),
            session_id: "session-context-tools".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "context-tools-message".into(),
            content: MessageContent::String("run".into()),
            delivery: piko_protocol::AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    let requests = model.requests().await;
    assert!(
        requests.len() >= 3,
        "expected three model requests; got {}",
        requests.len()
    );

    // Request 2 carries the get_context_remaining result with a real estimate.
    let remaining_result = requests[1]
        .transcript
        .iter()
        .find_map(|message| match message {
            Message::ToolResult {
                tool_name: Some(name),
                content,
                ..
            } if name == "get_context_remaining" => Some(content),
            _ => None,
        })
        .expect("second model request must contain the context-remaining result");
    let remaining_text = remaining_result
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        remaining_text.contains("tokens_left"),
        "result must report tokens_left; got {remaining_text}"
    );

    // The fresh-window callback fired exactly once with the root identity.
    let windows = new_windows.lock().await.clone();
    assert_eq!(windows, vec!["session-context-tools/root".to_string()]);

    // The running execution trimmed to the latest user message: the final
    // request still starts from the user instruction and the run continued.
    let last = requests.last().unwrap();
    assert!(
        matches!(
            last.transcript.first(),
            Some(Message::User {
                content: MessageContent::String(text),
                ..
            }) if text == "run"
        ),
        "fresh-window run must keep the latest user message first"
    );
    assert!(
        last.transcript
            .iter()
            .any(|message| matches!(
                message,
                Message::ToolResult {
                    tool_name: Some(name),
                    ..
                } if name == "new_context_window"
            )),
        "fresh-window tool result must remain in the running transcript"
    );
}

