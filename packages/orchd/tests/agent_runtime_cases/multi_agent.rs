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
async fn v2_followup_task_starts_a_turn_when_idle() {
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
                "followup_task",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "do second work",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "followup failed: {:?}", followup.error);
    assert_eq!(followup.value.as_ref().unwrap()["disposition"], "accepted");

    for _ in 0..200 {
        if model.call_count().await == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert_eq!(model.call_count().await, 2);
}

#[tokio::test]
async fn v2_followup_task_queues_while_busy_and_commits_input() {
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
                "followup_task",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "queued work",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "followup failed: {:?}", followup.error);
    assert_eq!(followup.value.as_ref().unwrap()["disposition"], "queued");

    let commands = commits.commands.lock().await;
    assert!(commands.iter().any(|command| matches!(
        command,
        AgentDurableCommand::InputQueued { queued_input, .. }
            if queued_input.request.agent_instance_id == child
                && queued_input.request.request_id.starts_with("followup:")
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
                "followup_task",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "continue",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "follow-up after interrupt failed: {:?}", followup.error);
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
                "followup_task",
                serde_json::json!({
                    "agent_instance_id": child,
                    "message": "block again",
                }),
            ),
            v2_context(),
        )
        .await;
    assert!(followup.ok, "second follow-up failed: {:?}", followup.error);
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
        "spawn_agent",
        "spawn_agent_detached",
        "send_agent_message",
        "collect_agent_reports",
        "close_agent",
        "reopen_agent",
        "followup_task",
        "interrupt_agent",
        "list_agents",
        "wait_agent",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    let names: std::collections::BTreeSet<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, expected, "multi-agent surface drifted from the consolidated set");

    let send = tools
        .iter()
        .find(|tool| tool.name == "send_agent_message")
        .unwrap();
    assert!(
        send.input_schema
            .get("properties")
            .and_then(|properties| properties.get("delivery"))
            .is_none(),
        "send_agent_message must not expose a delivery mode; follow-up lives on followup_task"
    );
}
