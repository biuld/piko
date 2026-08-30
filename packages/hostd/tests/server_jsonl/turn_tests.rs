use super::*;

#[tokio::test]
async fn root_chat_streams_started_before_runner_finishes() {
    let server = HostServer::with_turn_runner(Arc::new(SlowRunner::default()));
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

    let mut events = server.handle_command_stream(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        format!("agent_{session_id}_root"),
        piko_protocol::MessageContent::String("hello".into()),
    ));

    let accepted = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        accepted,
        Event::CommandResponse {
            command_id,
            result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { .. }),
        } if command_id == "submit"
    ));

    let started = events.recv().await.unwrap();
    assert!(matches!(started, Event::SessionReconciled(reconciled)
        if reconciled.snapshot.agent_work.iter().any(|work| work.active_work.is_some())));
}

#[tokio::test]
async fn approval_response_is_not_blocked_by_active_turn() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
        ..WaitingApprovalRunner::default()
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

    let mut events = server.handle_command_stream(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        format!("agent_{session_id}_root"),
        piko_protocol::MessageContent::String("hello".into()),
    ));

    let accepted = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        accepted,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { .. }),
            ..
        }
    ));
    let started_event = events.recv().await.unwrap();
    assert!(matches!(started_event, Event::SessionReconciled(reconciled)
        if reconciled.snapshot.agent_work.iter().any(|work| work.active_work.is_some())));
    tokio::time::timeout(Duration::from_millis(50), started.notified())
        .await
        .unwrap();

    let approval_events = tokio::time::timeout(
        Duration::from_millis(50),
        server.handle_command(Command::ApprovalRespond {
            command_id: "approval".into(),
            session_id,
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        }),
    )
    .await
    .expect("approval response should not wait for the active turn to finish");

    assert!(
        approval_events.iter().any(|event| matches!(
            event,
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::Empty),
                ..
            }
        )),
        "approval should return business Empty; events={approval_events:?}"
    );
    assert!(
        approval_events.iter().any(|event| matches!(
            event,
            Event::Approval(piko_hostd::api::ApprovalEvent::Resolved { .. })
        )),
        "approval should emit resolved; events={approval_events:?}"
    );

    finish.notify_one();
}

#[tokio::test]
async fn create_session_returns_session_created() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::SessionCreate {
            command_id: "cmd-1".into(),
            cwd: "/tmp/project".into(),
        })
        .await;

    assert!(!events.is_empty());
    assert!(matches!(
        events[0],
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::SessionCreated { .. }),
            ..
        }
    ));
}

#[tokio::test]
async fn root_chat_persists_completed_assistant_as_session_entry() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(AssistantRunner::default()));
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

    let turn_events = server
        .handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;
    let committed = turn_events
        .iter()
        .filter_map(|event| match event {
            Event::TranscriptCommitted(committed) => Some(committed),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(committed.len(), 2);
    assert_eq!(committed[0].transcript_seq, 1);
    assert!(matches!(committed[0].message, Message::User { .. }));
    assert_eq!(committed[1].transcript_seq, 2);
    assert!(matches!(committed[1].message, Message::Assistant { .. }));
    let final_message_index = turn_events
        .iter()
        .rposition(|event| matches!(event, Event::TranscriptCommitted(_)))
        .unwrap();
    let completed_index = turn_events
        .iter()
        .position(|event| {
            matches!(event, Event::SessionReconciled(reconciled)
                if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none()))
        })
        .unwrap();
    assert!(
        final_message_index < completed_index,
        "completion barrier must project final transcript before terminal reconciliation"
    );

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
    assert_eq!(snapshot.entries.len(), 2);
    assert!(matches!(
        &snapshot.entries[0],
        piko_hostd::api::SessionTreeEntry::Message(entry)
            if matches!(entry.message, piko_hostd::api::Message::User { .. })
    ));
    assert!(matches!(
        &snapshot.entries[1],
        piko_hostd::api::SessionTreeEntry::Message(entry)
            if entry.id == "assistant-1"
                && matches!(entry.message, piko_hostd::api::Message::Assistant { .. })
    ));
}

#[tokio::test]
async fn rollout_pages_durable_agent_transcript_with_opaque_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(AssistantRunner::default()));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create-page".into(),
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
    let target_agent_instance_id = format!("agent_{session_id}_root");
    let _turn_events = server
        .handle_command(Command::submit_follow_up(
            "submit-page",
            session_id.clone(),
            target_agent_instance_id.clone(),
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;
    let agent_instance_id = target_agent_instance_id;

    let first = server
        .handle_command(Command::RolloutPageGet {
            command_id: "page-1".into(),
            session_id: session_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            after_cursor: None,
            limit: Some(1),
        })
        .await;
    let Event::CommandResponse {
        result: Ok(piko_hostd::api::CommandResult::RolloutPaged { page, .. }),
        ..
    } = &first[0]
    else {
        panic!("expected rollout page, got {first:?}");
    };
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].transcript_seq, 1);
    assert!(matches!(page.items[0].message, Message::User { .. }));
    let cursor = page.next_cursor.clone().expect("second page cursor");

    let second = server
        .handle_command(Command::RolloutPageGet {
            command_id: "page-2".into(),
            session_id,
            agent_instance_id,
            after_cursor: Some(cursor),
            limit: Some(1),
        })
        .await;
    let Event::CommandResponse {
        result: Ok(piko_hostd::api::CommandResult::RolloutPaged { page, .. }),
        ..
    } = &second[0]
    else {
        panic!("expected second rollout page, got {second:?}");
    };
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].transcript_seq, 2);
    assert!(matches!(page.items[0].message, Message::Assistant { .. }));
    assert!(page.next_cursor.is_none());
}
