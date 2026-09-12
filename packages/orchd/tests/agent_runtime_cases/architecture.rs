#[tokio::test]
async fn bootstrap_runtime_does_not_retain_itself_through_built_in_tools() {
    let runtime = AgentRuntime::bootstrap(
        Arc::new(FauxProvider::new()) as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        test_orchd_config(),
    )
    .await;

    let weak = Arc::downgrade(&runtime);
    drop(runtime);

    assert!(
        weak.upgrade().is_none(),
        "built-in tool providers must not keep AgentRuntime alive"
    );
}

#[tokio::test]
async fn cancelling_a_dispatched_attached_spawn_interrupts_its_child() {
    let model = Arc::new(FauxProvider::new());
    let mut config = test_orchd_config();
    config.agents.get_mut("main").unwrap().tool_set_ids = vec!["multi_agent".into()];
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        config,
    )
    .await;
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "attached-cancel".into(),
            root: AgentInstanceIdentity {
                session_id: "attached-cancel".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: Arc::new(CollectingAgentCommitPort::default()),
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                )),
            },
        })
        .await
        .unwrap();
    model
        .push_response(CannedResponse::tool_calls(vec![piko_protocol::ToolCall {
            id: "spawn".into(),
            name: "spawn_agent".into(),
            arguments: serde_json::json!({ "agent_spec_id": "main", "prompt": "wait" }),
            partial_json: None,
        }]))
        .await;
    model
        .push_response(CannedResponse::waiting_for_cancel())
        .await;

    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "parent".into(),
            session_id: "attached-cancel".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "parent-message".into(),
            content: MessageContent::String("spawn".into()),
            delivery: AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while model.call_count().await < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("attached child did not start");

    runtime
        .interrupt_agent("attached-cancel".into(), "root".into())
        .await
        .unwrap();
    runtime
        .wait_agent_input_completion("attached-cancel".into(), "root".into(), "parent".into())
        .await
        .unwrap();

    let child = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let child = runtime
                .list_agents("attached-cancel".into())
                .await
                .unwrap()
                .into_iter()
                .find(|agent| agent.identity.agent_instance_id != "root")
                .unwrap();
            if child.latest_report.is_some() {
                return child;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("attached child did not finish");
    assert!(matches!(
        child.latest_report.unwrap().outcome,
        piko_protocol::AgentWorkOutcome::Cancelled { .. }
    ));
    runtime
        .detach_agent_session("attached-cancel".into())
        .await
        .unwrap();
}

struct BlockingLifecycleCommit {
    inner: CollectingAgentCommitPort,
    close_entered: Semaphore,
    release_close: Semaphore,
}

#[async_trait]
impl AgentCommitPort for BlockingLifecycleCommit {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<piko_protocol::AgentCommitAck, CommitError> {
        let block = matches!(
            &command,
            AgentDurableCommand::SetLifecycle {
                lifecycle: AgentInstanceLifecycle::Closed,
                ..
            }
        );
        let ack = self.inner.commit_agent_command(session_id, command).await?;
        if block {
            self.close_entered.add_permits(1);
            self.release_close.acquire().await.unwrap().forget();
        }
        Ok(ack)
    }
}

#[tokio::test]
async fn lifecycle_commands_commit_and_apply_in_actor_order() {
    let runtime = Arc::new(AgentRuntime::new(
        Arc::new(FauxProvider::new()) as Arc<dyn piko_llmd::gateway::InferenceGateway>,
    ));
    runtime.register_agent(test_agent()).await;
    let commit = Arc::new(BlockingLifecycleCommit {
        inner: CollectingAgentCommitPort::default(),
        close_entered: Semaphore::new(0),
        release_close: Semaphore::new(0),
    });
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "lifecycle-order".into(),
            root: AgentInstanceIdentity {
                session_id: "lifecycle-order".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: commit.clone(),
                executions: SessionExecutionPorts::new(Arc::new(
                    CollectingExecutionCommitPort::new(),
                )),
            },
        })
        .await
        .unwrap();
    let close_runtime = Arc::clone(&runtime);
    let close = tokio::spawn(async move {
        close_runtime
            .close_agent(AgentLifecycleRequest {
                request_id: "close".into(),
                session_id: "lifecycle-order".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
            })
            .await
    });
    commit.close_entered.acquire().await.unwrap().forget();
    let reopen_runtime = Arc::clone(&runtime);
    let reopen = tokio::spawn(async move {
        reopen_runtime
            .reopen_agent(AgentLifecycleRequest {
                request_id: "reopen".into(),
                session_id: "lifecycle-order".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
            })
            .await
    });
    commit.release_close.add_permits(1);
    close.await.unwrap().unwrap();
    reopen.await.unwrap().unwrap();

    let snapshot = runtime
        .agent_snapshot("lifecycle-order".into(), "root".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Open);
    let lifecycle_commands = commit
        .inner
        .commands
        .lock()
        .await
        .iter()
        .filter_map(|command| match command {
            AgentDurableCommand::SetLifecycle { lifecycle, .. } => Some(*lifecycle),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle_commands,
        vec![AgentInstanceLifecycle::Closed, AgentInstanceLifecycle::Open],
        "durable lifecycle order must match actor application order"
    );
    runtime
        .detach_agent_session("lifecycle-order".into())
        .await
        .unwrap();
}

#[tokio::test]
async fn conditional_interrupt_does_not_cancel_a_successor_root() {
    let (runtime, _commits, model) = attached_runtime().await;
    model
        .push_response(CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "current-root".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "current-message".into(),
            content: MessageContent::String("wait".into()),
            delivery: AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while model.call_count().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("root did not start");

    let stale = runtime
        .interrupt_agent_if_active("session-1".into(), "root".into(), "previous-root".into())
        .await
        .unwrap();
    assert!(!stale.accepted);

    let current = runtime
        .interrupt_agent("session-1".into(), "root".into())
        .await
        .unwrap();
    assert!(current.accepted, "stale cleanup cancelled the current root");
    runtime
        .wait_agent_input_completion("session-1".into(), "root".into(), "current-root".into())
        .await
        .unwrap();
}
