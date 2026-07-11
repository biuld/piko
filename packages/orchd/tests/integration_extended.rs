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

async fn run_test_stream(
    supervisor: &Supervisor,
    prompt: &str,
    opts: Option<OrchRunOptions>,
) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Event> + Send>> {
    let mut channels = supervisor.run_streaming_channels(prompt, opts).await;
    let mut display = channels.display_stream().unwrap();
    let mut persist = channels.persist_stream().unwrap();
    let mut lifecycle = channels.lifecycle_stream().unwrap();

    Box::pin(async_stream::stream! {
        use orchd::runtime::dispatch::{
            server_message_from_display_event, server_message_from_persist_event,
        };

        let mut display_done = false;
        let mut persist_done = false;
        let mut lifecycle_done = false;

        while !(display_done && persist_done && lifecycle_done) {
            tokio::select! {
                biased;
                display_event = display.next(), if !display_done => {
                    match display_event {
                        Some(event) => {
                            if let Some(msg) = server_message_from_display_event(event.as_ref()) {
                                yield msg;
                            }
                        }
                        None => display_done = true,
                    }
                }
                persist_event = persist.next(), if !persist_done => {
                    match persist_event {
                        Some(event) => {
                            if let Some(msg) = server_message_from_persist_event(event.as_ref()) {
                                yield msg;
                            }
                        }
                        None => persist_done = true,
                    }
                }
                lifecycle_event = lifecycle.next(), if !lifecycle_done => {
                    match lifecycle_event {
                        Some(event) => {
                            if let orchd::runtime::dispatch::LifecycleEvent::Task(task_event) = event.as_ref() {
                                yield Event::TaskLifecycle(task_event.clone());
                            }
                        }
                        None => lifecycle_done = true,
                    }
                }
            }
        }
    })
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

async fn wait_for_task_status(
    supervisor: &Supervisor,
    task_id: &str,
    expected: orchd::protocol::agents::AgentTaskStatus,
) {
    let reached = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let snapshot = supervisor.snapshot().await;
            if snapshot
                .tasks
                .get(task_id)
                .is_some_and(|task| task.status == expected)
            {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if reached.is_err() {
        let actual = supervisor
            .snapshot()
            .await
            .tasks
            .get(task_id)
            .map(|task| task.status.clone());
        panic!("task {task_id} did not reach {expected:?}; actual status: {actual:?}");
    }
}

async fn wait_for_task_report(
    supervisor: &Supervisor,
    task_id: &str,
) -> orchd::ports::agent_spawner::AgentReport {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(report) = supervisor.poll_task(task_id).await {
                return report;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("task {task_id} did not produce a report"))
}

// ── Tool provider: TaskControlProvider ──

#[tokio::test]
async fn test_task_control_spawn_and_join() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("sub-task result").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    // Register a worker
    let sub_spec = test_agent_spec("worker");
    core.register_agent(sub_spec).await;

    // Spawn detached task on worker
    let _task_input = TaskInput::new("do delegated work").with_agent("worker");
    let task_id = core
        .spawn_detached(
            "worker",
            "do delegated work",
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
    let result = wait_for_task_report(&core, &task_id).await;
    assert_eq!(result.text, "sub-task result");
}

#[tokio::test]
async fn test_detached_task_remains_registered_for_steer() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first report").await;
    faux.push_text("second report").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "first task",
            None,
            None,
            HostTaskContext {
                session_id: "session_detached_reuse".into(),
                turn_id: "turn_1".into(),
            },
            None,
        )
        .await;

    let first = wait_for_task_report(&core, &task_id).await;
    assert_eq!(first.text, "first report");
    assert_eq!(first.task_id.as_deref(), Some(task_id.as_str()));
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Idle,
    )
    .await;

    assert!(core.steer_task(&task_id, "second task").await);
    let second = wait_for_task_report(&core, &task_id).await;
    assert_eq!(second.text, "second report");
    assert_eq!(second.task_id.as_deref(), Some(task_id.as_str()));
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Idle,
    )
    .await;
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
                "agent_id": "worker",
                "prompt": "do detached delegated work"
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
    core.register_agent(test_agent_spec("worker")).await;

    let events = Arc::new(Mutex::new(Vec::<Event>::new()));
    let mut rx = run_test_stream(
        &core,
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
            && agent_id == "worker"
            && !parent_task_id.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(
            piko_protocol::TaskEvent::Idle {
                session_id,
                agent_id,
                summary,
                ..
            } | piko_protocol::TaskEvent::Completed {
                session_id,
                agent_id,
                summary,
                ..
            }
        ) if session_id == "session_detached_stream"
            && agent_id == "worker"
            && summary == "detached child done"
    )));
}

