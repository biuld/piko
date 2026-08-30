// ---- World-state baseline invalidation on compaction (F-04 slice 2) ----

/// Captures frozen prompt resources per turn so the test can observe the
/// full-vs-diff world-state injection across the compact boundary.
struct WorldStateCapturingRunner(
    Arc<std::sync::Mutex<Vec<piko_protocol::PromptResourceSnapshot>>>,
    DistinctIdRunRunner,
);

#[async_trait]
impl AgentRunRunner for WorldStateCapturingRunner {
    async fn ensure_session_runtime(
        &self,
        session_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        AgentRunRunner::ensure_session_runtime(&self.1, session_id, cwd, session_dir, resume_agent)
            .await
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        if let Some(resources) = &runtime.prompt_resources {
            self.0.lock().unwrap().push(resources.clone());
        }
        AgentRunRunner::submit_agent_input(&self.1, input, runtime).await
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        AgentRunRunner::wait_agent_input_started(&self.1, session_id, agent_instance_id, input_id, disposition)
            .await
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        AgentRunRunner::wait_agent_input_completion(&self.1, session_id, agent_instance_id, input_id)
            .await
    }

    async fn finish_agent_run(&self, session_id: &str, agent_instance_id: &str, input_id: &str) {
        AgentRunRunner::finish_agent_run(&self.1, session_id, agent_instance_id, input_id).await;
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
        Arc::new(WorldStateCapturingRunner(
            captured.clone(),
            DistinctIdRunRunner::new(),
        )),
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
            .handle_command(Command::submit_follow_up(
format!("submit-{index}"),
session_id.clone(),
root.clone(),
piko_protocol::MessageContent::String(text.to_string()),
))
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
        .handle_command(Command::submit_follow_up(
            "submit-2",
session_id.clone(),
root,
piko_protocol::MessageContent::String("third".into()),
))
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
