#[derive(Default)]
struct FailAfterFirstPromptAssemblyPort {
    calls: AtomicU64,
}

#[async_trait]
impl PromptAssemblyPort for FailAfterFirstPromptAssemblyPort {
    async fn assemble_prompt(
        &self,
        request: PromptAssemblyRequest,
    ) -> Result<piko_protocol::SemanticRunPrompt, piko_orchd_api::AgentApiError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
            return Err(piko_orchd_api::AgentApiError::PromptAssemblyFailed(
                "injected permanent failure".into(),
            ));
        }
        Ok(piko_protocol::SemanticRunPrompt {
            source_digest: "first".into(),
            assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            blocks: vec![test_prompt_block(request.agent_spec.base_instructions)],
            cache_plan: Default::default(),
        })
    }
}

#[tokio::test]
async fn permanent_follow_up_failure_retries_its_durable_cancellation() {
    let model = Arc::new(FauxProvider::new());
    model
        .push_response(CannedResponse::waiting_for_cancel())
        .await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-terminal-retry".into(),
            root: AgentInstanceIdentity {
                session_id: "session-terminal-retry".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                ))
                .with_prompt(Arc::new(FailAfterFirstPromptAssemblyPort::default())),
            },
        })
        .await
        .unwrap();
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "active-before-failure".into(),
            session_id: "session-terminal-retry".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "active-message".into(),
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
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "permanent-follow-up".into(),
            session_id: "session-terminal-retry".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "permanent-message".into(),
            content: MessageContent::String("will fail".into()),
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    agents.fail_next_input_cancellation();
    runtime
        .interrupt_agent("session-terminal-retry".into(), "root".into())
        .await
        .unwrap();

    for _ in 0..200 {
        let snapshot = runtime
            .agent_snapshot("session-terminal-retry".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        let cancellation_committed = agents.commands.lock().await.iter().any(|command| {
            matches!(
                command,
                AgentDurableCommand::AgentInputDispositionChanged { change }
                    if change.input_id == "permanent-follow-up"
                        && change.disposition == piko_protocol::AgentInputDisposition::Cancelled
            )
        });
        if cancellation_committed && snapshot.pending_follow_up_ids.is_empty() {
            assert_eq!(model.call_count().await, 1);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("failed follow-up cancellation was not retried to durability");
}
