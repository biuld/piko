//! poll_task behavior for detached child tasks.

use std::sync::Arc;

use orchd::testing::{ToolCall, ToolDiscoveryContext, ToolRegistry};
use piko_protocol::agents::HostTaskContext;

use crate::faux_provider::FauxProvider;
use crate::runtime::{
    test_agent_spec, test_config, test_supervisor, wait_for_task_report,
};

#[tokio::test]
async fn test_poll_task_with_host_context_keeps_runtime_idle() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("joined result").await;

    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("join-agent")).await;

    let task_id = core
        .spawn_detached(
            "join-agent",
            "do joined work",
            None,
            None,
            HostTaskContext::new("session_join"),
        )
        .await;

    let result = wait_for_task_report(&core, &task_id).await;
    assert_eq!(result.task_id.as_deref(), Some(task_id.as_str()));

    let snapshot = core.snapshot().await;
    assert!(matches!(
        snapshot.tasks.get(&task_id).map(|task| &task.status),
        Some(piko_protocol::agents::AgentTaskStatus::Idle)
    ));
}

#[tokio::test]
async fn test_poll_task_via_tool_provider_accepts_task_ids() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("child hello").await;

    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "say hello",
            None,
            None,
            HostTaskContext::new("session_poll_provider"),
        )
        .await;

    wait_for_task_report(&core, &task_id).await;

    let discovery_ctx = ToolDiscoveryContext {
        agent_id: "main".into(),
        task_id: Some("task_main".into()),
        tool_set_ids: vec!["builtin".into()],
        active_tool_names: None,
    };
    let (_, routes) = core.tool_registry().discover_tools(&discovery_ctx).await;
    let route = routes
        .get("poll_task")
        .expect("poll_task should be discoverable");

    let exec_ctx = orchd::testing::ToolExecutionContext {
        agent_id: "main".into(),
        task_id: "task_main".into(),
        tool_set_ids: vec!["builtin".into()],
        turn_index: Some(0),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: Some("msg_poll".into()),
        content_index: Some(0),
        tool_call_index: Some(0),
        tool_entity_id: None,
        host_context: Some(HostTaskContext::new("session_poll_provider")),
        active_work_id: None,
        source_turn_id: None,
    };
    let call = ToolCall {
        id: "call_poll_task".into(),
        name: "poll_task".into(),
        arguments: serde_json::json!({
            "task_ids": [task_id]
        }),
        partial_json: None,
    };

    let record = core
        .tool_registry()
        .execute_tool(&call, &exec_ctx, route, None)
        .await;
    assert!(
        record.result.ok,
        "poll_task tool provider call should succeed"
    );
    let value = record
        .result
        .value
        .expect("poll_task should return a value");
    let results = value
        .get("results")
        .and_then(|v| v.as_array())
        .expect("poll_task should return results array");
    assert_eq!(results.len(), 1);
    assert!(
        results[0].get("result").is_some(),
        "poll_task should return cached child report"
    );
}

#[tokio::test]
async fn test_poll_task_returns_immediately_when_not_ready() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("slow child").await;

    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "slow work",
            None,
            None,
            HostTaskContext::new("session_poll_immediate"),
        )
        .await;

    let started = std::time::Instant::now();
    let immediate = core.poll_task(&task_id).await;
    assert!(
        started.elapsed() < std::time::Duration::from_millis(200),
        "poll should not block"
    );

    let report = match immediate {
        Some(report) => report,
        None => wait_for_task_report(&core, &task_id).await,
    };
    assert_eq!(report.text, "slow child");
}
