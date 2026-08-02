// ---- Compaction pending-guard acceptance ----

#[tokio::test]
async fn concurrent_compacts_produce_a_single_rewrite() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_runner_settings(
        repo,
        Arc::new(CompactAgentRunRunner),
        HostSettings::default(),
    );
    let (started_tx, started_rx) = support::test_oneshot();
    let (release_tx, release_rx) = support::test_oneshot();
    server
        .set_model_executor(Arc::new(BlockingGateway::new(started_tx, release_rx)))
        .await;

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

    server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "hello".into(),
        })
        .await;

    let compact = |command_id: String| {
        let server = server.clone();
        let session_id = session_id.clone();
        async move {
            let agent_instance_id = format!("agent_{session_id}_root");
            server
                .handle_command(Command::SessionCompact {
                    command_id,
                    session_id,
                    agent_instance_id,
                    mode: piko_protocol::command::CompactMode::Summarize,
                })
                .await
        }
    };

    let first = tokio::spawn(compact("compact-1".into()));
    started_rx.await.expect("first summarizer started");
    let second = tokio::spawn(compact("compact-2".into()));
    let second_events = second.await.unwrap();
    assert!(
        second_events
            .iter()
            .all(|event| !matches!(event, Event::SessionReconciled(_))),
        "second compact must be skipped by the pending guard; events={second_events:?}"
    );

    let _ = release_tx.send(());
    let first_events = first.await.unwrap();
    let reconciles = first_events
        .iter()
        .filter(|event| matches!(event, Event::SessionReconciled(_)))
        .count();
    assert_eq!(
        reconciles, 1,
        "exactly one rewrite; events={first_events:?}"
    );
}
