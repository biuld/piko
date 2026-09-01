use super::*;

#[test]
fn committed_message_replaces_draft_and_rejects_late_delta() {
    let mut app = live_app();
    app.apply_event(realtime(
        "assistant-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "assistant-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "partial".into(),
        },
    ));
    app.apply_event(committed("assistant-1", 2, assistant("complete")));
    app.apply_event(realtime(
        "assistant-1",
        2,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: " stale".into(),
        },
    ));

    assert_eq!(app.timeline().message_ids(), vec!["assistant-1"]);
    assert_eq!(
        app.timeline().assistant_text("assistant-1").as_deref(),
        Some("complete")
    );
}

#[test]
fn committed_messages_use_task_seq_not_arrival_order() {
    let mut app = live_app();
    app.apply_event(committed("assistant-1", 2, assistant("answer")));
    app.apply_event(committed(
        "user-1",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("question".into()),
            timestamp: None,
        },
    ));

    assert_eq!(app.timeline().message_ids(), vec!["user-1", "assistant-1"]);
}

#[test]
fn streaming_assistant_that_arrives_first_renders_after_its_user_prompt() {
    let mut app = live_app();
    let Event::StreamItem(mut patch) = realtime(
        "assistant-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "answer".into(),
        },
    ) else {
        unreachable!("realtime helper returns a stream item")
    };
    patch.fields.as_mut().unwrap()["rootInputId"] = json!("work-1");
    app.apply_event(Event::StreamItem(patch));
    app.apply_event(committed(
        "user-1",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("question".into()),
            timestamp: None,
        },
    ));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![TimelineKind::User, TimelineKind::Assistant]
    );
}

#[test]
fn commit_before_realtime_never_creates_a_second_draft() {
    let mut app = live_app();
    app.apply_event(committed("assistant-1", 2, assistant("complete")));
    app.apply_event(realtime(
        "assistant-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "assistant-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "late".into(),
        },
    ));

    assert_eq!(app.timeline().message_ids(), vec!["assistant-1"]);
    assert_eq!(
        app.timeline().assistant_text("assistant-1").as_deref(),
        Some("complete")
    );
}

#[test]
fn conflicting_duplicate_commit_requests_authoritative_snapshot() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.apply_event(committed("assistant-1", 2, assistant("first")));

    let effects = app.apply_event(committed("assistant-1", 2, assistant("conflict")));

    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::StateSnapshot { session_id, .. })]
            if session_id == "session-1"
    ));
    assert_eq!(
        app.timeline().assistant_text("assistant-1").as_deref(),
        Some("first")
    );
}

#[test]
fn tool_start_and_end_update_one_timeline_item() {
    let mut app = live_app();

    app.apply_event(Event::StreamItem(
        piko_protocol::StreamItemPatch::from_tool_execution(
            &piko_protocol::ToolExecutionEvent::Started {
                session_id: "session-1".into(),
                agent_instance_id: "task-1".into(),
                agent_id: "agent-1".into(),
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                args: json!({ "path": "Cargo.toml" }),
                parent_message_id: Some("message-1".into()),
                root_input_id: Some("turn-1".into()),
            },
        )
        .into_iter()
        .next()
        .expect("tool stream item"),
    ));
    app.apply_event(Event::StreamItem(
        piko_protocol::StreamItemPatch::from_tool_execution(
            &piko_protocol::ToolExecutionEvent::Ended {
                session_id: "session-1".into(),
                agent_instance_id: "task-1".into(),
                agent_id: "agent-1".into(),
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                result: json!({ "ok": true }),
                is_error: false,
                parent_message_id: None,
                root_input_id: Some("turn-1".into()),
            },
        )
        .into_iter()
        .next()
        .expect("tool stream item"),
    ));

    assert_eq!(app.timeline().tool_calls.len(), 1);
    assert_eq!(app.timeline().tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(app.timeline().tool_call_count(), 1);
}

#[test]
fn committed_tool_result_updates_existing_tool_call() {
    let mut app = live_app();

    app.apply_event(Event::StreamItem(
        piko_protocol::StreamItemPatch::from_tool_execution(
            &piko_protocol::ToolExecutionEvent::Started {
                session_id: "session-1".into(),
                agent_instance_id: "task-1".into(),
                agent_id: "agent-1".into(),
                tool_call_id: "call-1".into(),
                tool_name: "run".into(),
                args: json!({ "cmd": "true" }),
                parent_message_id: None,
                root_input_id: Some("turn-1".into()),
            },
        )
        .into_iter()
        .next()
        .expect("tool stream item"),
    ));
    app.apply_event(Event::StreamItem(
        piko_protocol::StreamItemPatch::from_tool_execution(
            &piko_protocol::ToolExecutionEvent::Ended {
                session_id: "session-1".into(),
                agent_instance_id: "task-1".into(),
                agent_id: "agent-1".into(),
                tool_call_id: "call-1".into(),
                tool_name: "run".into(),
                result: json!({"done": true}),
                is_error: true,
                parent_message_id: None,
                root_input_id: Some("turn-1".into()),
            },
        )
        .into_iter()
        .next()
        .expect("tool stream item"),
    ));

    assert_eq!(app.timeline().tool_calls.len(), 1);
    assert_eq!(app.timeline().tool_calls[0].status, ToolStatus::Failed);
    assert_eq!(
        app.timeline().tool_calls[0].result.as_deref(),
        Some("{\"done\":true}")
    );
}

#[test]
fn assistant_streaming_updates_one_component() {
    let mut app = live_app();

    app.apply_event(realtime(
        "message-1",
        0,
        piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
            role: piko_protocol::MessageRole::Assistant,
        },
    ));
    app.apply_event(realtime(
        "message-1",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "hello".into(),
        },
    ));
    app.apply_event(realtime(
        "message-1",
        2,
        piko_protocol::agent_runtime::RealtimeDelta::Thinking {
            content_index: 0,
            delta: "thought".into(),
        },
    ));
    app.apply_event(realtime(
        "message-1",
        3,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: " world".into(),
        },
    ));

    assert_eq!(
        app.timeline().component_kinds(),
        vec![TimelineKind::Assistant, TimelineKind::Thought]
    );
}

