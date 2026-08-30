#[tokio::test]
async fn multi_agent_tools_use_trusted_context_for_attached_and_detached_spawn() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model.push_text("attached report").await;
    model.push_text("detached report").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-exec".into(),
        cancellation: None,
        agent_id: "main".into(),
        agent_role: None,
        agent_kind: piko_protocol::AgentKind::Supervisor,
        tool_set_ids: Vec::new(),
        turn_index: Some(1),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
        context_remaining: None,
    };

    let attached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-attached".into(),
                name: "spawn_agent".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do attached work",
                    "session_id": "forged-session",
                    "parent_agent_instance_id": "forged-parent"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(attached.ok, "attached spawn failed: {:?}", attached.error);
    assert_eq!(
        attached.value.as_ref().unwrap()["summary"],
        "attached report"
    );
    assert!(
        attached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    let terminal_attempts_before_detached = commits.terminal_attempts.load(Ordering::SeqCst);
    commits.fail_next_run_terminal();
    commits.fail_next_report_commit();
    let detached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-detached".into(),
                name: "spawn_agent_detached".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(detached.ok, "detached spawn failed: {:?}", detached.error);
    assert_eq!(detached.value.as_ref().unwrap()["status"], "accepted");
    assert!(
        detached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    for _ in 0..100 {
        if commits.terminal_attempts.load(Ordering::SeqCst) > terminal_attempts_before_detached {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .unwrap()
            .items
            .is_empty()
    );

    for _ in 0..100 {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .expect("root inbox");
        if inbox
            .items
            .iter()
            .any(|item| item.report.summary == "detached report")
        {
            let commands = commits.commands.lock().await;
            let start_index = commands
                .iter()
                .position(|command| {
                    matches!(
                        command,
                        AgentDurableCommand::AgentInputProcessingStarted {
                            detached_recipient_agent_instance_id: Some(recipient),
                            ..
                        } if recipient == "root"
                    )
                })
                .expect("detached registration must be durable");
            let (root_input_id, terminal_index) = commands
                .iter()
                .enumerate()
                .find_map(|(index, command)| match command {
                    AgentDurableCommand::AgentInputProcessingFinished {
                        root_input_id,
                        report,
                        ..
                    } if report.summary == "detached report" => Some((root_input_id, index)),
                    _ => None,
                })
                .expect("detached terminal must be durable");
            let delivery_index = commands
                .iter()
                .position(|command| {
                    matches!(
                        command,
                        AgentDurableCommand::CommitReport { report, .. }
                            if report.summary == "detached report"
                    )
                })
                .expect("detached report must be committed");
            assert!(start_index < terminal_index);
            assert!(terminal_index < delivery_index);
            assert!(
                commands[start_index..terminal_index]
                    .iter()
                    .any(|command| matches!(
                        command,
                        AgentDurableCommand::AgentInputProcessingStarted { root_input_id: started, .. } if started == root_input_id
                    ))
            );
            drop(commands);
            let collected = provider
                .execute(
                    piko_protocol::ToolCall {
                        id: "call-collect".into(),
                        name: "collect_agent_reports".into(),
                        arguments: serde_json::json!({}),
                        partial_json: None,
                    },
                    context,
                )
                .await;
            assert!(collected.ok);
            assert_eq!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]["summary"],
                "detached report"
            );
            assert!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]
                    .get("execution_id")
                    .is_none()
            );
            let consumed = runtime
                .agent_inbox("session-1".into(), "root".into())
                .await
                .unwrap();
            assert!(consumed.items[0].consumed_at.is_some());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("detached report was not delivered to the durable parent inbox");
}

// ---- F-10 v2 collaboration tools ----

fn v2_context() -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-exec-v2".into(),
        cancellation: None,
        agent_id: "main".into(),
        agent_role: None,
        agent_kind: piko_protocol::AgentKind::Supervisor,
        tool_set_ids: Vec::new(),
        turn_index: Some(1),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
        context_remaining: None,
    }
}

#[tokio::test]
async fn worker_multi_agent_provider_rejects_direct_calls() {
    let (runtime, _commits, _model) = attached_runtime().await;
    let provider = MultiAgentToolProvider::new(Arc::new(runtime) as Arc<dyn AgentRuntimeApi>);
    let mut context = v2_context();
    context.agent_kind = piko_protocol::AgentKind::Worker;

    let result = provider
        .execute(v2_call("list_agent_specs", serde_json::json!({})), context)
        .await;
    assert!(!result.ok);
    let error = result.error.expect("worker call must fail");
    assert_eq!(error.code, "agent_cannot_spawn_children");
}

fn v2_call(name: &str, arguments: serde_json::Value) -> piko_protocol::ToolCall {
    piko_protocol::ToolCall {
        id: format!("call-{name}"),
        name: name.into(),
        arguments,
        partial_json: None,
    }
}

async fn spawn_detached_v2(provider: &MultiAgentToolProvider, prompt: &str) -> String {
    let result = provider
        .execute(
            v2_call(
                "spawn_agent_detached",
                serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": prompt,
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(result.ok, "detached spawn failed: {:?}", result.error);
    result.value.as_ref().unwrap()["agent_instance_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn wait_until_activity(
    runtime: &AgentRuntime,
    agent_instance_id: &str,
    activity: piko_protocol::AgentActivity,
) {
    for _ in 0..500 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), agent_instance_id.into())
            .await
            .unwrap()
            .unwrap();
        if snapshot.activity == activity {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("agent {agent_instance_id} did not reach {activity:?}");
}


#[path = "multi_agent/inbox_tests.rs"]
mod inbox_tests;
#[path = "multi_agent/queue_tests.rs"]
mod queue_tests;
#[path = "multi_agent/wait_tests.rs"]
mod wait_tests;
