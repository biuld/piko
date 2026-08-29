use super::*;

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
            agent_kind: piko_protocol::AgentKind::Supervisor,
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