#[tokio::test]
async fn test_spawn_detached_child_finalized_reaches_persist_stream() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "delegate work",
        vec![CannedToolCall {
            id: "call_spawn_detached".to_string(),
            name: "spawn_detached".to_string(),
            arguments: serde_json::json!({
                "agent_id": "worker",
                "prompt": "do detached delegated work"
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
    core.register_agent(test_agent_spec("worker")).await;

    let mut channels = core
        .run_streaming_channels(
            "start detached task",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("root-agent".into()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_detached_persist".into(),
                    turn_id: "turn_detached_persist".into(),
                }),
            }),
        )
        .await;
    let mut display = channels.display_stream().unwrap();
    let mut persist = channels.persist_stream().unwrap();
    let mut lifecycle = channels.lifecycle_stream().unwrap();
    drop(channels);

    tokio::spawn(async move { while display.next().await.is_some() {} });
    tokio::spawn(async move { while lifecycle.next().await.is_some() {} });

    let mut persist_events = Vec::new();
    while let Some(event) = persist.next().await {
        persist_events.push(event);
    }

    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::Finalized {
            session_id,
            agent_id,
            message,
            ..
        } if session_id == "session_detached_persist"
            && agent_id == "worker"
            && matches!(
                message,
                piko_protocol::Message::Assistant { content, .. }
                    if content.iter().any(|block| matches!(
                        block,
                        piko_protocol::ContentBlock::Text { text }
                            if text == "detached child done"
                    ))
            )
    )));
}

#[tokio::test]
async fn test_poll_task_with_host_context_keeps_runtime_idle() {
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

    let result = wait_for_task_report(&core, &task_id).await;
    assert_eq!(result.task_id.as_deref(), Some(task_id.as_str()));

    let snapshot = core.snapshot().await;
    assert!(matches!(
        snapshot.tasks.get(&task_id).map(|task| &task.status),
        Some(orchd::protocol::agents::AgentTaskStatus::Idle)
    ));
}

#[tokio::test]
async fn test_poll_task_via_tool_provider_accepts_task_ids() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("child hello").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "say hello",
            None,
            None,
            HostTaskContext {
                session_id: "session_poll_provider".into(),
                turn_id: "turn_poll_provider".into(),
            },
            None,
        )
        .await;

    wait_for_task_report(&core, &task_id).await;

    use orchd::adapters::tools::registry::ToolRegistry;
    use orchd::domain::tools::call::ToolCall;
    use orchd::ports::tool_provider::ToolDiscoveryContext;

    let discovery_ctx = ToolDiscoveryContext {
        agent_id: "main".into(),
        task_id: Some("task_main".into()),
        tool_set_ids: vec!["builtin".into()],
        active_tool_names: None,
    };
    let (_, routes) = core.tool_registry().discover_tools(&discovery_ctx).await;
    let route = routes
        .get("poll_task")
        .expect("poll_task should be discoverable");

    let exec_ctx = orchd::ports::tool_provider::ToolExecutionContext {
        agent_id: "main".into(),
        task_id: "task_main".into(),
        tool_set_ids: vec!["builtin".into()],
        turn_index: Some(0),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: Some("msg_poll".into()),
        content_index: Some(0),
        tool_call_index: Some(0),
        tool_entity_id: None,
        host_context: Some(HostTaskContext {
            session_id: "session_poll_provider".into(),
            turn_id: "turn_poll_provider".into(),
        }),
        senders: None,
    };
    let call = ToolCall {
        id: "call_poll_task".into(),
        name: "poll_task".into(),
        arguments: serde_json::json!({
            "task_ids": [task_id]
        }),
        partial_json: None,
    };

    let record = core
        .tool_registry()
        .execute_tool(&call, &exec_ctx, route, None)
        .await;
    assert!(
        record.result.ok,
        "poll_task tool provider call should succeed"
    );
    let value = record
        .result
        .value
        .expect("poll_task should return a value");
    let results = value
        .get("results")
        .and_then(|v| v.as_array())
        .expect("poll_task should return results array");
    assert_eq!(results.len(), 1);
    assert!(
        results[0].get("result").is_some(),
        "poll_task should return cached child report"
    );
}

#[tokio::test]
async fn test_poll_task_returns_immediately_when_not_ready() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("slow child").await;

    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("worker")).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "slow work",
            None,
            None,
            HostTaskContext {
                session_id: "session_poll_immediate".into(),
                turn_id: "turn_poll_immediate".into(),
            },
            None,
        )
        .await;

    let started = std::time::Instant::now();
    let immediate = core.poll_task(&task_id).await;
    assert!(
        immediate.is_none(),
        "poll should return immediately when result is not ready"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_millis(200),
        "poll should not block"
    );

    let report = wait_for_task_report(&core, &task_id).await;
    assert_eq!(report.text, "slow child");
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

