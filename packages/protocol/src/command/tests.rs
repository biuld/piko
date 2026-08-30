use super::*;

#[test]
fn canonical_agent_input_commands_round_trip() {
    let input = crate::AgentInput {
        input_id: "input-1".into(),
        request_id: "request-1".into(),
        session_id: "s1".into(),
        agent_instance_id: "a1".into(),
        origin: crate::AgentInputOrigin::User,
        delivery: crate::AgentInputDelivery::FollowUp,
        content: crate::MessageContent::String("hello".into()),
        submitted_at: 42,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    };
    let submit = Command::AgentInputSubmit {
        command_id: "command-1".into(),
        input: input.clone(),
    };
    let value = serde_json::to_value(&submit).unwrap();
    assert_eq!(value["type"], "agent_input_submit");
    assert_eq!(serde_json::from_value::<Command>(value).unwrap(), submit);

    let cancel = Command::AgentInputCancel {
        command_id: "command-2".into(),
        session_id: input.session_id,
        agent_instance_id: input.agent_instance_id,
        input_id: input.input_id,
    };
    let value = serde_json::to_value(&cancel).unwrap();
    assert_eq!(value["type"], "agent_input_cancel");
    assert_eq!(serde_json::from_value::<Command>(value).unwrap(), cancel);
}

#[test]
fn follow_up_and_steer_helpers_build_agent_input_submit() {
    let follow_up = Command::submit_follow_up(
        "c1",
        "s1",
        "a1",
        crate::MessageContent::String("hello".into()),
    );
    let Command::AgentInputSubmit { input, .. } = follow_up else {
        panic!("expected AgentInputSubmit");
    };
    assert_eq!(input.session_id, "s1");
    assert_eq!(input.agent_instance_id, "a1");
    assert_eq!(input.delivery, crate::AgentInputDelivery::FollowUp);
    assert_eq!(input.content, crate::MessageContent::String("hello".into()));

    let steer = Command::submit_steer(
        "c2",
        "s1",
        "a1",
        crate::MessageContent::Blocks(vec![crate::ContentBlock::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }]),
    );
    let Command::AgentInputSubmit { input, .. } = steer else {
        panic!("expected AgentInputSubmit");
    };
    assert_eq!(input.delivery, crate::AgentInputDelivery::SteerActive);
    assert!(matches!(
        input.content,
        crate::MessageContent::Blocks(blocks)
            if matches!(blocks.as_slice(), [crate::ContentBlock::Image { data, .. }] if data == "AA==")
    ));
}

#[test]
fn agent_interrupt_round_trips() {
    let command = Command::AgentInterrupt {
        command_id: "interrupt-1".into(),
        session_id: "session-1".into(),
        agent_instance_id: "agent-child".into(),
    };
    let value = serde_json::to_value(&command).unwrap();
    assert_eq!(value["type"], "agent_interrupt");
    assert_eq!(value["agent_instance_id"], "agent-child");
    assert_eq!(serde_json::from_value::<Command>(value).unwrap(), command);

    let result = crate::CommandResult::AgentInterrupted {
        session_id: "session-1".into(),
        agent_instance_id: "agent-child".into(),
        accepted: false,
        timestamp: 42,
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["type"], "agent_interrupted");
    assert_eq!(value["accepted"], false);
}

#[test]
fn oauth_login_defaults_to_browser_and_cancel_round_trips() {
    let parsed: Command = serde_json::from_value(serde_json::json!({
        "type": "auth_login_o_auth",
        "command_id": "login-1",
        "provider": "openai"
    }))
    .unwrap();
    assert!(matches!(
        parsed,
        Command::AuthLoginOAuth {
            mode: OAuthLoginMode::Browser,
            ..
        }
    ));

    let cancel = Command::AuthCancelOAuth {
        command_id: "cancel-1".into(),
        provider: "openai".into(),
    };
    let value = serde_json::to_value(cancel).unwrap();
    assert_eq!(value["type"], "auth_cancel_o_auth");
    assert_eq!(value["provider"], "openai");
}

