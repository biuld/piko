use super::*;

#[tokio::test]
async fn parent_next_run_injects_unread_completion_before_input() {
    let (runtime, agents, executions, model) = attached_runtime_ports().await;
    let runtime = Arc::new(runtime);
    model.push_text("detached report").await;
    model.push_text("parent continues").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = v2_context();

    let detached = provider
        .execute(
            v2_call(
                "spawn_agent_detached",
                serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
            ),
            context.clone(),
        )
        .await;
    assert!(detached.ok, "detached spawn failed: {:?}", detached.error);

    let report_id = loop {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .expect("root inbox");
        if let Some(item) = inbox
            .items
            .iter()
            .find(|item| item.report.summary == "detached report")
        {
            break item.report_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    };

    // Parent's next turn should inject the completion before the user input.
    runtime
        .wait_sent_agent(SendAgentInputRequest {
            request_id: "parent-after-child".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "parent-message-after-child".into(),
            content: MessageContent::String("continue after child".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    let completion_id = piko_protocol::agent_completion_message_id(&report_id);
    let messages = executions.messages();
    let completion = messages
        .iter()
        .find(|commit| commit.message_id == completion_id)
        .expect("completion message must be committed on parent run start");
    assert!(matches!(
        &completion.message,
        piko_protocol::Message::Context {
            source,
            content: MessageContent::String(text),
            ..
        } if source.kind == piko_protocol::AGENT_COMPLETION_SOURCE_KIND
            && source.locator == report_id
            && text.contains("outcome: succeeded")
            && text.contains("summary: detached report")
    ));
    let commands = agents.commands.lock().await;
    let input_parent_message_id = commands
        .iter()
        .find_map(|command| match command {
            AgentDurableCommand::AgentInputProcessingStarted {
                input_message_id,
                input_parent_message_id,
                ..
            } if input_message_id == "parent-message-after-child" => {
                Some(input_parent_message_id.as_deref())
            }
            _ => None,
        })
        .expect("parent input commit");
    assert_eq!(
        input_parent_message_id,
        Some(completion_id.as_str()),
        "durable chain places completion immediately before the run input"
    );
    drop(commands);

    // Inbox remains unread until collect.
    let inbox = runtime
        .agent_inbox("session-1".into(), "root".into())
        .await
        .unwrap();
    let item = inbox
        .items
        .iter()
        .find(|item| item.report_id == report_id)
        .expect("report stays in inbox");
    assert!(item.consumed_at.is_none());

    // Second parent run must not inject again (transcript already carries it).
    model.push_text("second parent turn").await;
    let before = executions.messages().len();
    runtime
        .wait_sent_agent(SendAgentInputRequest {
            request_id: "parent-after-inject".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "parent-message-2".into(),
            content: MessageContent::String("again".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();
    let completions_after = executions
        .messages()
        .into_iter()
        .skip(before)
        .filter(|commit| commit.message_id.starts_with("agent.completion/"))
        .count();
    assert_eq!(completions_after, 0, "idempotent: no second completion commit");
}

#[tokio::test]
async fn consumed_inbox_skips_completion_injection() {
    let (runtime, _agents, executions, model) = attached_runtime_ports().await;
    let runtime = Arc::new(runtime);
    model.push_text("detached report").await;
    model.push_text("after collect").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = v2_context();

    let detached = provider
        .execute(
            v2_call(
                "spawn_agent_detached",
                serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
            ),
            context.clone(),
        )
        .await;
    assert!(detached.ok);

    let report_id = loop {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .unwrap();
        if let Some(item) = inbox
            .items
            .iter()
            .find(|item| item.report.summary == "detached report")
        {
            break item.report_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    };

    let collected = provider
        .execute(v2_call("collect_agent_reports", serde_json::json!({})), context)
        .await;
    assert!(collected.ok);
    assert_eq!(
        collected.value.as_ref().unwrap()["reports"][0]["report_id"],
        report_id
    );

    runtime
        .wait_sent_agent(SendAgentInputRequest {
            request_id: "parent-after-collect".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            root_input_id: None,
            message_id: "parent-after-collect-msg".into(),
            content: MessageContent::String("after collect".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    assert!(
        executions
            .messages()
            .iter()
            .all(|commit| !commit.message_id.starts_with("agent.completion/")),
        "consumed inbox must not inject a completion fragment"
    );
}
