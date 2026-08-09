// ---- World-state baseline invalidation on compaction (F-04 slice 2) ----

/// Captures frozen prompt resources per turn so the test can observe the
/// full-vs-diff world-state injection across the compact boundary.
struct WorldStateCapturingRunner(Arc<std::sync::Mutex<Vec<piko_protocol::PromptResourceSnapshot>>>);

#[async_trait]
impl AgentRunRunner for WorldStateCapturingRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        if let Some(resources) = &input.prompt_resources {
            self.0.lock().unwrap().push(resources.clone());
        }
        DistinctIdRunRunner.run_agent(input).await
    }
}

fn world_state_content(snapshot: &piko_protocol::PromptResourceSnapshot) -> Option<String> {
    match &snapshot.world_state {
        Some(piko_protocol::Message::Context {
            content: piko_protocol::MessageContent::String(text),
            ..
        }) => Some(text.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn compaction_clears_world_state_baseline_and_next_run_reinjects_full() {
    struct PanicGateway;

    #[async_trait]
    impl InferenceGateway for PanicGateway {
        async fn start(
            &self,
            _req: InferenceRequest,
            _cancel: CancellationToken,
        ) -> Result<InferenceExecution, InferenceError> {
            panic!("new-context-window compaction must not call the model")
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = HostServer::with_storage_runner_settings(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(WorldStateCapturingRunner(captured.clone())),
        HostSettings::default(),
    );
    server.set_model_executor(Arc::new(PanicGateway)).await;

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created");
    let root = format!("agent_{session_id}_root");

    for (index, text) in ["first", "second"].iter().enumerate() {
        let events = server
            .handle_command(Command::ChatSubmit {
                command_id: format!("submit-{index}"),
                session_id: session_id.clone(),
                target_agent_instance_id: root.clone(),
                text: text.to_string(),
            })
            .await;
        assert!(
            events.iter().any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed { .. })
            )),
            "turn must complete; events={events:?}"
        );
    }

    // Before compact: full snapshot then diff, and a durable baseline.
    let snapshots = captured.lock().unwrap().clone();
    assert_eq!(snapshots.len(), 2);
    let first = world_state_content(&snapshots[0]).expect("full world-state on run 1");
    assert!(first.contains("session_id:"));
    assert!(first.contains("run_kind: initial"));
    let second = world_state_content(&snapshots[1]).expect("diff world-state on run 2");
    assert!(second.starts_with("world-state changed since the previous run:"));
    let repo = JsonlSessionRepository::new(temp.path());
    let persisted = repo
        .list(None)
        .expect("list sessions")
        .into_iter()
        .find(|session| session.state.session_id == session_id)
        .expect("persisted session");
    assert!(
        persisted.state.world_state_baseline.is_some(),
        "baseline recorded before compaction"
    );

    let compact_events = server
        .handle_command(Command::SessionCompact {
            command_id: "compact".into(),
            session_id: session_id.clone(),
            agent_instance_id: root.clone(),
            mode: piko_protocol::command::CompactMode::NewContextWindow,
        })
        .await;
    assert!(
        compact_events
            .iter()
            .any(|event| matches!(event, Event::SessionReconciled(_))),
        "compact must reconcile; events={compact_events:?}"
    );

    // After compact: durable baseline cleared, so the next run re-injects full.
    let persisted = repo
        .list(None)
        .expect("list sessions")
        .into_iter()
        .find(|session| session.state.session_id == session_id)
        .expect("persisted session");
    assert!(
        persisted.state.world_state_baseline.is_none(),
        "compaction clears the world-state baseline"
    );

    let events = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit-2".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root,
            text: "third".into(),
        })
        .await;
    assert!(
        events
            .iter()
            .any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_hostd::api::TurnEvent::Completed { .. })
            )),
        "third turn must complete"
    );
    let snapshots = captured.lock().unwrap().clone();
    let third = world_state_content(&snapshots[2]).expect("world-state on run 3");
    assert!(
        third.contains("session_id:") && !third.starts_with("world-state changed"),
        "baseline cleared ⇒ full re-injection; content={third:?}"
    );
}
