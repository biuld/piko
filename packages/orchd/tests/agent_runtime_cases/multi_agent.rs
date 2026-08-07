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
                        AgentDurableCommand::RunStarted {
                            detached_recipient_agent_instance_id: Some(recipient),
                            ..
                        } if recipient == "root"
                    )
                })
                .expect("detached registration must be durable");
            let (run_id, terminal_index) = commands
                .iter()
                .enumerate()
                .find_map(|(index, command)| match command {
                    AgentDurableCommand::RunTerminal { run_id, report, .. }
                        if report.summary == "detached report" =>
                    {
                        Some((run_id, index))
                    }
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
                        AgentDurableCommand::RunStarted { run_id: started, .. } if started == run_id
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

#[tokio::test]
async fn v2_message_agent_queue_starts_a_turn_when_idle() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model.push_text("first report").await;

    let child = spawn_detached_v2(&provider, "do first work").await;
    wait_until_activity(
        &runtime,
        &child,
        piko_protocol::AgentActivity::Idle,
    )
    .await;

    let followup = provider
        .execute(
            v2_call(
                "message_agent",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "do second work",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "message_agent failed: {:?}", followup.error);
    assert_eq!(followup.value.as_ref().unwrap()["disposition"], "accepted");
    assert_eq!(followup.value.as_ref().unwrap()["when"], "queue");

    for _ in 0..200 {
        if model.call_count().await == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert_eq!(model.call_count().await, 2);
}

#[tokio::test]
async fn v2_message_agent_queue_while_busy_and_commits_input() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;

    let child = spawn_detached_v2(&provider, "blocking work").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Running).await;

    let followup = provider
        .execute(
            v2_call(
                "message_agent",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "queued work",
                    "when": "queue",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "message_agent failed: {:?}", followup.error);
    assert_eq!(followup.value.as_ref().unwrap()["disposition"], "queued");
    assert_eq!(followup.value.as_ref().unwrap()["when"], "queue");

    let commands = commits.commands.lock().await;
    assert!(commands.iter().any(|command| matches!(
        command,
        AgentDurableCommand::InputQueued { queued_input, .. }
            if queued_input.request.agent_instance_id == child
                && queued_input.request.request_id.starts_with("message:")
    )));
    drop(commands);

    runtime
        .cancel_agent_run("session-1".into(), child.clone())
        .await
        .unwrap();
}

#[tokio::test]
async fn v2_interrupt_agent_cancels_running_and_keeps_agent_usable() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;

    let child = spawn_detached_v2(&provider, "blocking work").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Running).await;

    let interrupt = provider
        .execute(
            v2_call(
                "interrupt_agent",
                serde_json::json!({ "agent_instance_id": child }),
            ),
            v2_context(),
        )
        .await;
    assert!(interrupt.ok, "interrupt failed: {:?}", interrupt.error);
    let value = interrupt.value.as_ref().unwrap();
    assert_eq!(value["previous_activity"], "running");
    assert_eq!(value["accepted"], true);

    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Idle).await;

    model.push_text("after interrupt").await;
    let followup = provider
        .execute(
            v2_call(
                "message_agent",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "continue",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "message_agent after interrupt failed: {:?}", followup.error);
    assert_eq!(followup.value.as_ref().unwrap()["disposition"], "accepted");
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Idle).await;
}

#[tokio::test]
async fn v2_interrupt_agent_idle_is_benign() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model.push_text("quick work").await;

    let child = spawn_detached_v2(&provider, "quick work").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Idle).await;

    let interrupt = provider
        .execute(
            v2_call(
                "interrupt_agent",
                serde_json::json!({ "agent_instance_id": child }),
            ),
            v2_context(),
        )
        .await;
    assert!(interrupt.ok, "idle interrupt failed: {:?}", interrupt.error);
    let value = interrupt.value.as_ref().unwrap();
    assert_eq!(value["previous_activity"], "idle");
    assert_eq!(value["accepted"], false);

    let snapshot = runtime
        .agent_snapshot("session-1".into(), child)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Open);
}

