// Runtime integration test for the direct agent execution path.

use std::sync::Arc;

use orchd::AgentRuntimeService;
use piko_protocol::ServerMessage as Event;
use piko_protocol::agents::AgentSpec;
use piko_protocol::config::OrchdConfig;

#[path = "common/faux_provider.rs"]
mod faux_provider;
#[path = "common/runtime.rs"]
mod runtime;
#[path = "common/session_output.rs"]
mod session_output;

use faux_provider::FauxProvider;
use runtime::{TEST_STREAM_TIMEOUT, test_supervisor};
use session_output::{collect_test_events, subscription_event_stream};

#[tokio::test]
async fn direct_agent_run_emits_lifecycle_events() {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();

    let faux = Arc::new(FauxProvider::new());
    faux.push_text("direct runtime response").await;
    let gateway: Arc<dyn llmd::gateway::LlmGateway> = faux;
    let core = test_supervisor(gateway, config).await;

    core.register_agent(AgentSpec {
        id: "direct-agent".into(),
        name: "Direct Agent".into(),
        role: "assistant".into(),
        description: None,
        system_prompt: "You are a test agent.".into(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    })
    .await;

    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    let subscription = runtime
        .start_root_turn(
            "session-test",
            "turn-test",
            "work-test",
            "direct-agent",
            "hello",
            None,
            None,
        )
        .await
        .expect("start_root_turn");

    let stream = subscription_event_stream(subscription);
    let collected = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;

    assert!(collected.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Started {
            agent_id,
            ..
        }) if agent_id == "direct-agent"
    )));
    assert!(collected.iter().any(|event| matches!(
        event,
        Event::RealtimeMessage(piko_protocol::RealtimeMessageEvent {
            agent_id,
            delta: piko_protocol::agent_runtime::RealtimeDelta::Text { delta, .. },
            ..
        }) if agent_id == "direct-agent" && delta == "direct runtime response"
    )));
}