#[test]
fn session_compact_without_mode_defaults_to_summarize() {
    // Older clients omit `mode`; the wire must stay compatible.
    let parsed: Command = serde_json::from_value(serde_json::json!({
        "type": "session_compact",
        "command_id": "c1",
        "session_id": "s1",
        "agent_instance_id": "agent_s1_root",
    }))
    .unwrap();
    assert!(matches!(
        parsed,
        Command::SessionCompact {
            mode: CompactMode::Summarize,
            ..
        }
    ));

    let with_mode: Command = serde_json::from_value(serde_json::json!({
        "type": "session_compact",
        "command_id": "c2",
        "session_id": "s1",
        "agent_instance_id": "agent_s1_root",
        "mode": "new-context-window",
    }))
    .unwrap();
    assert!(matches!(
        with_mode,
        Command::SessionCompact {
            mode: CompactMode::NewContextWindow,
            ..
        }
    ));
}

#[test]
fn rollout_page_get_round_trips() {
    let value = serde_json::json!({
        "type": "rollout_page_get",
        "command_id": "c-page",
        "session_id": "s1",
        "agent_instance_id": "a1",
        "after_cursor": "seq:10",
        "limit": 25
    });
    let command: Command = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        &command,
        Command::RolloutPageGet {
            after_cursor: Some(cursor),
            limit: Some(25),
            ..
        } if cursor == "seq:10"
    ));
    assert_eq!(serde_json::to_value(command).unwrap(), value);
}

#[test]
fn agent_work_diff_get_round_trips() {
    let value = serde_json::json!({
        "type": "agent_work_diff_get",
        "command_id": "c-diff",
        "session_id": "s1",
        "root_input_id": "input-1"
    });
    let command: Command = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        &command,
        Command::AgentWorkDiffGet { session_id, root_input_id, .. }
            if session_id == "s1" && root_input_id == "input-1"
    ));
    assert_eq!(serde_json::to_value(command).unwrap(), value);
}

#[test]
fn process_list_and_info_round_trip() {
    let command: Command =
        serde_json::from_value(serde_json::json!({ "type": "process_list", "command_id": "c1" }))
            .unwrap();
    assert!(matches!(command, Command::ProcessList { .. }));

    let info = ProcessInfo {
        process_id: "proc-1".into(),
        pid: 42,
        command: "cargo test -p tui".into(),
        cwd: "/tmp/proj".into(),
        exited: false,
        exit_code: None,
        signal: None,
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["processId"], "proc-1");
    assert_eq!(json["pid"], 42);
    assert!(json.get("exitCode").is_none());
    let back: ProcessInfo = serde_json::from_value(json).unwrap();
    assert_eq!(back, info);
}

#[test]
fn process_stop_and_exit_round_trip() {
    let command: Command = serde_json::from_value(serde_json::json!({
        "type": "process_stop",
        "command_id": "c1",
        "process_id": "proc-3",
    }))
    .unwrap();
    assert!(matches!(command, Command::ProcessStop { process_id, .. } if process_id == "proc-3"));

    let exit = ProcessExit {
        exit_code: None,
        signal: Some(15),
    };
    let json = serde_json::to_value(exit).unwrap();
    assert_eq!(json["signal"], 15);
    assert!(json.get("exitCode").is_none());
    assert_eq!(serde_json::from_value::<ProcessExit>(json).unwrap(), exit);
}

#[test]
fn mcp_status_and_server_info_round_trip() {
    let command: Command =
        serde_json::from_value(serde_json::json!({ "type": "mcp_status", "command_id": "c1" }))
            .unwrap();
    assert!(matches!(command, Command::McpStatus { .. }));

    let info = McpServerInfo {
        name: "filesystem".into(),
        connected: true,
        tool_count: 3,
        resource_count: 1,
        template_count: 0,
        error: None,
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["name"], "filesystem");
    assert_eq!(json["toolCount"], 3);
    assert!(json.get("error").is_none());
    let back: McpServerInfo = serde_json::from_value(json).unwrap();
    assert_eq!(back, info);

    let failed = McpServerInfo {
        name: "hang".into(),
        connected: false,
        tool_count: 0,
        resource_count: 0,
        template_count: 0,
        error: Some("timed out after 10000 ms".into()),
    };
    let json = serde_json::to_value(&failed).unwrap();
    assert_eq!(json["connected"], false);
    assert!(json["error"].is_string());
}
