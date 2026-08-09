// ---- Compaction new-context-window acceptance ----

#[tokio::test]
async fn new_context_window_mode_rewrites_without_calling_the_model() {
    struct PanicGateway;

    #[async_trait]
    impl LlmGateway for PanicGateway {
        async fn execute(
            &self,
            _req: ModelRequest,
            _cancel: CancellationToken,
        ) -> Result<ModelEventStream, GatewayError> {
            Err(GatewayError::new(piko_llmd::gateway::ErrorClass::Upstream, "panic", "execute", "not used"))
        }

        async fn llm_call(
            &self,
            _model: Model,
            _system_prompt: Option<String>,
            _messages: Vec<Message>,
            _settings: ModelRunSettings,
        ) -> Result<String, String> {
            panic!("new-context-window compaction must not call the model")
        }

        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_runner_settings(
        repo,
        Arc::new(DistinctIdRunRunner),
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

    for (index, text) in ["first", "second"].iter().enumerate() {
        let events = server
            .handle_command(Command::ChatSubmit {
                command_id: format!("submit-{index}"),
                session_id: session_id.clone(),
                target_agent_instance_id: format!("agent_{session_id}_root"),
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

    let compact_events = server
        .handle_command(Command::SessionCompact {
            command_id: "compact".into(),
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
            mode: piko_protocol::command::CompactMode::NewContextWindow,
        })
        .await;

    let reconciled = compact_events.iter().find_map(|event| match event {
        Event::SessionReconciled(reconciled) => Some(reconciled),
        _ => None,
    });
    let Some(reconciled) = reconciled else {
        panic!("new-context-window compact must emit SessionReconciled; events={compact_events:?}");
    };

    let entries = &reconciled.snapshot.entries;
    let compaction = entries
        .iter()
        .find_map(|entry| match entry {
            piko_hostd::api::SessionTreeEntry::Compaction(compaction) => Some(compaction),
            _ => None,
        })
        .expect("snapshot must include the checkpoint");
    assert_eq!(
        compaction.summary,
        "A new context window was started without summarizing conversation history."
    );
    assert_eq!(
        compaction.details.as_ref().unwrap()["trigger"],
        "new_context_window"
    );
    // The checkpoint must anchor the fresh window at the most recent user
    // message: everything before it is dropped from the projected view.
    let second_user_id = entries
        .iter()
        .find_map(|entry| match entry {
            piko_hostd::api::SessionTreeEntry::Message(message_entry)
                if matches!(
                    &message_entry.message,
                    piko_hostd::api::Message::User {
                        content: piko_hostd::api::MessageContent::String(text),
                        ..
                    } if text == "second"
                ) =>
            {
                Some(message_entry.id.clone())
            }
            _ => None,
        })
        .expect("second user message must be present in the tree");
    assert_eq!(
        compaction.first_kept_entry_id, second_user_id,
        "fresh window must keep the latest user message; entries={entries:?}"
    );
}
