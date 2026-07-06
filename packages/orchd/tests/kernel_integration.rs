// Runtime integration test for the direct agent execution path.

use std::sync::Arc;

use orchd::Supervisor;
use orchd::protocol::agents::AgentSpec;
use orchd::protocol::config::OrchdConfig;
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};
use piko_protocol::ServerMessage as Event;
use tokio_stream::StreamExt;

mod faux_provider;
use faux_provider::FauxProvider;

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

    let mut events = core
        .run_streaming(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("direct-agent".into()),
                },
                history: None,
                host_context: Some(orchd::protocol::agents::HostTaskContext {
                    session_id: "session-test".into(),
                    turn_id: "turn-test".into(),
                }),
            }),
        )
        .await;

    let mut collected = Vec::new();
    while let Some(event) = events.next().await {
        collected.push(event);
    }

    assert!(collected.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Started {
            agent_id,
            ..
        }) if agent_id == "direct-agent"
    )));
    assert!(collected.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Completed {
            agent_id,
            summary,
            ..
        }) if agent_id == "direct-agent" && summary == "direct runtime response"
    )));
}
