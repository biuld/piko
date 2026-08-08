use super::*;

#[test]
fn test_unknown_slash_command_blocks_submit() {
    let mut app = app();
    app.editor.insert_char('/');
    app.editor.insert_char('u');
    app.editor.insert_char('n');
    app.editor.insert_char('k');
    app.editor.insert_char('n');
    app.editor.insert_char('o');
    app.editor.insert_char('w');
    app.editor.insert_char('n');

    app.dispatch(EditorAction::Submit.into());

    // Because it's an unknown slash command, it should block submission,
    // so the editor should NOT be cleared (normal submits clear the editor).
    assert_eq!(app.editor.text(), "/unknown");
    assert!(app.status.contains("Unknown slash command"));
}

#[test]
fn ps_slash_command_sends_process_list() {
    let mut app = live_app();
    // `/ps` is slash-addressable once hostd advertises `process.list`.
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "process.list".into(),
                title: "List processes".into(),
                detail: "Show currently running external processes".into(),
                invoke: HostCommandInvoke::Immediate,
                group: Some(HostCommandGroup::Runtime),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    let effects = app.try_slash_command("/ps").expect("known slash");
    assert!(
        effects
            .iter()
            .any(|e| { matches!(e, Effect::Send(piko_protocol::Command::ProcessList { .. })) })
    );
}

#[test]
fn process_listed_event_renders_notification() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::ProcessListed {
            processes: vec![piko_protocol::command::ProcessInfo {
                process_id: "proc-1".into(),
                pid: 42,
                command: "cargo test -p tui".into(),
                cwd: "/tmp/proj".into(),
                exited: false,
                exit_code: None,
                signal: None,
            }],
            timestamp: 0,
        }),
        command_id: "ps".into(),
    });
    assert!(app.status.contains("process(es) running"));
    let latest = app.notifications.items().back().expect("notification");
    assert!(latest.message.contains("proc-1"));
    assert!(latest.message.contains("cargo test -p tui"));

    // Empty list renders a distinct info notification.
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::ProcessListed {
            processes: Vec::new(),
            timestamp: 0,
        }),
        command_id: "ps".into(),
    });
    assert_eq!(app.status, "no processes running");
}

#[test]
fn mcp_slash_command_sends_mcp_status() {
    let mut app = live_app();
    // `/mcp` is slash-addressable once hostd advertises `mcp.status`.
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "mcp.status".into(),
                title: "MCP servers".into(),
                detail: "Show connected MCP servers".into(),
                invoke: HostCommandInvoke::Immediate,
                group: Some(HostCommandGroup::Runtime),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    let effects = app.try_slash_command("/mcp").expect("known slash");
    assert!(
        effects
            .iter()
            .any(|e| { matches!(e, Effect::Send(piko_protocol::Command::McpStatus { .. })) })
    );
}

#[test]
fn mcp_status_listed_event_opens_panel() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::McpStatusListed {
            servers: vec![
                piko_protocol::command::McpServerInfo {
                    name: "filesystem".into(),
                    connected: true,
                    tool_count: 3,
                    resource_count: 1,
                    template_count: 2,
                    error: None,
                },
                piko_protocol::command::McpServerInfo {
                    name: "hang".into(),
                    connected: false,
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                    error: Some("timed out after 10000 ms".into()),
                },
            ],
            timestamp: 0,
        }),
        command_id: "mcp".into(),
    });
    assert_eq!(app.mcp.connected_count(), 1);
    assert_eq!(app.mcp.servers().len(), 2);
    assert_eq!(app.status, "1 MCP server(s) connected");
    assert_eq!(app.focus_manager.active_mode(), AppMode::Mcp);
    let latest = app.notifications.items().back().expect("notification");
    assert!(latest.message.contains("filesystem"));
}

#[test]
fn kill_slash_command_sends_process_stop() {
    let mut app = live_app();
    // `/kill` is slash-addressable once hostd advertises `process.stop`.
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "process.stop".into(),
                title: "Stop process".into(),
                detail: "Terminate a running external process".into(),
                invoke: HostCommandInvoke::Args { schema: Vec::new() },
                group: Some(HostCommandGroup::Runtime),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    let effects = app.try_slash_command("/kill proc-7").expect("known slash");
    assert!(effects.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::ProcessStop {
                process_id,
                ..
            }) if process_id == "proc-7"
        )
    }));

    // Missing id shows usage instead of sending.
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "process.stop".into(),
                title: "Stop process".into(),
                detail: "Terminate a running external process".into(),
                invoke: HostCommandInvoke::Args { schema: Vec::new() },
                group: Some(HostCommandGroup::Runtime),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    let effects = app.try_slash_command("/kill").expect("known slash");
    assert!(effects.is_empty());
    assert!(app.status.contains("usage: /kill"));
}

#[test]
fn process_stopped_event_renders_notification() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::ProcessStopped {
            process_id: "proc-1".into(),
            stopped: true,
            exit_code: Some(0),
            signal: None,
            timestamp: 0,
        }),
        command_id: "kill".into(),
    });
    let latest = app.notifications.items().back().expect("notification");
    assert!(latest.message.contains("stopped proc-1"));
    assert!(latest.message.contains("exit 0"));

    // Unknown process renders a warning.
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::ProcessStopped {
            process_id: "proc-99".into(),
            stopped: false,
            exit_code: None,
            signal: None,
            timestamp: 0,
        }),
        command_id: "kill".into(),
    });
    let latest = app.notifications.items().back().expect("notification");
    assert!(latest.message.contains("no such process: proc-99"));
    assert!(app.status.contains("no such process: proc-99"));
}
