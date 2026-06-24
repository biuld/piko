// ---- Event sourcing tests ----
//
// Verify apply_event, rebuild_state, and sourcing event emission
// from OrchCore.

use std::sync::Arc;

use orchd::protocol::agents::{AgentSpec, AgentTaskResult, TaskSource};
use orchd::protocol::config::OrchdConfig;
use orchd::protocol::event_store::{OrchSourcingEvent, apply_event, rebuild_state};
use orchd::protocol::state::OrchState;

mod faux_provider;
use faux_provider::FauxProvider;

fn test_spec(id: &str) -> AgentSpec {
    AgentSpec {
        id: id.into(),
        name: id.into(),
        role: "test".into(),
        description: None,
        system_prompt: String::new(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
    }
}

fn test_config() -> OrchdConfig {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();
    config
}

// ── apply_event tests ──

#[test]
fn test_apply_agent_registered() {
    let mut state = OrchState::new("t".into());
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentRegistered {
            agent_id: "main".into(),
            spec: test_spec("main"),
            timestamp: 0,
        },
    );
    assert_eq!(state.agents.len(), 1);
    assert!(state.agents.contains_key("main"));
}

#[test]
fn test_apply_agent_unregistered() {
    let mut state = OrchState::new("t".into());
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentRegistered {
            agent_id: "main".into(),
            spec: test_spec("main"),
            timestamp: 0,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentUnregistered {
            agent_id: "main".into(),
            timestamp: 1,
        },
    );
    assert!(state.agents.is_empty());
}

#[test]
fn test_apply_task_lifecycle() {
    let mut state = OrchState::new("t".into());
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentRegistered {
            agent_id: "w".into(),
            spec: test_spec("w"),
            timestamp: 0,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskCreated {
            task_id: "t1".into(),
            target_agent_id: "w".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 1,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskStarted {
            task_id: "t1".into(),
            agent_id: "w".into(),
            timestamp: 2,
        },
    );
    assert_eq!(
        state.tasks["t1"].status,
        orchd::protocol::agents::AgentTaskStatus::Running
    );

    state = apply_event(
        state,
        &OrchSourcingEvent::TaskCompleted {
            task_id: "t1".into(),
            agent_id: "w".into(),
            result: AgentTaskResult {
                summary: "ok".into(),
                artifacts: None,
            },
            timestamp: 3,
        },
    );
    assert_eq!(
        state.tasks["t1"].status,
        orchd::protocol::agents::AgentTaskStatus::Completed
    );
}

#[test]
fn test_apply_task_failed() {
    let mut state = OrchState::new("t".into());
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentRegistered {
            agent_id: "w".into(),
            spec: test_spec("w"),
            timestamp: 0,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskCreated {
            task_id: "t1".into(),
            target_agent_id: "w".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 1,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskFailed {
            task_id: "t1".into(),
            agent_id: "w".into(),
            error: "boom".into(),
            timestamp: 2,
        },
    );
    assert_eq!(
        state.tasks["t1"].status,
        orchd::protocol::agents::AgentTaskStatus::Failed
    );
}

#[test]
fn test_apply_task_cancelled() {
    let mut state = OrchState::new("t".into());
    state = apply_event(
        state,
        &OrchSourcingEvent::AgentRegistered {
            agent_id: "w".into(),
            spec: test_spec("w"),
            timestamp: 0,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskCreated {
            task_id: "t1".into(),
            target_agent_id: "w".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 1,
        },
    );
    state = apply_event(
        state,
        &OrchSourcingEvent::TaskCancelled {
            task_id: "t1".into(),
            agent_id: "w".into(),
            reason: Some("cancelled".into()),
            timestamp: 2,
        },
    );
    assert_eq!(
        state.tasks["t1"].status,
        orchd::protocol::agents::AgentTaskStatus::Cancelled
    );
}

#[test]
fn test_event_kinds() {
    assert_eq!(
        OrchSourcingEvent::TaskCreated {
            task_id: "t".into(),
            target_agent_id: "a".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 0,
        }
        .kind(),
        "task_created"
    );
    assert_eq!(
        OrchSourcingEvent::TaskCompleted {
            task_id: "t".into(),
            agent_id: "a".into(),
            result: AgentTaskResult {
                summary: "ok".into(),
                artifacts: None
            },
            timestamp: 0,
        }
        .kind(),
        "task_completed"
    );
}

// ── rebuild_state tests ──

#[test]
fn test_rebuild_state_from_slice() {
    let events = vec![
        OrchSourcingEvent::AgentRegistered {
            agent_id: "a".into(),
            spec: test_spec("a"),
            timestamp: 0,
        },
        OrchSourcingEvent::TaskCreated {
            task_id: "t1".into(),
            target_agent_id: "a".into(),
            prompt: "p".into(),
            source: TaskSource::User,
            parent_task_id: None,
            timestamp: 1,
        },
    ];
    let state = rebuild_state(&events);
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.tasks.len(), 1);
}

#[test]
fn test_rebuild_state_empty() {
    let state = rebuild_state(&[]);
    assert!(state.agents.is_empty());
    assert!(state.tasks.is_empty());
}

// ── OrchCore sourcing event emission ──

#[tokio::test]
async fn test_core_emits_sourcing_events() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = orchd::orchestrator::core::OrchCore::from_config(faux, config).await;

    // Register an agent
    core.register_agent(test_spec("test-agent")).await;

    let events = core.sourcing_events().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind(), "agent_registered");
}

#[tokio::test]
async fn test_core_emits_task_events() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("response").await;

    let config = test_config();
    let core = orchd::orchestrator::core::OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_spec("worker")).await;

    let task = orchd::protocol::agents::AgentTask {
        id: None,
        target_agent_id: "worker".into(),
        prompt: "test".into(),
        source: TaskSource::User,
        priority: None,
        parent_task_id: None,
        history: None,
    };

    let _task_id = core.spawn(task).await;

    let events = core.sourcing_events().await;
    // Should have at least: AgentRegistered, TaskCreated, TaskStarted
    assert!(
        events.len() >= 3,
        "expected >= 3 events, got {}",
        events.len()
    );

    let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
    assert!(kinds.contains(&"agent_registered"));
    assert!(kinds.contains(&"task_created"));
    assert!(kinds.contains(&"task_started"));
}

#[tokio::test]
async fn test_core_emits_tool_set_events() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = orchd::orchestrator::core::OrchCore::from_config(faux, config).await;

    let tool_set = orchd::protocol::tools::ToolSet {
        id: "test-tools".into(),
        name: "Test".into(),
        description: None,
        tools: vec![],
        policy: None,
        metadata: None,
    };

    core.register_tool_set(tool_set).await;
    core.unregister_tool_set("test-tools").await;

    let events = core.sourcing_events().await;
    let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
    assert!(kinds.contains(&"tool_set_registered"));
    assert!(kinds.contains(&"tool_set_unregistered"));
}
