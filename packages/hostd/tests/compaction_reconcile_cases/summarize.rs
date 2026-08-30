// ---- Compaction summarize-mode acceptance ----

#[tokio::test]
async fn session_compact_emits_session_reconciled_when_history_rewritten() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    // Default compaction thresholds keep short transcripts below the auto-compact
    // waterline so root chat does not consume the opportunity; SessionCompact
    // still forces a rewrite via context_window = 0.
    let server = HostServer::with_storage_runner_settings(
        repo,
        Arc::new(CompactAgentRunRunner::new()),
        HostSettings::default(),
    );
    server.set_model_executor(Arc::new(SummaryGateway)).await;

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

    let turn_events = server
        .handle_command(Command::submit_follow_up(
            "submit",
session_id.clone(),
format!("agent_{session_id}_root"),
piko_protocol::MessageContent::String("hello".into()),
))
        .await;
    assert!(
        turn_events.iter().any(|event| matches!(
            event,
            Event::SessionReconciled(reconciled)
                if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none())
        )),
        "turn must complete before compact; events={turn_events:?}"
    );

    let compact_events = server
        .handle_command(Command::SessionCompact {
            command_id: "compact".into(),
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
            mode: piko_protocol::command::CompactMode::Summarize,
        })
        .await;

    let reconciled = compact_events.iter().find_map(|event| match event {
        Event::SessionReconciled(reconciled) => Some(reconciled),
        _ => None,
    });
    let Some(reconciled) = reconciled else {
        panic!(
            "compact that rewrites the view must emit SessionReconciled; events={compact_events:?}"
        );
    };
    assert_eq!(
        reconciled.reason,
        piko_protocol::ReconcileReason::ExplicitRefresh
    );
    assert!(
        reconciled
            .snapshot
            .entries
            .iter()
            .any(|entry| matches!(entry, piko_hostd::api::SessionTreeEntry::Compaction(_))),
        "reconciled snapshot should include compaction entry; entries={:?}",
        reconciled.snapshot.entries
    );
}

/// Runner that derives message ids from the turn id so repeated turns build a
/// linear user/assistant history.

#[tokio::test]
async fn summarizer_failure_falls_back_to_default_model() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_runner_settings(
        repo,
        Arc::new(CompactAgentRunRunner::new()),
        HostSettings {
            compaction: Some(CompactionSettings {
                enabled: Some(true),
                reserve_tokens: Some(16384),
                keep_recent_tokens: Some(20000),
                min_growth_tokens: Some(16384),
                min_growth_fraction: None,
                summarizer_model: Some("summarizer-model".into()),
                summarizer_provider: Some("test-provider".into()),
            }),
            ..HostSettings::default()
        },
    );
    let gateway = Arc::new(FailingOnceGateway::new());
    server
        .set_model_executor(Arc::clone(&gateway) as Arc<dyn InferenceGateway>)
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
        .handle_command(Command::submit_follow_up(
            "submit",
session_id.clone(),
format!("agent_{session_id}_root"),
piko_protocol::MessageContent::String("hello".into()),
))
        .await;

    let compact_events = server
        .handle_command(Command::SessionCompact {
            command_id: "compact".into(),
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
            mode: piko_protocol::command::CompactMode::Summarize,
        })
        .await;

    assert!(
        compact_events
            .iter()
            .any(|event| matches!(event, Event::SessionReconciled(_))),
        "fallback compaction must land; events={compact_events:?}"
    );
    let calls = gateway.calls.lock().unwrap().clone();
    assert_eq!(calls, vec!["summarizer-model", "default"]);
}