#[tokio::test]
async fn v2_list_agents_returns_depth_sorted_tree() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model.push_text("quick work").await;

    let child = spawn_detached_v2(&provider, "quick work").await;

    let listed = provider
        .execute(
            v2_call("list_agents", serde_json::json!({})),
            v2_context(),
        )
        .await;
    assert!(listed.ok, "list failed: {:?}", listed.error);
    let agents = listed.value.as_ref().unwrap()["agents"]
        .as_array()
        .unwrap();
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0]["agent_instance_id"], "root");
    assert_eq!(agents[1]["agent_instance_id"], child);
    assert_eq!(agents[1]["parent_agent_instance_id"], "root");
}

#[tokio::test]
async fn v2_wait_agent_returns_on_run_finished() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;

    let child = spawn_detached_v2(&provider, "blocking work").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Running).await;

    let wait_task = tokio::spawn({
        let provider = provider.clone();
        async move {
            provider
                .execute(
                    v2_call("wait_agent", serde_json::json!({ "timeout_ms": 2000 })),
                    v2_context(),
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runtime
        .cancel_agent_run("session-1".into(), child.clone())
        .await
        .unwrap();

    let result = wait_task.await.unwrap();
    assert!(result.ok, "wait failed: {:?}", result.error);
    let value = result.value.as_ref().unwrap();
    assert_eq!(value["timedOut"], false);
    assert_eq!(value["event"]["kind"], "runFinished");
    assert_eq!(value["event"]["agentInstanceId"], child);
    assert!(value["agents"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn v2_wait_agent_times_out_and_consumes_nothing() {
    let (runtime, _commits, _model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);

    let started = std::time::Instant::now();
    let result = provider
        .execute(
            v2_call("wait_agent", serde_json::json!({ "timeout_ms": 100 })),
            v2_context(),
        )
        .await;
    assert!(result.ok, "wait failed: {:?}", result.error);
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(90),
        "wait returned before its timeout"
    );
    let value = result.value.as_ref().unwrap();
    assert_eq!(value["timedOut"], true);
    assert!(value["event"].is_null());

    let inbox = runtime
        .agent_inbox("session-1".into(), "root".into())
        .await
        .unwrap();
    assert!(inbox.items.is_empty());
}

#[tokio::test]
async fn v2_wait_agent_filter_ignores_other_agents_and_matches_target() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;

    let child = spawn_detached_v2(&provider, "blocking work").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Running).await;

    // A filter matching no live agent skips both the child's RunFinished
    // event and root's InboxReport event, so the wait times out.
    let wait_task = tokio::spawn({
        let provider = provider.clone();
        async move {
            provider
                .execute(
                    v2_call(
                        "wait_agent",
                        serde_json::json!({
                            "timeout_ms": 300,
                            "agent_instance_id": "ghost",
                        }),
                    ),
                    v2_context(),
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runtime
        .cancel_agent_run("session-1".into(), child.clone())
        .await
        .unwrap();
    let result = wait_task.await.unwrap();
    assert!(result.ok, "filtered wait failed: {:?}", result.error);
    assert_eq!(result.value.as_ref().unwrap()["timedOut"], true);

    // The same filter on the child itself matches the next RunFinished event.
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    let followup = provider
        .execute(
            v2_call(
                "message_agent",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "block again",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "second message_agent failed: {:?}", followup.error);
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Running).await;

    let wait_task = tokio::spawn({
        let provider = provider.clone();
        let child = child.clone();
        async move {
            provider
                .execute(
                    v2_call(
                        "wait_agent",
                        serde_json::json!({
                            "timeout_ms": 2000,
                            "agent_instance_id": child,
                        }),
                    ),
                    v2_context(),
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runtime
        .cancel_agent_run("session-1".into(), child.clone())
        .await
        .unwrap();
    let result = wait_task.await.unwrap();
    assert!(result.ok, "targeted wait failed: {:?}", result.error);
    let value = result.value.as_ref().unwrap();
    assert_eq!(value["timedOut"], false);
    assert_eq!(value["event"]["kind"], "runFinished");
    assert_eq!(value["event"]["agentInstanceId"], child);
}

#[tokio::test]
async fn v2_consolidated_surface_has_no_redundant_tools() {
    let (runtime, _commits, _model) = attached_runtime().await;
    let provider = MultiAgentToolProvider::new(Arc::new(runtime) as Arc<dyn AgentRuntimeApi>);
    let tools = provider
        .discover(piko_orchd_api::ToolDiscoveryContext {
            agent_id: "main".into(),
            agent_instance_id: Some("root".into()),
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        })
        .await;

    let expected = [
        "list_agent_specs",
        "spawn_agent",
        "spawn_agent_detached",
        "message_agent",
        "collect_agent_reports",
        "close_agent",
        "reopen_agent",
        "interrupt_agent",
        "list_agents",
        "wait_agent",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    let names: std::collections::BTreeSet<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, expected, "multi-agent surface drifted from the F-21 set");

    let message = tools
        .iter()
        .find(|tool| tool.name == "message_agent")
        .unwrap();
    let when = message
        .input_schema
        .get("properties")
        .and_then(|properties| properties.get("when"))
        .expect("message_agent exposes when");
    assert_eq!(when["enum"], serde_json::json!(["queue", "steer"]));
}

#[tokio::test]
async fn f21_list_agent_specs_and_spawn_default_general() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    // Default spawn uses "general" when present.
    let mut general = test_agent();
    general.id = "general".into();
    general.name = "General".into();
    general.description = Some("General helper".into());
    runtime.register_agent(general).await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model.push_text("ok").await;

    let listed = provider
        .execute(v2_call("list_agent_specs", serde_json::json!({})), v2_context())
        .await;
    assert!(listed.ok, "{:?}", listed.error);
    let value = listed.value.as_ref().unwrap();
    assert_eq!(value["default_spawn_spec_id"], "general");
    let ids: Vec<_> = value["specs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"general"));
    assert!(ids.contains(&"main"));

    let spawned = provider
        .execute(
            v2_call(
                "spawn_agent",
                serde_json::json!({ "prompt": "hello from default" }),
            ),
            v2_context(),
        )
        .await;
    assert!(spawned.ok, "{:?}", spawned.error);
    assert_eq!(spawned.value.as_ref().unwrap()["agent_spec_id"], "general");
    assert_eq!(spawned.value.as_ref().unwrap()["attached"], true);
}

#[tokio::test]
async fn f21_spawn_unknown_spec_lists_valid_ids() {
    let (runtime, _commits, _model) = attached_runtime().await;
    let provider = MultiAgentToolProvider::new(Arc::new(runtime) as Arc<dyn AgentRuntimeApi>);
    let result = provider
        .execute(
            v2_call(
                "spawn_agent",
                serde_json::json!({
                    "agent_spec_id": "agents/main",
                    "prompt": "nope",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(!result.ok);
    let err = result.error.as_ref().unwrap();
    assert_eq!(err.code, "agent_spec_not_found");
    assert!(err.message.contains("main") || err.message.contains("coder"));
    assert!(err.message.contains("agents/main"));
}

#[tokio::test]
async fn f21_message_agent_steer_idle_fails_closed() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    model.push_text("done").await;
    let child = spawn_detached_v2(&provider, "quick").await;
    wait_until_activity(&runtime, &child, piko_protocol::AgentActivity::Idle).await;

    let result = provider
        .execute(
            v2_call(
                "message_agent",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "steer me",
                    "when": "steer",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(!result.ok);
    assert_eq!(result.error.as_ref().unwrap().code, "agent_not_running");
}

// ---- F-20 inter-agent completion fragments ----

#[tokio::test]
async fn parent_next_run_injects_unread_completion_before_input() {
    let (runtime, _agents, executions, model) = attached_runtime_ports().await;
    let runtime = Arc::new(runtime);
    model.push_text("detached report").await;
    model.push_text("parent continues").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = v2_context();

    let detached = provider
        .execute(
            v2_call(
                "spawn_agent_detached",
                serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
            ),
            context.clone(),
        )
        .await;
    assert!(detached.ok, "detached spawn failed: {:?}", detached.error);

    let report_id = loop {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .expect("root inbox");
        if let Some(item) = inbox
            .items
            .iter()
            .find(|item| item.report.summary == "detached report")
        {
            break item.report_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    };

    // Parent's next turn should inject the completion before the user input.
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "parent-after-child".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "parent-message-after-child".into(),
            content: MessageContent::String("continue after child".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();

    let completion_id = piko_protocol::agent_completion_message_id(&report_id);
    let messages = executions.messages();
    let completion = messages
        .iter()
        .find(|commit| commit.message_id == completion_id)
        .expect("completion message must be committed on parent run start");
    assert!(matches!(
        &completion.message,
        piko_protocol::Message::Context {
            source,
            content: MessageContent::String(text),
            ..
        } if source.kind == piko_protocol::AGENT_COMPLETION_SOURCE_KIND
            && source.locator == report_id
            && text.contains("outcome: succeeded")
            && text.contains("summary: detached report")
    ));
    let input = messages
        .iter()
        .find(|commit| commit.message_id == "parent-message-after-child")
        .expect("parent input commit");
    assert_eq!(
        input.parent_message_id.as_deref(),
        Some(completion_id.as_str()),
        "durable chain places completion immediately before the run input"
    );

    // Inbox remains unread until collect.
    let inbox = runtime
        .agent_inbox("session-1".into(), "root".into())
        .await
        .unwrap();
    let item = inbox
        .items
        .iter()
        .find(|item| item.report_id == report_id)
        .expect("report stays in inbox");
    assert!(item.consumed_at.is_none());

    // Second parent run must not inject again (transcript already carries it).
    model.push_text("second parent turn").await;
    let before = executions.messages().len();
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "parent-after-inject".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "parent-message-2".into(),
            content: MessageContent::String("again".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    let completions_after = executions
        .messages()
        .into_iter()
        .skip(before)
        .filter(|commit| commit.message_id.starts_with("agent.completion/"))
        .count();
    assert_eq!(completions_after, 0, "idempotent: no second completion commit");
}

#[tokio::test]
async fn consumed_inbox_skips_completion_injection() {
    let (runtime, _agents, executions, model) = attached_runtime_ports().await;
    let runtime = Arc::new(runtime);
    model.push_text("detached report").await;
    model.push_text("after collect").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = v2_context();

    let detached = provider
        .execute(
            v2_call(
                "spawn_agent_detached",
                serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
            ),
            context.clone(),
        )
        .await;
    assert!(detached.ok);

    let report_id = loop {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .unwrap();
        if let Some(item) = inbox
            .items
            .iter()
            .find(|item| item.report.summary == "detached report")
        {
            break item.report_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    };

    let collected = provider
        .execute(v2_call("collect_agent_reports", serde_json::json!({})), context)
        .await;
    assert!(collected.ok);
    assert_eq!(
        collected.value.as_ref().unwrap()["reports"][0]["report_id"],
        report_id
    );

    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "parent-after-collect".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "parent-after-collect-msg".into(),
            content: MessageContent::String("after collect".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();

    assert!(
        executions
            .messages()
            .iter()
            .all(|commit| !commit.message_id.starts_with("agent.completion/")),
        "consumed inbox must not inject a completion fragment"
    );
}
