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
                .handle_command(Command::ChatSubmit {
                    command_id: "submit-1".into(),
                    target_agent_instance_id: format!("agent_{session_id}_root"),
                    session_id: session_id.clone(),
                    text: "first".into(),
                })
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

    let mut second = server.handle_command_stream(Command::ChatSubmit {
        command_id: "submit-2".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        text: "second".into(),
    });
    let mut second_events = Vec::new();
    for _ in 0..2 {
        second_events.push(
            tokio::time::timeout(std::time::Duration::from_secs(2), second.recv())
                .await
                .expect("queued receipt events should arrive")
                .expect("queued command stream should remain open"),
        );
    }

    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            command_id,
            result: Ok(piko_hostd::api::CommandResult::Empty),
        } if command_id == "submit-2"
    )));
    assert!(
        second_events.iter().any(|event| matches!(
            event,
            Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
                agent_instance_id,
                ..
            }) if agent_instance_id == &format!("agent_{session_id}_root")
        )),
        "second root chat must queue while prior turn is active; events={second_events:?}"
    );
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
    assert!(
        first_events.iter().any(|event| matches!(
            event,
            Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
        )),
        "first turn should complete; events={first_events:?}"
    );

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
    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Started { .. })
    )));
    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}
