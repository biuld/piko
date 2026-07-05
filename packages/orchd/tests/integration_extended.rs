// ---- Tool provider, error path & concurrency tests ----

use std::sync::{Arc, Mutex};

use orchd::Supervisor;
use orchd::protocol::agents::{AgentSpec, HostTaskContext};
use orchd::protocol::config::{OrchdConfig, TaskInput};
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};
use orchd::runtime::dispatch::{DisplayEvent, PersistEvent};
use piko_protocol::ServerMessage as Event;

mod faux_provider;
use faux_provider::{CannedResponse, CannedToolCall, FauxProvider};

use tokio_stream::StreamExt;

/// Helper: drain remaining events from the stream into the vec.
async fn drain_test_events<S>(rx: &mut S, events: &Arc<Mutex<Vec<Event>>>)
where
    S: tokio_stream::Stream<Item = Event> + Unpin,
{
    while let Some(event) = rx.next().await {
        if let Ok(mut guard) = events.lock() {
            guard.push(event);
        }
    }
}

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
        thinking_level: None,
    }
}

// ── Tool provider: TaskControlProvider ──

#[tokio::test]
async fn test_task_control_spawn_and_join() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("sub-task result").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    // Register a sub-agent
    let sub_spec = test_agent_spec("sub-agent");
    core.register_agent(sub_spec).await;

    // Spawn detached task on sub-agent
    let _task_input = TaskInput::new("do sub work").with_agent("sub-agent");
    let task_id = core
        .spawn_detached(
            "sub-agent",
            "do sub work",
            None,
            None,
            HostTaskContext {
                session_id: "s1".into(),
                turn_id: "t1".into(),
            },
            None,
        )
        .await;
    assert!(!task_id.is_empty());

    // Join — the result comes from FauxProvider
    let result = core.poll_task(&task_id, None).await;
    // FauxProvider goes through the agent loop and detached result future.
    assert!(result.is_some(), "join result should be present");
}

#[tokio::test]
async fn test_task_control_spawn_detached_joins_run_stream() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "delegate work",
        vec![CannedToolCall {
            id: "call_spawn_detached".to_string(),
            name: "spawn_detached".to_string(),
            arguments: serde_json::json!({
                "agent_id": "sub-agent",
                "prompt": "do detached sub work"
            }),
        }],
    ))
    .await;
    faux.push_text("root done").await;
    faux.push_text("detached child done").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(AgentSpec {
        tool_set_ids: vec!["builtin".into()],
        ..test_agent_spec("root-agent")
    })
    .await;
    core.register_agent(test_agent_spec("sub-agent")).await;

    let events = Arc::new(Mutex::new(Vec::<Event>::new()));
    let mut rx = core
        .run_streaming(
            "start detached task",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("root-agent".into()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_detached_stream".into(),
                    turn_id: "turn_detached_stream".into(),
                }),
            }),
        )
        .await;

    drain_test_events(&mut rx, &events).await;

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
            session_id,
            agent_id,
            parent_task_id: Some(parent_task_id),
            ..
        }) if session_id == "session_detached_stream"
            && agent_id == "sub-agent"
            && !parent_task_id.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Completed {
            session_id,
            agent_id,
            summary,
            ..
        }) if session_id == "session_detached_stream"
            && agent_id == "sub-agent"
            && summary == "detached child done"
    )));
}

#[tokio::test]
async fn test_await_task_with_host_context_emits_task_joined() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("joined result").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("join-agent")).await;

    let task_id = core
        .spawn_detached(
            "join-agent",
            "do joined work",
            None,
            None,
            HostTaskContext {
                session_id: "session_join".into(),
                turn_id: "turn_join".into(),
            },
            None,
        )
        .await;

    let result = core.poll_task(&task_id, None).await;
    assert!(result.is_some(), "join result should be present");

    let snapshot = core.snapshot().await;
    assert!(matches!(
        snapshot.tasks.get(&task_id).map(|task| &task.status),
        Some(orchd::protocol::agents::AgentTaskStatus::Completed)
    ));
}

// ── Error path: unregistered agent ──

