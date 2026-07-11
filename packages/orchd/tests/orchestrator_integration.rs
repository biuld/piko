// ---- Phase 8: Orchestrator integration tests ----
//
// End-to-end tests using FauxProvider to mock the LLM layer.

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::host::Supervisor;
use piko_protocol::agents::{AgentSpec, HostTaskContext};
use piko_protocol::config::OrchdConfig;
use piko_protocol::runtime::{OrchRunOptions, RunStatus};

#[path = "common/faux_provider.rs"]
mod faux_provider;
#[path = "common/runtime.rs"]
mod runtime;
#[path = "common/session_output.rs"]
mod session_output;

use faux_provider::FauxProvider;
use runtime::{TEST_STREAM_TIMEOUT, test_supervisor};
use session_output::{collect_test_events, subscription_event_stream};

fn test_config(provider_name: &str) -> OrchdConfig {
    let mut config = OrchdConfig::single_provider(provider_name, "test-key", "faux-1");
    config.agents.clear();
    config
}

fn test_agent_spec(id: &str, name: &str) -> AgentSpec {
    AgentSpec {
        id: id.to_string(),
        name: name.to_string(),
        role: "test".to_string(),
        description: None,
        system_prompt: "You are a test agent.".to_string(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    }
}

#[tokio::test]
async fn test_orchestrator_core_creation() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    assert!(core.snapshot().await.run_id.starts_with("run_"));
}

#[tokio::test]
async fn test_register_agent() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let spec = test_agent_spec("test-agent", "TestAgent");
    core.register_agent(spec).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.contains_key("test-agent"));
    assert_eq!(snapshot.agents["test-agent"].spec.id, "test-agent");
}

#[tokio::test]
async fn test_unregister_agent() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let spec = test_agent_spec("temp-agent", "TempAgent");
    core.register_agent(spec).await;
    assert!(core.snapshot().await.agents.contains_key("temp-agent"));

    core.unregister_agent("temp-agent").await;
    assert!(!core.snapshot().await.agents.contains_key("temp-agent"));
}

#[tokio::test]
async fn test_spawn_task() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let spec = test_agent_spec("test-agent", "TestAgent");
    core.register_agent(spec).await;

    let task_id = core
        .spawn_detached(
            "test-agent",
            "test prompt",
            None,
            None,
            HostTaskContext::new("session-1"),
        )
        .await;
    assert!(!task_id.is_empty());
    assert!(core.snapshot().await.tasks.contains_key(&task_id));
}

#[tokio::test]
async fn test_run_with_canned_response() {
    let config = test_config("faux");
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("Hello from faux!").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("main", "Main")).await;

    let result = core
        .run(
            "Say hello",
            Some(OrchRunOptions {
                command: Default::default(),
                history: None,
                host_context: None,
                ..Default::default()
            }),
        )
        .await;

    assert_eq!(result.status, RunStatus::Completed);
    assert!(result.messages.iter().any(|m| matches!(
        m,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|b| matches!(
                b,
                piko_protocol::ContentBlock::Text { text } if text.contains("Hello from faux")
            ))
    )));
}

#[tokio::test]
async fn test_subscribe_events() {
    let config = test_config("faux");
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("event test").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("main", "Main")).await;

    let runtime = AgentRuntimeService::runtime_for(&core);
    let subscription = runtime
        .start_root_turn(
            "session-sub",
            "turn-sub",
            "work-sub",
            "main",
            "hello",
            None,
            None,
        )
        .await
        .expect("start_root_turn");

    let stream = subscription_event_stream(subscription);
    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;
    assert!(!events.is_empty());
}

#[tokio::test]
async fn test_snapshot_state() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let spec = test_agent_spec("snap-agent", "SnapAgent");
    core.register_agent(spec).await;

    let snapshot = core.snapshot().await;
    assert_eq!(snapshot.agents.len(), 1);
    assert!(snapshot.agents.contains_key("snap-agent"));
}
