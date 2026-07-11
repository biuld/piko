//! Session output observation and event streaming.

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
async fn test_run_with_host_context_emits_task_host_events() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("host context response").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("hosted")).await;

    let stream = run_test_stream(
        &core,
        "hello",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("hosted".to_string()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_1".to_string())),
            ..Default::default()
        }),
    )
    .await;

    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
            session_id,
            ..
        }) if session_id == "session_1"
    )));
    assert!(events.iter().any(
        |event| matches!(event, Event::TaskLifecycle(piko_protocol::TaskEvent::Started { session_id, .. }) if session_id == "session_1")
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::TaskLifecycle(piko_protocol::TaskEvent::Idle { session_id, .. }) if session_id == "session_1"))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            delta: piko_protocol::agent_runtime::RealtimeDelta::Text { delta, .. },
            ..
        }) if delta == "host context response"
    )));
}

#[tokio::test]
async fn test_start_root_turn_splits_realtime_and_persist_events() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("typed channel response").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("typed")).await;

    let stream = run_test_stream(
        &core,
        "hello",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("typed".to_string()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_typed".to_string())),
            ..Default::default()
        }),
    )
    .await;

    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;

    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created { session_id, .. })
            if session_id == "session_typed"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            delta: piko_protocol::agent_runtime::RealtimeDelta::Text { delta, .. },
            ..
        }) if delta == "typed channel response"
    )));
}

#[tokio::test]
async fn test_subscribe_captures_multiple_events() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("step 1").await;
    faux.push_text("step 2").await;

    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("pubsub")).await;

    let stream = run_test_stream(
        &core,
        "multi-step",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("pubsub".into()),
            },
            history: None,
            host_context: None,
            ..Default::default()
        }),
    )
    .await;

    let received = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;
    assert!(
        !received.is_empty(),
        "should receive at least one event, got none"
    );
}
