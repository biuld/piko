#[tokio::test]
async fn lifecycle_and_activity_are_independent() {
    let (runtime, _commits, model) = attached_runtime().await;
    let closed = runtime
        .close_agent(AgentLifecycleRequest {
            request_id: "close-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("close root");
    assert_eq!(closed.lifecycle, AgentInstanceLifecycle::Closed);

    let snapshot = runtime
        .agent_snapshot("session-1".into(), "root".into())
        .await
        .expect("snapshot")
        .expect("root snapshot");
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Closed);
    assert!(matches!(
        snapshot.activity,
        piko_protocol::AgentActivity::Idle
    ));
    let rejected = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "closed-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "closed-message".into(),
            content: MessageContent::String("must reject".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .expect_err("closed AgentInstance must reject input");
    assert_eq!(rejected, piko_orchd_api::AgentApiError::AgentClosed);

    let reopened = runtime
        .reopen_agent(AgentLifecycleRequest {
            request_id: "reopen-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("reopen root");
    assert_eq!(reopened.lifecycle, AgentInstanceLifecycle::Open);
    model.push_text("reused after reopen").await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "reopened-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "reopened-message".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::Auto,
        prompt_resources: None,
        active_tool_names: None,
})
        .await
        .unwrap();
}

#[tokio::test]
async fn each_run_gets_one_fresh_prompt_from_its_resource_snapshot() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("first").await;
    model.push_text("second").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let prompts = Arc::new(RecordingPromptAssemblyPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-prompt-refresh".into(),
            root: AgentInstanceIdentity {
                session_id: "session-prompt-refresh".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                )
                .with_prompt(prompts.clone() as Arc<dyn PromptAssemblyPort>),
            },
        })
        .await
        .unwrap();

    for (suffix, context) in [("first", "day one"), ("second", "day two")] {
        runtime
            .send_agent_input(SendAgentInputRequest {
                request_id: format!("request-{suffix}"),
                session_id: "session-prompt-refresh".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                source_turn_id: None,
                message_id: format!("message-{suffix}"),
                content: MessageContent::String(suffix.into()),
                delivery: AgentInputDelivery::Auto,
                prompt_resources: Some(piko_protocol::PromptResourceSnapshot {
                    blocks: vec![test_prompt_block(context)],
                    world_state: None,
                    user_mentions: Vec::new(),
                    cache_policy: Default::default(),
                }),
                active_tool_names: None,
            })
            .await
            .unwrap();
    }

    let requests = model.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(gateway_prompt_text(&requests[0]), "test|day one");
    assert_eq!(gateway_prompt_text(&requests[1]), "test|day two");
    assert_eq!(prompts.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn canonical_submission_preserves_distinct_root_input_identity() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("completed").await;
    let input = piko_protocol::AgentInput {
        input_id: "input-distinct-root".into(),
        request_id: "request-distinct-root".into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: AgentInputDelivery::StartWhenIdle,
        content: MessageContent::String("start with a distinct identity".into()),
        submitted_at: 10,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };

    let receipt = runtime
        .submit_agent_input(input.clone())
        .await
        .expect("canonical input should start a run");
    assert_eq!(receipt.input_id, input.input_id);
    let report = runtime
        .wait_agent_input_completion(
            input.session_id.clone(),
            input.agent_instance_id.clone(),
            input.input_id.clone(),
        )
        .await
        .expect("root input should publish its terminal report");
    assert_eq!(report.root_input_id, input.input_id);

    let commands = commits.commands.lock().await;
    let started = commands.iter().find_map(|command| match command {
        piko_protocol::AgentDurableCommand::AgentInputProcessingStarted {
            input: started,
            ..
        } => Some(started),
        _ => None,
    });
    let started = started.expect("run start should carry the canonical input");
    assert_eq!(started.input_id, input.input_id);
    assert_eq!(started.request_id, input.request_id);
    assert_eq!(started.agent_instance_id, input.agent_instance_id);
    assert_eq!(started.content, input.content);
    assert_eq!(started.submitted_at, input.submitted_at);
}

#[path = "behavior/follow_up.rs"]
mod follow_up;
#[path = "behavior/lifecycle.rs"]
mod lifecycle;
#[path = "behavior/requests.rs"]
mod requests;
#[path = "behavior/steer.rs"]
mod steer;

async fn wait_until_idle(runtime: &AgentRuntime) {
    for _ in 0..100 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .expect("snapshot")
            .expect("root");
        if matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
            && snapshot.latest_report.is_some()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("AgentActor did not return to Idle");
}
