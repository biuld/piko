// ---- Tool provider, error path & concurrency tests ----

use std::sync::{Arc, Mutex};

use orchd::orchestrator::core::OrchCore;
use orchd::protocol::agents::{AgentSpec, HostTaskContext, TaskSource};
use orchd::protocol::config::{OrchdConfig, TaskInput};
use orchd::protocol::host_event::HostEvent;
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

mod faux_provider;
use faux_provider::{CannedResponse, CannedToolCall, FauxProvider};

fn test_config() -> OrchdConfig {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();
    config
}

fn test_agent_spec(id: &str) -> AgentSpec {
    AgentSpec {
        id: id.to_string(),
        name: id.to_string(),
        role: "test".to_string(),
        description: None,
        system_prompt: "You are a test agent.".to_string(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
    }
}

// ── Tool provider: TaskControlProvider ──

#[tokio::test]
async fn test_task_control_spawn_and_join() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("sub-task result").await;

    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    // Register a sub-agent
    let sub_spec = test_agent_spec("sub-agent");
    core.register_agent(sub_spec).await;

    // Spawn detached task on sub-agent
    let task_input = TaskInput::new("do sub work").with_agent("sub-agent");
    let task_id = core
        .spawn_detached(task_input.convert_to_agent_task(TaskSource::User))
        .await;
    assert!(!task_id.is_empty());

    // Join — the result comes from FauxProvider
    let result = core.await_task(&task_id).await;
    // FauxProvider goes through agent loop and oneshot
    assert!(result.is_some(), "join result should be present");
}

#[tokio::test]
async fn test_await_task_with_host_context_emits_task_joined() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("joined result").await;

    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_agent_spec("join-agent")).await;

    let events: Arc<Mutex<Vec<HostEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let _cleanup = core
        .subscribe_host_events(
            "session_join".to_string(),
            "join-agent".to_string(),
            Box::new(move |event| {
                events_clone.lock().unwrap().push(event);
            }),
        )
        .await;

    let task_id = core
        .spawn_detached(orchd::protocol::agents::AgentTask {
            id: None,
            target_agent_id: "join-agent".to_string(),
            prompt: "do joined work".to_string(),
            source: TaskSource::Agent {
                agent_id: "parent-agent".to_string(),
                task_id: "parent-task".to_string(),
            },
            priority: None,
            parent_task_id: Some("parent-task".to_string()),
            history: None,
            host_context: Some(HostTaskContext {
                session_id: "session_join".to_string(),
                turn_id: "turn_join".to_string(),
            }),
        })
        .await;

    let result = core.await_task(&task_id).await;
    assert!(result.is_some(), "join result should be present");

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::TaskJoined {
            session_id,
            task_id: joined_task_id,
            parent_task_id,
            ..
        } if session_id == "session_join"
            && joined_task_id == &task_id
            && parent_task_id == "parent-task"
    )));
}

// ── Error path: unregistered agent ──

#[tokio::test]
async fn test_run_on_unregistered_agent() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = OrchCore::from_config(faux, config).await;

    // Try to run on agent that doesn't exist — should auto-register via ensure_agent
    let _result = core
        .run(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("ghost".to_string()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;

    // Auto-registration should succeed and run
    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.contains_key("ghost"));
}

// ── Error path: cancel task ──

#[tokio::test]
async fn test_cancel_task() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = OrchCore::from_config(faux, config).await;

    let spec = test_agent_spec("cancellable");
    core.register_agent(spec).await;

    // Cancel a non-existent task — should not panic
    core.cancel_task("nonexistent-task", Some("test cancel"))
        .await;
}

// ── Error path: snapshot on empty state ──

#[tokio::test]
async fn test_snapshot_empty_state() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = OrchCore::from_config(faux, config).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.is_empty());
    assert!(snapshot.tasks.is_empty());
}

#[tokio::test]
async fn test_run_with_host_context_emits_task_host_events() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("host context response").await;
    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_agent_spec("hosted")).await;

    let events: Arc<Mutex<Vec<HostEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let _cleanup = core
        .subscribe_host_events(
            "session_1".to_string(),
            "hosted".to_string(),
            Box::new(move |event| {
                events_clone.lock().unwrap().push(event);
            }),
        )
        .await;

    let result = core
        .run(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("hosted".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_1".to_string(),
                    turn_id: "turn_1".to_string(),
                }),
            }),
        )
        .await;

    assert_eq!(
        result.status,
        orchd::protocol::runtime::RunStatus::Completed
    );
    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::TaskCreated {
            session_id,
            turn_id,
            ..
        } if session_id == "session_1" && turn_id == "turn_1"
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, HostEvent::TaskStarted { session_id, .. } if session_id == "session_1"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, HostEvent::TaskCompleted { session_id, .. } if session_id == "session_1"))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::AssistantMessageCompleted {
            session_id,
            text,
            ..
        } if session_id == "session_1" && text == "host context response"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::TaskTranscriptCommitted {
            session_id,
            final_status,
            ..
        } if session_id == "session_1" && final_status == "completed"
    )));
}

