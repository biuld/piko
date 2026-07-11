//! Supervisor snapshot projection tests.

use std::sync::Arc;

use piko_protocol::ServerMessage as Event;
use piko_protocol::agents::HostTaskContext;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::FauxProvider;
use crate::runtime::{
    TEST_STREAM_TIMEOUT, run_test_stream, test_agent_spec, test_config, test_supervisor,
};
use crate::session_output::collect_test_events;

#[tokio::test]
async fn test_snapshot_empty_state() {
    let config = test_config();
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.is_empty());
    assert!(snapshot.tasks.is_empty());
}

#[tokio::test]
async fn test_root_lifecycle_updates_supervisor_snapshot() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("snapshot response").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("snapshot-root")).await;

    let stream = run_test_stream(
        &core,
        "hello",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("snapshot-root".to_string()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_snapshot_root".to_string())),
            ..Default::default()
        }),
    )
    .await;

    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;

    let mut task_id = None;
    for event in &events {
        match event {
            Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
                task_id: created_task_id,
                ..
            }) => task_id = Some(created_task_id.clone()),
            Event::TaskLifecycle(piko_protocol::TaskEvent::Idle { .. }) => break,
            _ => {}
        }
    }

    let task_id = task_id.expect("expected task id");
    let snapshot = core.snapshot().await;
    assert!(matches!(
        snapshot.tasks.get(&task_id).map(|task| &task.status),
        Some(piko_protocol::agents::AgentTaskStatus::Idle)
    ));
}