#[tokio::test]
async fn test_cancelled_task_runtime_is_unregistered() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("ready").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("cancellable")).await;

    let task_id = core
        .spawn_detached(
            "cancellable",
            "wait",
            None,
            None,
            HostTaskContext {
                session_id: "session_cancel".into(),
                turn_id: "turn_cancel".into(),
            },
            None,
        )
        .await;
    wait_for_task_report(&core, &task_id).await;

    core.cancel_task(&task_id, Some("stop")).await;
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Cancelled,
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while core.steer_task(&task_id, "should fail").await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled runtime handle should be removed");
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
    let mut rx = run_test_stream(
        &core,
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
            .any(|event| matches!(event, Event::TaskLifecycle(piko_protocol::TaskEvent::Idle { session_id, .. }) if session_id == "session_1"))
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

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    let mut display = Box::pin(display);
    let mut persist = Box::pin(persist);
    let mut lifecycle = Box::pin(lifecycle);
    let mut display_events = Vec::new();
    let mut persist_events = Vec::new();
    let mut lifecycle_events = Vec::new();

    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            event = display.next() => {
                if let Some(event) = event {
                    display_events.push(event);
                }
            }
            event = persist.next() => {
                if let Some(event) = event {
                    persist_events.push(event);
                }
            }
            event = lifecycle.next() => {
                if let Some(event) = event {
                    lifecycle_events.push(event);
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }

        let saw_created = lifecycle_events.iter().any(|event| matches!(
            event.as_ref(),
            piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Created { session_id, .. })
                if session_id == "session_typed"
        ));
        let saw_text = display_events.iter().any(|event| {
            matches!(
                event.as_ref(),
                DisplayEvent::TextDelta { delta, .. } if delta == "typed channel response"
            )
        });
        let saw_task_persist = persist_events.iter().any(|event| matches!(
            event.as_ref(),
            PersistEvent::TaskEventCommitted(piko_protocol::TaskEvent::Created { session_id, .. })
                if session_id == "session_typed"
        ));
        let saw_user = persist_events.iter().any(|event| {
            matches!(
                event.as_ref(),
                PersistEvent::UserCommitted { session_id, message, .. }
                    if session_id == "session_typed"
                        && matches!(message, piko_protocol::Message::User {
                            content: piko_protocol::MessageContent::String(text), ..
                        } if text == "hello")
            )
        });
        let saw_finalized = persist_events.iter().any(|event| {
            matches!(
                event.as_ref(),
                PersistEvent::Finalized { session_id, message, .. }
                    if session_id == "session_typed"
                        && matches!(message, piko_protocol::Message::Assistant { content, .. }
                            if content.iter().any(|block| matches!(
                                block,
                                piko_protocol::ContentBlock::Text { text }
                                    if text == "typed channel response"
                            )))
            )
        });
        if saw_created && saw_text && saw_task_persist && saw_user && saw_finalized {
            break;
        }
    }

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
        PersistEvent::TaskEventCommitted(piko_protocol::TaskEvent::Created { session_id, .. })
            if session_id == "session_typed"
    )));
    assert!(persist_events.iter().any(|event| matches!(
        event.as_ref(),
        PersistEvent::UserCommitted { session_id, message, .. }
            if session_id == "session_typed"
                && matches!(message, piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String(text), ..
                } if text == "hello")
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
async fn test_root_lifecycle_updates_supervisor_snapshot() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("snapshot response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("snapshot-root")).await;

    let mut channels = core
        .run_streaming_channels(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("snapshot-root".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_snapshot_root".to_string(),
                    turn_id: "turn_snapshot_root".to_string(),
                }),
            }),
        )
        .await;
    let display = channels.display_stream().unwrap();
    let persist = channels.persist_stream().unwrap();
    let mut lifecycle = channels.lifecycle_stream().unwrap();
    drop(channels);

    tokio::spawn(async move { display.collect::<Vec<_>>().await });
    tokio::spawn(async move { persist.collect::<Vec<_>>().await });

    let mut task_id = None;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), lifecycle.next())
            .await
            .unwrap()
            .expect("expected lifecycle event");
        match event.as_ref() {
            piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Created {
                task_id: created_task_id,
                ..
            }) => task_id = Some(created_task_id.clone()),
            piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Idle { .. }) => break,
            _ => {}
        }
    }

    let task_id = task_id.expect("expected task id");
    let snapshot = core.snapshot().await;
    assert!(matches!(
        snapshot.tasks.get(&task_id).map(|task| &task.status),
        Some(orchd::protocol::agents::AgentTaskStatus::Idle)
    ));
}

