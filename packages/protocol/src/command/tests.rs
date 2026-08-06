use super::*;

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
fn prompt_debug_get_round_trips() {
    let value = serde_json::json!({
        "type": "prompt_debug_get",
        "command_id": "c-debug",
        "session_id": "s1",
        "agent_instance_id": "a1"
    });
    let command: Command = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        &command,
        Command::PromptDebugGet {
            command_id,
            session_id,
            agent_instance_id,
        } if command_id == "c-debug" && session_id == "s1" && agent_instance_id == "a1"
    ));
    assert_eq!(serde_json::to_value(command).unwrap(), value);
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
fn turn_diff_get_round_trips() {
    let value = serde_json::json!({
        "type": "turn_diff_get",
        "command_id": "c-diff",
        "session_id": "s1",
        "turn_id": "t1"
    });
    let command: Command = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        &command,
        Command::TurnDiffGet { session_id, turn_id, .. }
            if session_id == "s1" && turn_id == "t1"
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