#[tokio::test]
async fn test_run_with_host_context_emits_tool_result_commit_event() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "need a tool",
        vec![CannedToolCall {
            id: "call_missing".to_string(),
            name: "missing_tool".to_string(),
            arguments: serde_json::json!({"path": "nope"}),
        }],
    ))
    .await;
    faux.push_text("done after tool").await;

    let config = test_config();
    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_agent_spec("tool-commit")).await;

    let events: Arc<Mutex<Vec<HostEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let _cleanup = core
        .subscribe_host_events(
            "session_tool".to_string(),
            "tool-commit".to_string(),
            Box::new(move |event| {
                events_clone.lock().unwrap().push(event);
            }),
        )
        .await;

    let result = core
        .run(
            "use tool",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("tool-commit".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_tool".to_string(),
                    turn_id: "turn_tool".to_string(),
                }),
            }),
        )
        .await;

    assert_eq!(
        result.status,
        orchd::protocol::runtime::RunStatus::Completed
    );
    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::AssistantMessageCompleted {
            session_id,
            tool_calls,
            ..
        } if session_id == "session_tool" && tool_calls.iter().any(|call| call.id == "call_missing")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        HostEvent::ToolResultCommitted {
            session_id,
            tool_call_id,
            is_error,
            ..
        } if session_id == "session_tool" && tool_call_id == "call_missing" && *is_error
    )));
}

// ── Error path: model error response ──

#[tokio::test]
async fn test_run_with_model_error() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_error("API overloaded").await;

    let config = test_config();
    let core = orchd::orchestrator::core::OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    let spec = test_agent_spec("error-agent");
    core.register_agent(spec).await;

    // Should not panic — agent loop handles error responses gracefully
    let result = core
        .run(
            "test",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("error-agent".to_string()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;

    // Error response still completes (engine loop may retry or accept)
    // The key assertion is that it doesn't panic
    assert!(result.total_steps <= 5);
}

// ── Concurrency: multiple tasks on same agent ──

#[tokio::test]
async fn test_sequential_tasks_on_same_agent() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_text("second response").await;

    let config = test_config();
    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    let spec = test_agent_spec("worker");
    core.register_agent(spec).await;

    // First task
    let r1 = core
        .run(
            "task1",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("worker".into()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;
    assert_eq!(r1.status, orchd::protocol::runtime::RunStatus::Completed);

    // Second task
    let r2 = core
        .run(
            "task2",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("worker".into()),
                },
                history: None,
                host_context: None,
            }),
        )
        .await;
    assert_eq!(r2.status, orchd::protocol::runtime::RunStatus::Completed);
}

// ── Concurrency: multiple agents ──

#[tokio::test]
async fn test_multiple_agents_concurrent() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("a1 response").await;
    faux.push_text("a2 response").await;

    let config = test_config();
    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_agent_spec("a1")).await;
    core.register_agent(test_agent_spec("a2")).await;

    // Run both concurrently
    let core1 = core.clone();
    let core2 = core.clone();

    let h1 = tokio::spawn(async move {
        core1
            .run(
                "task1",
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some("a1".into()),
                    },
                    history: None,
                    host_context: None,
                }),
            )
            .await
    });

    let h2 = tokio::spawn(async move {
        core2
            .run(
                "task2",
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some("a2".into()),
                    },
                    history: None,
                    host_context: None,
                }),
            )
            .await
    });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    assert_eq!(r1.status, orchd::protocol::runtime::RunStatus::Completed);
    assert_eq!(r2.status, orchd::protocol::runtime::RunStatus::Completed);
}

// ── Subscribe with event capture ──

#[tokio::test]
async fn test_subscribe_captures_multiple_events() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("step 1").await;
    faux.push_text("step 2").await;

    let config = test_config();
    let core = OrchCore::from_config(
        faux as Arc<dyn orchd::model::executor::ModelStepExecutor>,
        config,
    )
    .await;

    core.register_agent(test_agent_spec("pubsub")).await;

    let events = Arc::new(std::sync::Mutex::new(Vec::<HostEvent>::new()));
    let events_clone = events.clone();

    let _cleanup = core
        .subscribe_host_events(
            "session_test".to_string(),
            "pubsub".to_string(),
            Box::new(move |event| {
                events_clone.lock().unwrap().push(event);
            }),
        )
        .await;

    core.run(
        "multi-step",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("pubsub".into()),
            },
            history: None,
            host_context: None,
        }),
    )
    .await;

    let received = events.lock().unwrap();
    // Should receive at least some events
    assert!(
        !received.is_empty(),
        "should receive at least one event, got none"
    );
}

// ── Tool set registration ──

#[tokio::test]
async fn test_register_and_unregister_tool_set() {
    let config = test_config();
    let faux: Arc<dyn orchd::model::executor::ModelStepExecutor> = Arc::new(FauxProvider::new());
    let core = OrchCore::from_config(faux, config).await;

    let tool_set = orchd::protocol::tools::ToolSet {
        id: "test-tools".into(),
        name: "Test Tools".into(),
        description: None,
        tools: vec![],
        policy: None,
        metadata: None,
    };

    core.register_tool_set(tool_set).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.tool_sets.contains_key("test-tools"));

    core.unregister_tool_set("test-tools").await;

    let snapshot2 = core.snapshot().await;
    assert!(!snapshot2.tool_sets.contains_key("test-tools"));

    // Verify sourcing events from OrchCore
    let events = core.sourcing_events().await;
    let has_registered = events.iter().any(|e| {
        matches!(
            e,
            orchd::protocol::event_store::OrchSourcingEvent::ToolSetRegistered { .. }
        )
    });
    let has_unregistered = events.iter().any(|e| {
        matches!(
            e,
            orchd::protocol::event_store::OrchSourcingEvent::ToolSetUnregistered { .. }
        )
    });
    assert!(has_registered);
    assert!(has_unregistered);
}