#[tokio::test]
async fn test_task_control_close_reopen_and_steer() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_text("second response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("controlled")).await;

    let mut channels = core
        .run_streaming_channels(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("controlled".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_control".to_string(),
                    turn_id: "turn_control".to_string(),
                }),
            }),
        )
        .await;
    let display = channels.display_stream().unwrap();
    let persist = channels.persist_stream().unwrap();
    let mut lifecycle = channels.lifecycle_stream().unwrap();
    drop(channels);

    tokio::spawn(async move { display.collect::<Vec<_>>().await });
    tokio::spawn(async move { persist.collect::<Vec<_>>().await });

    let mut task_id = None;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), lifecycle.next())
            .await
            .unwrap()
            .expect("expected lifecycle event");
        match event.as_ref() {
            piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Created {
                task_id: created_task_id,
                ..
            }) => {
                task_id = Some(created_task_id.clone());
            }
            piko_protocol::LifecycleEvent::Task(piko_protocol::TaskEvent::Idle { .. }) => break,
            _ => {}
        }
    }

    let task_id = task_id.expect("expected task id");
    assert!(core.close_task(&task_id).await);
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Closed,
    )
    .await;
    let snapshot = core.snapshot().await;
    let closed_status = snapshot.tasks.get(&task_id).map(|task| task.status.clone());
    assert!(
        matches!(
            closed_status,
            Some(orchd::protocol::agents::AgentTaskStatus::Closed)
        ),
        "expected Closed, got {closed_status:?}"
    );

    assert!(core.reopen_task(&task_id).await);
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Idle,
    )
    .await;
    let snapshot = core.snapshot().await;
    let reopened_status = snapshot.tasks.get(&task_id).map(|task| task.status.clone());
    assert!(
        matches!(
            reopened_status,
            Some(orchd::protocol::agents::AgentTaskStatus::Idle)
        ),
        "expected Idle, got {reopened_status:?}"
    );

    let result = core
        .run(
            "resume",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("controlled".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext {
                    session_id: "session_control".to_string(),
                    turn_id: "turn_control_2".to_string(),
                }),
            }),
        )
        .await;

    assert!(result.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "second response"
            ))
    )));
}

#[tokio::test]
async fn test_spawn_root_agent_local_stream_preserves_task_persist_facts() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("local stream response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("local-stream")).await;

    let mut stream = core
        .spawn_root_agent(
            test_agent_spec("local-stream"),
            "hello".to_string(),
            Some(HostTaskContext {
                session_id: "session_local".to_string(),
                turn_id: "turn_local".to_string(),
            }),
        )
        .await;

    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created { session_id, .. })
            if session_id == "session_local"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Persist(PersistEvent::TaskEventCommitted(
            piko_protocol::TaskEvent::Created { session_id, .. }
        )) if session_id == "session_local"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Persist(PersistEvent::UserCommitted { session_id, message, .. })
            if session_id == "session_local"
                && matches!(message, piko_protocol::Message::User {
                    content: piko_protocol::MessageContent::String(text), ..
                } if text == "hello")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Persist(PersistEvent::Finalized { session_id, message, .. })
            if session_id == "session_local"
                && matches!(message, piko_protocol::Message::Assistant { content, .. }
                    if content.iter().any(|block| matches!(
                        block,
                        piko_protocol::ContentBlock::Text { text }
                            if text == "local stream response"
                    )))
    )));
}

#[tokio::test]
async fn test_spawn_root_agent_without_host_context_emits_runtime_task_lifecycle() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("local runtime response").await;
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("local-runtime")).await;

    let mut stream = core
        .spawn_root_agent(test_agent_spec("local-runtime"), "hello".to_string(), None)
        .await;

    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(piko_protocol::TaskEvent::Created { session_id, turn_id, .. })
            if !session_id.is_empty() && !turn_id.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Persist(PersistEvent::TaskEventCommitted(
            piko_protocol::TaskEvent::Created { session_id, turn_id, .. }
        )) if !session_id.is_empty() && !turn_id.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TaskLifecycle(
            piko_protocol::TaskEvent::Idle { session_id, summary, .. }
            | piko_protocol::TaskEvent::Completed { session_id, summary, .. }
        ) if !session_id.is_empty() && summary == "local runtime response"
    )));
}

