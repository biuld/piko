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
fn browser_auth_event_opens_url_and_keeps_fallback_notice() {
    let mut app = app();
    let effects = app.apply_event(Event::Auth(piko_protocol::AuthEvent::LoginBrowser {
        login_id: "login-1".into(),
        provider: "openai".into(),
        authorization_url: "https://auth.example/login".into(),
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenUrl(url)] if url == "https://auth.example/login"
    ));
    assert!(
        app.notifications
            .row_visible_for(std::time::Instant::now(), None, None)
            .is_some_and(|notice| notice.message.contains("https://auth.example/login"))
    );
}

#[test]
fn local_slash_commands_exist_before_host_catalog_arrives() {
    let app = app();
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/diff")
    );
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/resume")
    );
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/noti")
    );
}

#[test]
fn noti_opens_the_notifications_modal() {
    let mut app = app();

    let effects = app.try_slash_command("/noti").expect("known slash");

    assert!(effects.is_empty());
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Notifications)
    );
}

#[test]
fn top_slash_command_sends_process_list() {
    let mut app = live_app();
    // `/top` is slash-addressable once hostd advertises `process.list`.
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
    let effects = app.try_slash_command("/top").expect("known slash");
    assert!(
        effects
            .iter()
            .any(|e| { matches!(e, Effect::Send(piko_protocol::Command::ProcessList { .. })) })
    );
}

#[test]
fn process_listed_event_opens_structured_panel() {
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
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Processes)
    );
    assert!(app.notifications.items().is_empty());

    // Empty list keeps the structured panel open with an empty-state message.
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
fn command_catalog_retains_invoke_and_hides_unsupported_aliases() {
    let entries = crate::app::command::merge_command_catalog(&[
        HostCommandDescriptor {
            id: "session.rename".into(),
            title: "Rename session".into(),
            detail: "Set a new name".into(),
            invoke: HostCommandInvoke::Args { schema: Vec::new() },
            group: Some(HostCommandGroup::Session),
        },
        HostCommandDescriptor {
            id: "session.export".into(),
            title: "Export session".into(),
            detail: "Get its path".into(),
            invoke: HostCommandInvoke::Immediate,
            group: Some(HostCommandGroup::Session),
        },
        HostCommandDescriptor {
            id: "session.clone".into(),
            title: "Clone session".into(),
            detail: "Clone the current session".into(),
            invoke: HostCommandInvoke::Immediate,
            group: Some(HostCommandGroup::Session),
        },
        HostCommandDescriptor {
            id: "auth.login-device".into(),
            title: "Sign in with device code".into(),
            detail: "Start headless OAuth login".into(),
            invoke: HostCommandInvoke::Args { schema: Vec::new() },
            group: Some(HostCommandGroup::Auth),
        },
        HostCommandDescriptor {
            id: "auth.cancel-login".into(),
            title: "Cancel sign in".into(),
            detail: "Cancel active OAuth login".into(),
            invoke: HostCommandInvoke::Args { schema: Vec::new() },
            group: Some(HostCommandGroup::Auth),
        },
    ]);
    let rename = entries
        .iter()
        .find(|entry| entry.slash == "/rename")
        .unwrap();
    assert!(matches!(rename.invoke, HostCommandInvoke::Args { .. }));
    assert!(!entries.iter().any(|entry| entry.slash == "/clear"));
    assert!(!entries.iter().any(|entry| entry.slash == "/export"));
    assert!(!entries.iter().any(|entry| entry.slash == "/clone"));
    assert!(!entries.iter().any(|entry| entry.slash == "/login-device"));
    assert!(!entries.iter().any(|entry| entry.slash == "/login-cancel"));
}

#[test]
fn removed_compatibility_alias_is_not_a_command() {
    let mut app = live_app();
    assert!(app.try_slash_command("/clear").is_none());
}

#[test]
fn fork_without_argument_opens_tree_picker() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "session.fork".into(),
                title: "Fork session".into(),
                detail: "Fork from a tree entry".into(),
                invoke: HostCommandInvoke::Immediate,
                group: Some(HostCommandGroup::Session),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    let effects = app.try_slash_command("/fork").unwrap();
    assert!(effects.is_empty());
    assert!(app.tree_fork_mode);
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Tree)
    );
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
    assert_eq!(app.status, "1 MCP server(s) connected");
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Mcp)
    );
    assert!(app.notifications.items().is_empty());
}

#[test]
fn top_process_stop_requires_two_confirms() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::ProcessListed {
            processes: vec![piko_protocol::command::ProcessInfo {
                process_id: "proc-7".into(),
                pid: 7,
                command: "cargo test".into(),
                cwd: "/tmp/project".into(),
                exited: false,
                exit_code: None,
                signal: None,
            }],
            timestamp: 0,
        }),
        command_id: "top".into(),
    });
    let first = app.dispatch(crate::app::command::SurfaceAction::Confirm.into());
    assert!(first.is_empty());
    assert_eq!(app.status, "confirm stopping proc-7");

    let second = app.dispatch(crate::app::command::SurfaceAction::Confirm.into());
    assert!(second.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::ProcessStop {
                process_id,
                ..
            }) if process_id == "proc-7"
        )
    }));
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Processes)
    );
}

#[test]
fn ps_and_kill_are_not_visible_slash_commands() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![
                HostCommandDescriptor {
                    id: "process.list".into(),
                    title: "List processes".into(),
                    detail: "Show running processes".into(),
                    invoke: HostCommandInvoke::Immediate,
                    group: Some(HostCommandGroup::Runtime),
                },
                HostCommandDescriptor {
                    id: "process.stop".into(),
                    title: "Stop process".into(),
                    detail: "Terminate a running process".into(),
                    invoke: HostCommandInvoke::Args { schema: Vec::new() },
                    group: Some(HostCommandGroup::Runtime),
                },
            ],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/top")
    );
    assert!(!app.command_catalog.iter().any(|entry| entry.slash == "/ps"));
    assert!(
        !app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/kill")
    );
}

#[test]
fn missing_slash_arguments_preserve_the_command_for_editing() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "session.rename".into(),
                title: "Rename session".into(),
                detail: "Set a new name".into(),
                invoke: HostCommandInvoke::Args { schema: Vec::new() },
                group: Some(HostCommandGroup::Session),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    app.editor.restore_text("/rename");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(effects.is_empty());
    assert_eq!(app.editor.text(), "/rename");
    assert_eq!(app.status, "usage: /rename <session name>");
}

#[test]
fn logout_without_known_provider_does_not_guess_one() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: vec![HostCommandDescriptor {
                id: "auth.logout".into(),
                title: "Log out".into(),
                detail: "Remove provider credentials".into(),
                invoke: HostCommandInvoke::Args { schema: Vec::new() },
                group: Some(HostCommandGroup::Auth),
            }],
            timestamp: 0,
        }),
        command_id: "catalog".into(),
    });
    app.model.active_provider = None;
    app.editor.restore_text("/logout");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(effects.is_empty());
    assert_eq!(app.editor.text(), "/logout");
    assert_eq!(app.status, "usage: /logout <provider>");
}

#[test]
fn direct_command_errors_are_visible() {
    let mut app = app();
    app.apply_event(Event::CommandResponse {
        command_id: "command".into(),
        result: Err("failed visibly".into()),
    });
    assert_eq!(app.status, "failed visibly");
    assert!(
        app.notifications
            .has_row_visible_for(app.last_tick, None, None)
    );
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
