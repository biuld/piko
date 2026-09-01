use super::*;

#[tokio::test]
async fn root_chat_while_active_is_queued_until_prior_turn_terminals() {
    use piko_hostd::api::{Command, ServerMessage as Event};
    use piko_hostd::infra::storage::JsonlSessionRepository;

    let runner = GatedAgentRunRunner::new();
    let prompts = Arc::clone(&runner.prompts);
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(runner.clone()));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("unexpected {other:?}"),
    };

    let first = {
        let server = server.clone();
        let session_id = session_id.clone();
        tokio::spawn(async move {
            server
                .handle_command(Command::submit_follow_up(
                    "submit-1",
                    session_id.clone(),
                    format!("agent_{session_id}_root"),
                    piko_protocol::MessageContent::String("first".into()),
                ))
                .await
        })
    };

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if !prompts.lock().unwrap().is_empty() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first turn should start");

    let mut second = server.handle_command_stream(Command::submit_follow_up(
        "submit-2",
        session_id.clone(),
        format!("agent_{session_id}_root"),
        piko_protocol::MessageContent::String("second".into()),
    ));
    let mut second_events = Vec::new();
    for _ in 0..4 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), second.recv())
            .await
            .expect("queued receipt events should arrive")
            .expect("queued command stream should remain open");
        let queued = matches!(
            event,
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { ref receipt, .. }),
                ..
            } if receipt.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp
        );
        second_events.push(event);
        if queued {
            break;
        }
    }

    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            command_id,
            result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { receipt, .. }),
        } if command_id == "submit-2"
            && receipt.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp
    )));
    assert_eq!(
        prompts.lock().unwrap().as_slice(),
        ["first"],
        "second submit must not start a concurrent root turn"
    );

    runner.release();
    while let Some(event) = second.recv().await {
        second_events.push(event);
    }
    let first_events = first.await.expect("first turn join");
    assert!(first_events.iter().any(|event| matches!(
        event,
        Event::SessionReconciled(reconciled)
            if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none())
    )));

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if prompts.lock().unwrap().len() >= 2 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queued second turn should drain after first terminals");

    assert_eq!(
        prompts.lock().unwrap().as_slice(),
        ["first", "second"],
        "queued root chat must run after prior turn terminals"
    );
    // Live progress is streamed as interaction/realtime events; the host
    // reconciliation snapshots are durable admission and terminal barriers.
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, Event::Interaction(_)))
    );
    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::SessionReconciled(reconciled)
            if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none())
    )));
}
