use super::*;

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
        AgentDurableCommand::AgentInputAdmitted { admission }
            if admission.input.agent_instance_id == child
                && admission.input.request_id.starts_with("message:")
                && admission.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp
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
async fn v2_wait_agent_returns_on_work_finished() {
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
    assert_eq!(value["event"]["kind"], "workFinished");
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
