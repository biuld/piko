#[tokio::test]
async fn recovery_preserves_worker_delegation_mode() {
    let model = Arc::new(FauxProvider::new());
    let runtime = AgentRuntime::new(model as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;

    let mut worker = test_agent();
    worker.id = "scout".into();
    worker.name = "Scout".into();
    worker.kind = piko_protocol::AgentKind::Worker;

    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-kind-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    let child = AgentInstanceIdentity {
        session_id: "session-kind-recovery".into(),
        agent_instance_id: "worker".into(),
        agent_spec_id: "scout".into(),
        parent_agent_instance_id: Some("root".into()),
    };

    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-kind-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![
                AgentRecoveryState {
                    identity: root,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: Vec::new(),
                    latest_report: None,
                    execution_reports: Vec::new(),
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
                AgentRecoveryState {
                    identity: child,
                    spec: worker,
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: Vec::new(),
                    latest_report: None,
                    execution_reports: Vec::new(),
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
            ],
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-from-recovered-worker".into(),
            session_id: "session-kind-recovery".into(),
            parent_agent_instance_id: "worker".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("grandchild".into()),
            origin_tool_call_id: None,
        })
        .await
        .expect_err("a recovered worker must remain unable to spawn children");
    assert_eq!(
        error,
        piko_orchd_api::AgentApiError::AgentCannotSpawnChildren
    );
    assert!(!agents.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::Create { identity, .. }
            if identity.agent_instance_id == "grandchild"
    )));
}
