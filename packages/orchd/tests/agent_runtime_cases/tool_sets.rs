#[tokio::test]
async fn declared_tool_sets_expand_into_model_catalog() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("catalog ok").await;

    let mut agents = std::collections::HashMap::new();
    agents.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            name: "main".into(),
            role: "root".into(),
            description: None,
            base_system_prompt: "test".into(),
            model: Some("faux-1".into()),
            thinking_level: None,
            tool_set_ids: vec![
                "todo".into(),
                "workspace".into(),
                "user_interaction".into(),
                "multi_agent".into(),
            ],
            active_tool_names: None,
        },
    );

    let mut config = piko_protocol::config::OrchdConfig::single_provider("faux", "test", "faux-1");
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn llmd::gateway::LlmGateway>,
        config,
    )
    .await;
    runtime
        .register_tool_provider(Box::new(UserInteractionProvider::new()))
        .await;
    runtime
        .register_tool_set(piko_protocol::tools::ToolSet {
            id: "user_interaction".into(),
            name: "User Interaction".into(),
            description: None,
            metadata: None,
            policy: None,
            tools: vec![piko_protocol::tools::ToolSetToolRef::ProviderNamespace {
                provider_id: "user_interaction".into(),
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
            session_id: "session-catalog".into(),
            root: AgentInstanceIdentity {
                session_id: "session-catalog".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents_port as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "catalog-run".into(),
            session_id: "session-catalog".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-catalog".into(),
            content: MessageContent::String("tools?".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();

    let requests = model.requests().await;
    assert_eq!(requests.len(), 1);
    let names: std::collections::BTreeSet<_> =
        requests[0].tools.iter().map(|tool| tool.name.as_str()).collect();
    for expected in [
        "todo_write",
        "todo_read",
        "read",
        "bash",
        "edit",
        "write",
        "ask_user",
        "request_user_input",
        "spawn_agent",
        "spawn_agent_detached",
        "send_agent_message",
        "get_agent_status",
        "collect_agent_reports",
        "close_agent",
        "reopen_agent",
    ] {
        assert!(
            names.contains(expected),
            "missing {expected} in catalog {names:?}"
        );
    }
}

#[tokio::test]
async fn undeclared_tool_sets_are_absent_from_model_catalog() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("sparse catalog").await;

    let mut agents = std::collections::HashMap::new();
    agents.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            name: "main".into(),
            role: "root".into(),
            description: None,
            base_system_prompt: "test".into(),
            model: Some("faux-1".into()),
            thinking_level: None,
            tool_set_ids: vec!["todo".into()],
            active_tool_names: None,
        },
    );

    let mut config = piko_protocol::config::OrchdConfig::single_provider("faux", "test", "faux-1");
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn llmd::gateway::LlmGateway>,
        config,
    )
    .await;

    let agents_port = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-sparse".into(),
            root: AgentInstanceIdentity {
                session_id: "session-sparse".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents_port as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "sparse-run".into(),
            session_id: "session-sparse".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-sparse".into(),
            content: MessageContent::String("tools?".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();

    let requests = model.requests().await;
    let names: std::collections::BTreeSet<_> =
        requests[0].tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(
        names,
        ["todo_read", "todo_write"].into_iter().collect()
    );
}