#[test]
fn realtime_gap_requests_authoritative_snapshot() {
    let mut app = live_app();
    app.apply_event(realtime(
        "message-gap",
        1,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "a".into(),
        },
    ));
    let effects = app.apply_event(realtime(
        "message-gap",
        3,
        piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "c".into(),
        },
    ));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::StateSnapshot { session_id, .. })]
            if session_id == "session-1"
    ));
    assert_eq!(
        app.timeline().assistant_text("message-gap").as_deref(),
        Some("a")
    );
}

#[test]
fn replace_and_clear_content_flow_through_canonical_projection() {
    let mut app = live_app();
    for (seq, op, text) in [
        (1, piko_protocol::StreamItemOp::AppendChunk, Some("draft")),
        (
            2,
            piko_protocol::StreamItemOp::ReplaceContent,
            Some("correct"),
        ),
        (3, piko_protocol::StreamItemOp::ClearContent, None),
    ] {
        app.apply_event(Event::StreamItem(piko_protocol::StreamItemPatch {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("task-1".into()),
            item_id: "message-correction".into(),
            item_kind: piko_protocol::StreamItemKind::AgentMessage,
            op,
            text: text.map(str::to_string),
            content_index: Some(0),
            delta_seq: Some(seq),
            fields: Some(serde_json::json!({
                "parentMessageId": "message-correction"
            })),
        }));
    }
    assert_eq!(
        app.timeline()
            .assistant_text("message-correction")
            .as_deref(),
        Some("")
    );
}

#[test]
fn agent_disconnected_preserves_parent_task_relationship() {
    let mut app = live_app();

    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        session_id: "session-1".into(),
        agent_instance_id: "task-main".into(),
        agent_id: "main".into(),
        parent_agent_instance_id: None,
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Running,
        unread_report_count: 0,
        name: "main".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Running,
    }));
    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        session_id: "session-1".into(),
        agent_instance_id: "task-child".into(),
        agent_id: "hello-agent".into(),
        parent_agent_instance_id: Some("task-main".into()),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Running,
        unread_report_count: 0,
        name: "hello-agent".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Running,
    }));
    app.apply_event(Event::AgentChanged(piko_protocol::AgentInfo {
        session_id: "session-1".into(),
        agent_instance_id: "task-child".into(),
        agent_id: "hello-agent".into(),
        parent_agent_instance_id: Some("task-main".into()),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Idle,
        unread_report_count: 0,
        name: "hello-agent".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Completed,
    }));

    let child = app
        .agent_panel
        .agents()
        .iter()
        .find(|agent| agent.agent_instance_id == "task-child")
        .expect("child agent should remain visible");
    assert_eq!(child.parent_agent_instance_id.as_deref(), Some("task-main"));
    assert_eq!(child.status, piko_protocol::AgentStatus::Completed);
}

#[test]
fn agent_subscribe_replaces_timeline_with_agent_replay() {
    let mut app = live_app();
    app.apply_event(committed(
        "root-user",
        1,
        Message::User {
            content: piko_protocol::MessageContent::String("root prompt".into()),
            timestamp: None,
        },
    ));

    app.apply_event(Event::CommandResponse {
        command_id: "subscribe-1".into(),
        result: Ok(piko_protocol::CommandResult::AgentSubscribed {
            session_id: "session-1".into(),
            agent_instance_id: "task-child".into(),
            agent_id: "hello-agent".into(),
            snapshot: piko_protocol::AgentViewSnapshot {
                agent_instance_id: "task-child".into(),
                agent_id: "hello-agent".into(),
                parent_agent_instance_id: Some("task-main".into()),
                status: Some(piko_protocol::AgentStatus::Running),
                next_seq: 3,
                events: vec![
                    piko_protocol::SequencedServerMessage {
                        seq: 1,
                        message: Box::new(Event::StreamItem(
                            piko_protocol::StreamItemPatch::from_realtime_delta(
                                Some("session-1".into()),
                                Some("task-child".into()),
                                "message-child",
                                Some(0),
                                &piko_protocol::agent_runtime::RealtimeDelta::MessageStarted {
                                    role: piko_protocol::MessageRole::Assistant,
                                },
                            )
                            .into_iter()
                            .next()
                            .expect("realtime stream item"),
                        )),
                    },
                    piko_protocol::SequencedServerMessage {
                        seq: 2,
                        message: Box::new(Event::StreamItem(
                            piko_protocol::StreamItemPatch::from_realtime_delta(
                                Some("session-1".into()),
                                Some("task-child".into()),
                                "message-child",
                                Some(1),
                                &piko_protocol::agent_runtime::RealtimeDelta::Text {
                                    content_index: 0,
                                    delta: "Hello".into(),
                                },
                            )
                            .into_iter()
                            .next()
                            .expect("realtime stream item"),
                        )),
                    },
                ],
            },
            replay: Vec::new(),
            next_seq: 3,
        }),
    });

    assert_eq!(
        app.timeline().component_kinds(),
        vec![TimelineKind::Assistant]
    );
    assert_eq!(
        app.agent_panel.active_agent_instance_id.as_deref(),
        Some("task-child")
    );
}
