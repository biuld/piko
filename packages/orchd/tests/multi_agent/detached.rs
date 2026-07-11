//! Detached task lifecycle and session observation.

use std::sync::Arc;

use piko_protocol::ServerMessage as Event;
use piko_protocol::agents::HostTaskContext;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::{CannedResponse, CannedToolCall, FauxProvider};
use crate::runtime::{
    TEST_STREAM_TIMEOUT, run_test_stream, test_agent_spec, test_config, test_supervisor,
    wait_for_task_report, wait_for_task_status,
};
use crate::session_output::collect_test_events;
use piko_protocol::agents::AgentSpec;

#[tokio::test]
async fn test_detached_task_remains_registered_for_steer() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first report").await;
    faux.push_text("second report").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "first task",
            None,
            None,
            HostTaskContext::new("session_detached_reuse"),
        )
        .await;

    let first = wait_for_task_report(&core, &task_id).await;
    assert_eq!(first.text, "first report");
    assert_eq!(first.task_id.as_deref(), Some(task_id.as_str()));
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Idle,
    )
    .await;

    assert!(core.steer_task(&task_id, "second task").await);
    let second = wait_for_task_report(&core, &task_id).await;
    assert_eq!(second.text, "second report");
    assert_eq!(second.task_id.as_deref(), Some(task_id.as_str()));
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Idle,
    )
    .await;
}

#[tokio::test]
async fn test_task_control_spawn_detached_is_observed_by_session_stream() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "delegate work",
        vec![CannedToolCall {
            id: "call_spawn_detached".to_string(),
            name: "spawn_detached".to_string(),
            arguments: serde_json::json!({
                "agent_id": "worker",
                "prompt": "do detached delegated work"
            }),
        }],
    ))
    .await;
    faux.push_text("root done").await;
    faux.push_text("detached child done").await;

    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(AgentSpec {
        tool_set_ids: vec!["builtin".into()],
        ..test_agent_spec("root-agent")
    })
    .await;
    core.register_agent(test_agent_spec("worker")).await;

    let stream = run_test_stream(
        &core,
        "start detached task",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("root-agent".into()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_detached_stream")),
            ..Default::default()
        }),
    )
    .await;

    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
            session_id,
            agent_id,
            parent_task_id: Some(parent_task_id),
            ..
        }) if session_id == "session_detached_stream"
            && agent_id == "worker"
            && !parent_task_id.is_empty()
    )));
}
