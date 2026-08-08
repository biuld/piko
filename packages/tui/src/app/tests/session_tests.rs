use super::*;

#[test]
fn session_created_waits_for_reconcile_without_local_refresh_effects() {
    let mut app = app();
    app.session.initializing = true;
    app.agent_panel.begin_loading();
    app.session.pending.track(
        "test".into(),
        crate::app::pending::PendingCommandKind::SessionCreate,
    );

    let effects = app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::SessionCreated {
            session_id: "session-1".into(),
            cwd: "/tmp/piko-test".into(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });

    assert!(app.session.id.is_none());
    assert_eq!(app.session.opening_id.as_deref(), Some("session-1"));
    assert!(app.agent_panel.is_loading());
    assert!(app.session.initializing);
    assert!(effects.is_empty());
}

#[test]
fn pending_first_turn_is_submitted_only_after_reconcile() {
    let mut app = app();
    app.begin_session_hydration(None);
    app.session.pending_turn_text = Some("hello".into());
    app.session.pending.track(
        "create".into(),
        crate::app::pending::PendingCommandKind::SessionCreate,
    );

    let created_effects = app.apply_event(Event::CommandResponse {
        command_id: "create".into(),
        result: Ok(piko_protocol::CommandResult::SessionCreated {
            session_id: "session-1".into(),
            cwd: "/tmp/piko-test".into(),
            timestamp: 0,
        }),
    });
    assert!(created_effects.is_empty());
    assert_eq!(app.session.pending_turn_text.as_deref(), Some("hello"));

    let reconcile_effects = app.apply_event(empty_reconcile("session-1"));
    assert!(matches!(
        reconcile_effects.as_slice(),
        [Effect::Send(piko_protocol::Command::ChatSubmit { session_id, text, .. })]
            if session_id == "session-1" && text == "hello"
    ));
}

#[test]
fn agent_list_cannot_complete_session_hydration() {
    let mut app = app();
    app.begin_session_hydration(Some("session-1".into()));

    app.apply_event(Event::CommandResponse {
        command_id: "agents".into(),
        result: Ok(piko_protocol::CommandResult::AgentListed {
            session_id: "session-1".into(),
            agents: Vec::new(),
            timestamp: 0,
        }),
    });

    assert!(app.session.initializing);
    assert!(app.agent_panel.is_loading());
    assert!(app.session.id.is_none());
}

#[test]
fn failed_initial_open_clears_loading_and_restores_pending_text() {
    let mut app = app();
    app.session.shell_ready = true;
    app.begin_session_hydration(Some("missing".into()));
    app.session.pending_turn_text = Some("keep me".into());
    app.session.pending.track(
        "open".into(),
        crate::app::pending::PendingCommandKind::SessionOpen,
    );

    let effects = app.handle_host_line(crate::host::HostLine::Message(Box::new(
        Event::CommandResponse {
            command_id: "open".into(),
            result: Err("not found".into()),
        },
    )));

    assert!(effects.is_empty());
    assert!(!app.session.initializing);
    assert!(!app.agent_panel.is_loading());
    assert_eq!(app.editor.text(), "keep me");
}

#[test]
fn cold_start_shows_loading_until_required_bootstrap_completes() {
    let mut app = app();
    assert!(!app.session.initializing);
    assert!(app.agent_panel.is_loading());
    let effects = app.bootstrap();

    let mut tui_config_id = None;
    let mut host_config_id = None;
    let mut catalog_id = None;
    for effect in &effects {
        match effect {
            Effect::Send(piko_protocol::Command::ConfigGet {
                command_id,
                namespace,
            }) if namespace == "tui" => tui_config_id = Some(command_id.clone()),
            Effect::Send(piko_protocol::Command::ConfigGet {
                command_id,
                namespace,
            }) if namespace == "host" => host_config_id = Some(command_id.clone()),
            Effect::Send(piko_protocol::Command::CommandCatalogGet { command_id }) => {
                catalog_id = Some(command_id.clone());
            }
            _ => {}
        }
    }
    let tui_config_id = tui_config_id.expect("bootstrap ConfigGet tui");
    let host_config_id = host_config_id.expect("bootstrap ConfigGet host");
    let catalog_id = catalog_id.expect("bootstrap CommandCatalogGet");

    app.apply_event(Event::CommandResponse {
        command_id: tui_config_id,
        result: Ok(piko_protocol::CommandResult::ConfigEntry {
            namespace: "tui".into(),
            value: serde_json::json!({}),
        }),
    });
    assert!(app.agent_panel.is_loading());
    app.apply_event(Event::CommandResponse {
        command_id: host_config_id,
        result: Ok(piko_protocol::CommandResult::ConfigEntry {
            namespace: "host".into(),
            value: serde_json::json!({}),
        }),
    });
    assert!(app.agent_panel.is_loading());
    app.apply_event(Event::CommandResponse {
        command_id: catalog_id,
        result: Ok(piko_protocol::CommandResult::CommandCatalogListed {
            commands: Vec::new(),
            timestamp: 0,
        }),
    });
    assert!(!app.agent_panel.is_loading());
    assert!(app.agent_panel.agents().is_empty());
    assert!(app.host_settings.loaded);
}

