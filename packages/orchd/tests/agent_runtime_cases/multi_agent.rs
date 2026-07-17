#[tokio::test]
async fn multi_agent_tools_use_trusted_context_for_attached_and_detached_spawn() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model.push_text("attached report").await;
    model.push_text("detached report").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-exec".into(),
        cancellation: None,
        agent_id: "main".into(),
        tool_set_ids: Vec::new(),
        turn_index: Some(1),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
    };

    let attached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-attached".into(),
                name: "spawn_agent".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do attached work",
                    "session_id": "forged-session",
                    "parent_agent_instance_id": "forged-parent"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(attached.ok, "attached spawn failed: {:?}", attached.error);
    assert_eq!(
        attached.value.as_ref().unwrap()["summary"],
        "attached report"
    );
    assert!(
        attached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    let terminal_attempts_before_detached = commits.terminal_attempts.load(Ordering::SeqCst);
    commits.fail_next_run_terminal();
    commits.fail_next_report_commit();
    let detached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-detached".into(),
                name: "spawn_agent_detached".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(detached.ok, "detached spawn failed: {:?}", detached.error);
    assert_eq!(detached.value.as_ref().unwrap()["status"], "accepted");
    assert!(
        detached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    for _ in 0..100 {
        if commits.terminal_attempts.load(Ordering::SeqCst) > terminal_attempts_before_detached {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .unwrap()
            .items
            .is_empty()
    );

    for _ in 0..100 {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .expect("root inbox");
        if inbox
            .items
            .iter()
            .any(|item| item.report.summary == "detached report")
        {
            let commands = commits.commands.lock().await;
            let start_index = commands
                .iter()
                .position(|command| {
                    matches!(
                        command,
                        AgentDurableCommand::RunStarted {
                            detached_recipient_agent_instance_id: Some(recipient),
                            ..
                        } if recipient == "root"
                    )
                })
                .expect("detached registration must be durable");
            let (run_id, terminal_index) = commands
                .iter()
                .enumerate()
                .find_map(|(index, command)| match command {
                    AgentDurableCommand::RunTerminal { run_id, report, .. }
                        if report.summary == "detached report" =>
                    {
                        Some((run_id, index))
                    }
                    _ => None,
                })
                .expect("detached terminal must be durable");
            let delivery_index = commands
                .iter()
                .position(|command| {
                    matches!(
                        command,
                        AgentDurableCommand::CommitReport { report, .. }
                            if report.summary == "detached report"
                    )
                })
                .expect("detached report must be committed");
            assert!(start_index < terminal_index);
            assert!(terminal_index < delivery_index);
            assert!(
                commands[start_index..terminal_index]
                    .iter()
                    .any(|command| matches!(
                        command,
                        AgentDurableCommand::RunStarted { run_id: started, .. } if started == run_id
                    ))
            );
            drop(commands);
            let collected = provider
                .execute(
                    piko_protocol::ToolCall {
                        id: "call-collect".into(),
                        name: "collect_agent_reports".into(),
                        arguments: serde_json::json!({}),
                        partial_json: None,
                    },
                    context,
                )
                .await;
            assert!(collected.ok);
            assert_eq!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]["summary"],
                "detached report"
            );
            assert!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]
                    .get("execution_id")
                    .is_none()
            );
            let consumed = runtime
                .agent_inbox("session-1".into(), "root".into())
                .await
                .unwrap();
            assert!(consumed.items[0].consumed_at.is_some());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("detached report was not delivered to the durable parent inbox");
}


