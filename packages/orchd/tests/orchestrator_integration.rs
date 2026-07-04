// ---- Phase 8: Orchestrator integration tests ----
//
// End-to-end tests using FauxProvider to mock the LLM layer.

use std::sync::Arc;

use orchd::protocol::agents::{AgentSpec, AgentTask, HostTaskContext, TaskSource};
use orchd::protocol::config::OrchdConfig;

use orchd::protocol::runtime::{OrchRunOptions, RunStatus};
use piko_protocol::ServerMessage as Event;
mod faux_provider;
use faux_provider::FauxProvider;

use tokio_stream::StreamExt;

/// Helper: drain remaining events from the stream into the vec.
async fn drain_test_events<S>(rx: &mut S, events: &Arc<std::sync::Mutex<Vec<Event>>>)
where
    S: tokio_stream::Stream<Item = Event> + Unpin,
{
    while let Some(event) = rx.next().await {
        if let Ok(mut guard) = events.lock() {
            guard.push(event);
        }
    }
}

/// Helper: create a minimal OrchdConfig for testing (no pre-registered agents).
fn test_config(provider_name: &str) -> OrchdConfig {
    let mut config = OrchdConfig::single_provider(provider_name, "test-key", "faux-1");
    config.agents.clear(); // Don't auto-register agents; tests manage their own
    config
}

/// Helper: create a minimal agent spec for testing.
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
    let core = orchd::Supervisor::from_config(faux, config).await;

    // Verify basic state
    assert!(core.snapshot().await.run_id.starts_with("run_"));
}

#[tokio::test]
async fn test_register_agent() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = orchd::Supervisor::from_config(faux, config).await;

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
    let core = orchd::Supervisor::from_config(faux, config).await;

    let spec = test_agent_spec("temp-agent", "Temp");
    core.register_agent(spec).await;

    {
        let snapshot = core.snapshot().await;
        assert!(snapshot.agents.contains_key("temp-agent"));
    }

    core.unregister_agent("temp-agent").await;

    let snapshot = core.snapshot().await;
    assert!(!snapshot.agents.contains_key("temp-agent"));
}

#[tokio::test]
async fn test_spawn_task() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("Hello, I completed the task.").await;

    let config = test_config("faux");
    let core =
        orchd::Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let spec = test_agent_spec("worker", "Worker");
    core.register_agent(spec).await;

    let task = AgentTask {
        id: None,
        target_agent_id: "worker".to_string(),
        prompt: "test prompt".to_string(),
        source: TaskSource::User,
        priority: None,
        parent_task_id: None,
        history: None,
        host_context: None,
    };

    let hc = HostTaskContext {
        session_id: "s1".into(),
        turn_id: "t1".into(),
    };
    let _res = core
        .spawn(&task.target_agent_id, &task.prompt, None, hc)
        .await;
    assert!(_res.is_some());
}

#[tokio::test]
async fn test_run_with_canned_response() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("The answer is 42.").await;

    let config = test_config("faux");
    let core =
        orchd::Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let spec = test_agent_spec("main-runner", "Main");
    core.register_agent(spec).await;

    let result = core
        .run(
            "What is the answer?",
            Some(OrchRunOptions {
                command: orchd::protocol::runtime::OrchRunCommandOptions {
                    target_agent_id: Some("main-runner".to_string()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;

    assert_eq!(result.status, RunStatus::Completed);
    assert!(result.total_steps >= 1);

    // Should have the assistant message in the output
    let has_answer = result
        .messages
        .iter()
        .any(|m| {
            if let orchd::protocol::messages::Message::Assistant { content, .. } = m {
                content.iter().any(|b| {
                    matches!(b, orchd::protocol::messages::ContentBlock::Text { text } if text.contains("42"))
                })
            } else {
                false
            }
        });
    assert!(has_answer, "Should contain the canned response");
}

#[tokio::test]
async fn test_subscribe_events() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("OK done.").await;

    let config = test_config("faux");
    let core =
        orchd::Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let spec = test_agent_spec("subscriber", "Subscriber");
    core.register_agent(spec).await;

    let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
    let mut rx = core
        .run_streaming(
            "hello",
            Some(OrchRunOptions {
                command: orchd::protocol::runtime::OrchRunCommandOptions {
                    target_agent_id: Some("subscriber".to_string()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;

    drain_test_events(&mut rx, &events).await;

    let received = events.lock().unwrap();
    assert!(!received.is_empty(), "Should receive at least one event");
}

#[tokio::test]
async fn test_snapshot_state() {
    let config = test_config("faux");
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = orchd::Supervisor::from_config(faux, config).await;

    let snapshot = core.snapshot().await;
    // No agents were auto-registered (agents cleared in test_config)
    assert_eq!(snapshot.agents.len(), 0);
}
