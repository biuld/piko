struct StrictCreateCommitPort {
    revision: AtomicU64,
    specs: Mutex<std::collections::HashMap<String, AgentSpec>>,
}

impl StrictCreateCommitPort {
    fn with_agent(agent_instance_id: &str, spec: AgentSpec) -> Self {
        Self {
            revision: AtomicU64::new(0),
            specs: Mutex::new(std::collections::HashMap::from([(
                agent_instance_id.into(),
                spec,
            )])),
        }
    }
}

#[async_trait]
impl AgentCommitPort for StrictCreateCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, spec, .. } => {
                let mut specs = self.specs.lock().await;
                match specs.get(&identity.agent_instance_id) {
                    Some(existing) if existing != &spec => {
                        return Err(CommitError::IdempotencyConflict);
                    }
                    Some(_) => {}
                    None => {
                        specs.insert(identity.agent_instance_id.clone(), spec);
                    }
                }
                identity.agent_instance_id
            }
            _ => String::new(),
        };
        Ok(AgentCommitAck {
            session_id: session_id.into(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}

#[tokio::test]
async fn recovered_follow_up_retry_is_deduplicated_while_running() {
    let model = Arc::new(FauxProvider::new());
    model
        .push_response(CannedResponse::waiting_for_cancel())
        .await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let queued = piko_protocol::AgentInput {
        input_id: "queued-retry".into(),
        request_id: "queued-retry".into(),
        session_id: "session-queued-retry".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::System,
        delivery: AgentInputDelivery::FollowUp,
        content: MessageContent::String("continue".into()),
        submitted_at: 1,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let root = AgentInstanceIdentity {
        session_id: queued.session_id.clone(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: queued.session_id.clone(),
            root: root.clone(),
            recovered_agents: vec![AgentRecoveryState {
                identity: root,
                spec: test_agent(),
                lifecycle: AgentInstanceLifecycle::Open,
                transcript: Vec::new(),
                head_message_id: None,
                inbox: Vec::new(),
                latest_report: None,
                execution_reports: Vec::new(),
                queued_inputs: vec![queued.clone()],
                pending_detached_deliveries: Vec::new(),
            }],
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                )),
            },
        })
        .await
        .unwrap();
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }

    let receipt = runtime.submit_agent_input(queued).await.unwrap();
    assert_eq!(
        receipt.disposition,
        piko_protocol::AgentInputDisposition::PendingFollowUp
    );
    let snapshot = runtime
        .agent_snapshot("session-queued-retry".into(), "root".into())
        .await
        .unwrap()
        .unwrap();
    assert!(snapshot.pending_follow_up_ids.is_empty());
    assert_eq!(model.call_count().await, 1);
    assert_eq!(
        agents
            .commands
            .lock()
            .await
            .iter()
            .filter(|command| matches!(
                command,
                AgentDurableCommand::AgentInputProcessingStarted { input, .. }
                    if input.input_id == "queued-retry"
            ))
            .count(),
        1
    );
    runtime
        .interrupt_agent("session-queued-retry".into(), "root".into())
        .await
        .unwrap();
}
