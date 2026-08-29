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
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/usage")
    );
    assert!(
        app.command_catalog
            .iter()
            .any(|entry| entry.slash == "/todo")
    );
    assert!(
        app.command_catalog
            .iter()
            .all(|entry| entry.slash != "/status")
    );
}

#[test]
fn todo_opens_centered_overlay_without_host_effects() {
    let mut app = app();

    let effects = app.try_slash_command("/todo").expect("known slash");

    assert!(effects.is_empty());
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Todos)
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
fn usage_opens_modal_and_refreshes_host_snapshot() {
    let mut app = live_app();

    let effects = app.try_slash_command("/usage").expect("known slash");

    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Usage)
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::StateSnapshot { session_id, .. })]
            if session_id == "session-1"
    ));
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
    assert!(entries.iter().any(|entry| entry.slash == "/model"));
    assert!(!entries.iter().any(|entry| entry.slash == "/models"));
    assert!(!entries.iter().any(|entry| entry.slash == "/thinking"));
}

#[test]
fn removed_compatibility_alias_is_not_a_command() {
    let mut app = live_app();
    assert!(app.try_slash_command("/clear").is_none());
}

#[test]
fn model_selection_drills_into_only_supported_thinking_levels() {
    use piko_protocol::model::ThinkingLevel;

    let mut app = live_app();
    app.models
        .load(vec![crate::features::model_selector::ModelOption {
            provider: "deepseek".into(),
            id: "deepseek-v4-flash@platform".into(),
            name: "DeepSeek V4 Flash".into(),
            has_auth: true,
            reasoning_efforts: vec![ThinkingLevel::Off, ThinkingLevel::High],
        }]);
    app.push_surface(SurfaceId::Models);

    let effects = app.dispatch(crate::app::command::SurfaceAction::Confirm.into());

    assert!(effects.is_empty());
    assert_eq!(app.mode(), AppMode::Surface(SurfaceId::Thinking));
    assert_eq!(app.thinking.list.len(), 2);
    assert_eq!(app.thinking.confirm(), Some(ThinkingLevel::Off));
}

#[test]
fn model_and_thinking_are_applied_in_one_config_patch() {
    use piko_protocol::{Command, model::ThinkingLevel};

    let mut app = live_app();
    app.models
        .load(vec![crate::features::model_selector::ModelOption {
            provider: "deepseek".into(),
            id: "deepseek-v4-flash@platform".into(),
            name: "DeepSeek V4 Flash".into(),
            has_auth: true,
            reasoning_efforts: vec![ThinkingLevel::High],
        }]);
    app.push_surface(SurfaceId::Models);
    app.dispatch(crate::app::command::SurfaceAction::Confirm.into());

    let effects = app.dispatch(crate::app::command::SurfaceAction::Confirm.into());

    let [Effect::Send(Command::ConfigUpdate { patch, .. })] = effects.as_slice() else {
        panic!("expected one atomic config update")
    };
    assert_eq!(patch["default-provider"], "deepseek");
    assert_eq!(patch["default-model"], "deepseek-v4-flash@platform");
    assert_eq!(patch["default-thinking-level"], "high");
    assert_eq!(app.mode(), AppMode::Chat);
}