#[test]
fn open_or_continue_boot_starts_agent_panel_loading() {
    let open = AppState::new(
        PathBuf::from("/tmp/piko-test"),
        Some("session-1".into()),
        false,
        InitialOptions::default(),
    );
    assert!(open.session.initializing);
    assert!(open.agent_panel.is_loading());

    let cont = AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        true,
        InitialOptions::default(),
    );
    assert!(cont.session.initializing);
    assert!(cont.agent_panel.is_loading());
}

#[test]
fn session_reconciled_marks_agents_hydrated_with_host_names() {
    let mut app = app();
    app.session.opening_id = Some("session-1".into());
    app.agent_panel.begin_loading();
    assert!(app.agent_panel.is_loading());

    app.apply_event(Event::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id: "session-1".into(),
            reason: piko_protocol::ReconcileReason::InitialHydration,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "hostd:session-1".into(),
                seq: 0,
            },
            snapshot: piko_protocol::SessionSnapshot {
                session_id: "session-1".into(),
                cwd: "/tmp/piko-test".into(),
                seq: 0,
                entries: Vec::new(),
                current_leaf_id: None,
                selected_agent_instance_id: None,
                active_turns: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: None,
            },
            agents: vec![piko_protocol::AgentInfo {
                session_id: "session-1".into(),
                agent_instance_id: "task-main".into(),
                agent_id: "main".into(),
                parent_agent_instance_id: None,
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count: 0,
                name: "Main".into(),
                role: "root".into(),
                status: piko_protocol::AgentStatus::Idle,
            }],
        },
    ));

    assert!(!app.agent_panel.is_loading());
    assert!(!app.session.initializing);
    assert_eq!(app.agent_panel.agents().len(), 1);
    assert_eq!(app.agent_panel.agents()[0].name, "Main");
    assert_eq!(app.agent_panel.agents()[0].agent_id, "main");
}

#[test]
fn session_opened_keeps_agent_panel_loading_until_reconcile() {
    let mut app = app();
    app.session.initializing = true;
    app.agent_panel.begin_loading();
    app.session.pending.track(
        "test".into(),
        crate::app::pending::PendingCommandKind::SessionOpen,
    );

    app.apply_event(Event::CommandResponse {
        result: Ok(piko_protocol::CommandResult::SessionOpened {
            session_id: "session-1".into(),
            timestamp: 0,
        }),
        command_id: "test".into(),
    });

    assert!(app.session.id.is_none());
    assert_eq!(app.session.opening_id.as_deref(), Some("session-1"));
    assert!(app.agent_panel.is_loading());
    assert!(app.session.initializing);
}

#[test]
fn stale_open_response_cannot_replace_latest_target() {
    let mut app = app();
    app.begin_session_hydration(Some("session-new".into()));
    app.session.pending.track(
        "open-old".into(),
        crate::app::pending::PendingCommandKind::SessionOpen,
    );

    app.apply_event(Event::CommandResponse {
        command_id: "open-old".into(),
        result: Ok(piko_protocol::CommandResult::SessionOpened {
            session_id: "session-old".into(),
            timestamp: 0,
        }),
    });

    assert_eq!(app.session.opening_id.as_deref(), Some("session-new"));
    assert!(app.session.initializing);
}