#[tokio::test]
async fn test_run_on_unregistered_agent() {
    let config = test_config();
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = Supervisor::from_config(faux, config).await;

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
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = Supervisor::from_config(faux, config).await;

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
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = Supervisor::from_config(faux, config).await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.is_empty());
    assert!(snapshot.tasks.is_empty());
}

#[tokio::test]
async fn test_run_with_host_context_emits_task_host_events() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("host context response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("hosted")).await;

    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let mut rx = core
        .run_streaming(
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

    drain_test_events(&mut rx, &events).await;

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
            session_id,
            turn_id,
            ..
        }) if session_id == "session_1" && turn_id == "turn_1"
    )));
    assert!(events.iter().any(
        |event| matches!(event, Event::TaskLifecycle(piko_protocol::TaskEvent::Started { session_id, .. }) if session_id == "session_1")
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::TaskLifecycle(piko_protocol::TaskEvent::Completed { session_id, .. }) if session_id == "session_1"))
    );
    assert!(events.iter().any(|event| match event {
        Event::Display(piko_protocol::DisplayEvent::Finalized { content, .. }) =>
            content.iter().any(|b| matches!(
                b,
                piko_protocol::ContentBlock::Text { text } if text == "host context response"
            )),
        _ => false,
    }));
}

#[tokio::test]
async fn test_run_streaming_channels_splits_display_and_persist_events() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("typed channel response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("typed")).await;

    let mut channels = core
        .run_streaming_channels(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("typed".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_typed".to_string(),
                    turn_id: "turn_typed".to_string(),
                }),
            }),
        )
        .await;
    let display = channels.display_stream().unwrap();
    let persist = channels.persist_stream().unwrap();
    let lifecycle = channels.lifecycle_stream().unwrap();
    drop(channels);

    let (display_events, persist_events, lifecycle_events) = tokio::join!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            display.collect::<Vec<_>>()
        ),
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            persist.collect::<Vec<_>>()
        ),
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            lifecycle.collect::<Vec<_>>()
        ),
    );
    let display_events = display_events.unwrap();
    let persist_events = persist_events.unwrap();
    let lifecycle_events = lifecycle_events.unwrap();

    assert!(lifecycle_events.iter().any(|event| matches!(
        event.as_ref(),
        piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Created { session_id, .. })
            if session_id == "session_typed"
    )));
    assert!(display_events.iter().any(|event| matches!(
        event.as_ref(),
        DisplayEvent::TextDelta { delta, .. } if delta == "typed channel response"
    )));
    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::TaskLifecycle(piko_protocol::TaskEvent::Created { session_id, .. })
            if session_id == "session_typed"
    )));
    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::Finalized { session_id, message, .. }
            if session_id == "session_typed"
                && matches!(message, piko_protocol::Message::Assistant { content, .. }
                    if content.iter().any(|block| matches!(
                        block,
                        piko_protocol::ContentBlock::Text { text }
                            if text == "typed channel response"
                    )))
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
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("tool-commit")).await;

    let mut channels = core
        .run_streaming_channels(
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
    let persist = channels.persist_stream().unwrap();
    drop(channels);
    let persist_events = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        persist.collect::<Vec<_>>(),
    )
    .await
    .unwrap();

    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::ToolCallCommitted { session_id, message, .. }
            if session_id == "session_tool"
                && matches!(message, piko_protocol::Message::ToolCall { id, .. }
                    if id == "call_missing")
    )));
    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::ToolResultCommitted { session_id, message, .. }
            if session_id == "session_tool"
                && matches!(message, piko_protocol::Message::ToolResult { tool_call_id, is_error, .. }
                    if tool_call_id == "call_missing" && *is_error == Some(true))
    )));
}

// ── Error path: model error response ──

#[tokio::test]
async fn test_run_with_model_error() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_error("API overloaded").await;

    let config = test_config();
    let core =
        orchd::Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

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
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

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
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

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
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("pubsub")).await;

    let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
    let mut rx = core
        .run_streaming(
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

    drain_test_events(&mut rx, &events).await;

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
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = Supervisor::from_config(faux, config).await;

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

    // orchd keeps only a runtime projection; persisted session facts belong to hostd.
    assert_eq!(snapshot2.tool_sets.get("test-tools"), None);
}