#[tokio::test]
async fn test_spawn_root_agent_without_host_context_emits_tool_result_committed() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "need a tool",
        vec![CannedToolCall {
            id: "call_missing_local".to_string(),
            name: "missing_tool".to_string(),
            arguments: serde_json::json!({"path": "nope"}),
        }],
    ))
    .await;
    faux.push_text("done after local tool").await;

    let config = test_config();
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("tool-local")).await;

    let mut stream = core
        .spawn_root_agent(test_agent_spec("tool-local"), "use tool".to_string(), None)
        .await;

    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(events.iter().any(|event| matches!(
        event,
        Event::Persist(PersistEvent::ToolResultCommitted { session_id, message, .. })
            if !session_id.is_empty()
                && matches!(message, piko_protocol::Message::ToolResult { tool_call_id, is_error, .. }
                    if tool_call_id == "call_missing_local" && *is_error == Some(true))
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
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    let mut persist = Box::pin(persist);
    let mut persist_events = Vec::new();
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(50), persist.next()).await
        {
            persist_events.push(event);
        }

        let saw_tool_call = persist_events.iter().any(|event| {
            matches!(
                event.as_ref(),
                PersistEvent::ToolCallCommitted { session_id, message, .. }
                    if session_id == "session_tool"
                        && matches!(message, piko_protocol::Message::ToolCall { id, .. }
                            if id == "call_missing")
            )
        });
        let saw_tool_result = persist_events.iter().any(|event| matches!(
            event.as_ref(),
            PersistEvent::ToolResultCommitted { session_id, message, .. }
                if session_id == "session_tool"
                    && matches!(message, piko_protocol::Message::ToolResult { tool_call_id, is_error, .. }
                        if tool_call_id == "call_missing" && *is_error == Some(true))
        ));
        if saw_tool_call && saw_tool_result {
            break;
        }
    }

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

#[tokio::test]
async fn test_reused_root_task_recovers_after_gateway_failure() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_error("temporary failure").await;
    faux.push_text("recovered response").await;
    let config = test_config();
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("recovering-root"))
        .await;

    let options = |turn_id: &str| {
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("recovering-root".into()),
            },
            history: None,
            host_context: Some(HostTaskContext {
                session_id: "session_recovering_root".into(),
                turn_id: turn_id.into(),
            }),
        })
    };

    let first = core.run("first", options("turn_1")).await;
    assert_eq!(first.status, orchd::protocol::runtime::RunStatus::Completed);
    let first_snapshot = core.snapshot().await;
    let task_id = first_snapshot
        .tasks
        .keys()
        .next()
        .expect("root task registered")
        .clone();

    let failed = core.run("fail once", options("turn_2")).await;
    assert_eq!(failed.status, orchd::protocol::runtime::RunStatus::Error);
    let failed_snapshot = core.snapshot().await;
    assert_eq!(
        failed_snapshot.tasks.len(),
        1,
        "unexpected root tasks: {:?}",
        failed_snapshot
            .tasks
            .iter()
            .map(|(id, task)| (id, &task.status))
            .collect::<Vec<_>>()
    );
    wait_for_task_status(
        &core,
        &task_id,
        orchd::protocol::agents::AgentTaskStatus::Failed,
    )
    .await;

    let recovered = core.run("try again", options("turn_3")).await;
    assert_eq!(
        recovered.status,
        orchd::protocol::runtime::RunStatus::Completed
    );
    assert!(recovered.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "recovered response"
            ))
    )));
    let recovered_snapshot = core.snapshot().await;
    assert_eq!(recovered_snapshot.tasks.len(), 1);
    assert!(matches!(
        recovered_snapshot
            .tasks
            .get(&task_id)
            .map(|task| &task.status),
        Some(orchd::protocol::agents::AgentTaskStatus::Idle)
    ));
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

#[tokio::test]
async fn test_root_task_reuse_is_scoped_by_session() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("session a first").await;
    faux.push_text("session b first").await;
    faux.push_text("session a second").await;
    let config = test_config();
    let core = Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("shared-agent")).await;

    let options = |session_id: &str, turn_id: &str| {
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("shared-agent".into()),
            },
            history: None,
            host_context: Some(HostTaskContext {
                session_id: session_id.into(),
                turn_id: turn_id.into(),
            }),
        })
    };

    core.run("a1", options("session_a", "turn_a1")).await;
    core.run("b1", options("session_b", "turn_b1")).await;
    let second_a = core.run("a2", options("session_a", "turn_a2")).await;

    assert!(second_a.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "session a second"
            ))
    )));
    assert_eq!(core.snapshot().await.tasks.len(), 2);
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
    let mut rx = run_test_stream(
        &core,
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
