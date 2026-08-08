use super::*;

#[tokio::test]
async fn root_chat_reuses_session_sink_across_turns() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server =
        HostServer::with_storage_and_runner(repo, Arc::new(ReuseRootAgentRunRunner::default()));
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
        other => panic!("expected session_created, got {other:?}"),
    };

    for (command_id, text) in [("submit-1", "hello"), ("submit-2", "follow up")] {
        let turn_events = server
            .handle_command(Command::ChatSubmit {
                command_id: command_id.into(),
                session_id: session_id.clone(),
                target_agent_instance_id: format!("agent_{session_id}_root"),
                text: text.into(),
            })
            .await;
        for event in &turn_events {
            if let Event::CommandResponse {
                result: Err(err), ..
            } = event
            {
                panic!("turn {command_id} failed: {err}");
            }
        }
        assert!(
            turn_events.iter().any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_protocol::TurnEvent::Started { .. })
            )),
            "turn {command_id} must emit TurnStarted for TUI spinner; events={turn_events:?}"
        );
        assert!(
            turn_events.iter().any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
            )),
            "turn {command_id} should complete"
        );
    }

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
    assert_eq!(snapshot.entries.len(), 4);
}

#[tokio::test]
async fn in_memory_session_navigate_to_root_user_appends_leaf_and_resets_current_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner));
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
        other => panic!("expected session_created, got {other:?}"),
    };

    let _ = server
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "hello".into(),
        })
        .await;

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
    let root_user_id = snapshot.entries[0].id().to_string();

    let navigated = server
        .handle_command(Command::SessionNavigate {
            command_id: "navigate".into(),
            session_id: session_id.clone(),
            entry_id: root_user_id.clone(),
            summarize: false,
            custom_instructions: None,
        })
        .await;

    assert!(matches!(
        &navigated[0],
        Event::CommandResponse { result: Ok(piko_hostd::api::CommandResult::SessionNavigated {
            new_leaf_id: None,
            selected_entry_id,
            editor_text: Some(text),
            ..
        }), .. } if selected_entry_id == &root_user_id && text == "hello"
    ));
    let Event::SessionReconciled(reconciled) = &navigated[1] else {
        panic!("expected session reconciled");
    };
    assert_eq!(reconciled.snapshot.current_leaf_id, None);
    assert!(matches!(
        reconciled.snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
    ));
}

#[tokio::test]
async fn jsonl_server_round_trips_events() {
    let input = serde_json::to_string(&Command::SessionCreate {
        command_id: "create".into(),
        cwd: "/tmp/project".into(),
    })
    .unwrap()
        + "\n";
    let (mut read_out, write_out) = tokio::io::duplex(4096);
    run_jsonl_server(
        std::io::Cursor::new(input.into_bytes()),
        write_out,
        HostServer::new(),
    )
    .await
    .unwrap();

    let mut output = String::new();
    read_out.read_to_string(&mut output).await.unwrap();
    let mut lines = output.lines();
    let event = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::SessionCreated { .. }),
            ..
        }
    ));
    let reconciled = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(reconciled, Event::SessionReconciled(_)));
}

#[tokio::test]
async fn jsonl_server_reads_next_command_while_turn_is_running() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
    }));
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
        other => panic!("expected session_created, got {other:?}"),
    };

    let (client_in, server_in) = tokio::io::duplex(4096);
    let (server_out, client_out) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        run_jsonl_server(BufReader::new(server_in), server_out, server)
            .await
            .unwrap();
    });
    let mut writer = client_in;
    let mut reader = BufReader::new(client_out);

    let submit = serde_json::to_string(&Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        text: "hello".into(),
    })
    .unwrap();
    writer.write_all(submit.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();

    let mut line = String::new();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_started should arrive")
        .unwrap();
    let event = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::Empty),
            ..
        }
    ));

    line.clear();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_started should arrive")
        .unwrap();
    let event = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        event,
        Event::TurnLifecycle(piko_hostd::api::TurnEvent::Started { .. })
    ));

    let approval = serde_json::to_string(&Command::ApprovalRespond {
        command_id: "approval".into(),
        session_id,
        approval_id: "approval-1".into(),
        decision: ApprovalDecision::Accept,
        note: None,
    })
    .unwrap();
    writer.write_all(approval.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut saw_approval_ack = false;
    let mut saw_approval_event = false;
    for _ in 0..10 {
        line.clear();
        tokio::time::timeout(Duration::from_millis(500), reader.read_line(&mut line))
            .await
            .expect("approval output should not wait for turn completion")
            .unwrap();
        let value = serde_json::from_str::<serde_json::Value>(line.trim()).unwrap();
        if value.get("kind").and_then(|v| v.as_str()) == Some("command_response")
            && value.get("command_id").and_then(|v| v.as_str()) == Some("approval")
        {
            saw_approval_ack = true;
        }
        if value.get("kind").and_then(|v| v.as_str()) == Some("approval")
            && value.get("type").and_then(|v| v.as_str()) == Some("resolved")
        {
            saw_approval_event = true;
        }
        if saw_approval_ack && saw_approval_event {
            break;
        }
    }
    assert!(saw_approval_ack);
    assert!(saw_approval_event);

    finish.notify_one();
    drop(writer);
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_config_update_returns_config_changed_event() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::ConfigUpdate {
            command_id: "cfg-1".into(),
            patch: serde_json::json!({}),
        })
        .await;

    let mut found = false;
    for event in events {
        if let Event::Model(piko_hostd::api::ModelEvent::ConfigChanged { .. }) = event {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "expected ModelEvent::ConfigChanged in response to ConfigUpdate"
    );
}
