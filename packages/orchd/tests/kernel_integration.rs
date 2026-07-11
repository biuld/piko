// Runtime integration test for the direct agent execution path.

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::host::Supervisor;
use piko_protocol::ServerMessage as Event;
use piko_protocol::agents::AgentSpec;
use piko_protocol::config::OrchdConfig;

mod faux_provider;
mod session_output_support;
use faux_provider::FauxProvider;
use session_output_support::{collect_test_events, subscription_event_stream};

const TEST_STREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[tokio::test]
async fn direct_agent_run_emits_lifecycle_events() {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();

    let faux = Arc::new(FauxProvider::new());
    faux.push_text("direct runtime response").await;
    let gateway: Arc<dyn llmd::gateway::LlmGateway> = faux;
    let core = Supervisor::from_config(gateway, config).await;

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

    let runtime = AgentRuntimeService::runtime_for(&core);
    let subscription = runtime
        .start_root_turn(
            "session-test",
            "turn-test",
            "direct-agent",
            "hello",
            piko_protocol::agents::HostTaskContext {
                session_id: "session-test".into(),
                turn_id: "turn-test".into(),
            },
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
        Event::Display(piko_protocol::DisplayEvent::Finalized { agent_id, content, .. })
            if agent_id == "direct-agent"
                && content.iter().any(|block| matches!(
                    block,
                    piko_protocol::ContentBlock::Text { text }
                        if text == "direct runtime response"
                ))
    )));
}
