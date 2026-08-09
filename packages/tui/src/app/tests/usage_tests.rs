use super::*;

#[test]
fn session_reconcile_projects_cumulative_usage() {
    let mut app = app();
    app.session.opening_id = Some("session-1".into());
    app.session.initializing = true;

    let mut usage = piko_protocol::messages::Usage::empty();
    usage.input = 10_000;
    usage.output = 2_000;
    usage.total_tokens = 12_000;
    usage.cost.total = 0.42;

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
                selected_agent_instance_id: Some("agent_session-1_root".into()),
                active_turns: Vec::new(),
                pending_approvals: Vec::new(),
                pending_interactions: Vec::new(),
                name: None,
                cumulative_usage: Some(usage.clone()),
            },
            agents: vec![piko_protocol::AgentInfo {
                session_id: "session-1".into(),
                agent_instance_id: "agent_session-1_root".into(),
                agent_id: "main".into(),
                parent_agent_instance_id: None,
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count: 0,
                name: "Main".into(),
                role: "assistant".into(),
                status: piko_protocol::AgentStatus::Idle,
            }],
        },
    ));

    assert_eq!(
        app.session.cumulative_usage.as_ref().map(|u| u.cost.total),
        Some(0.42)
    );
    assert_eq!(
        app.session
            .cumulative_usage
            .as_ref()
            .map(|u| u.total_tokens),
        Some(12_000)
    );
}

#[test]
fn usage_event_sets_chrome_and_clears_active_turn() {
    let mut app = live_app();
    app.session.active_turns.insert(
        "agent-1".into(),
        crate::app::ActiveTurnUi {
            turn_id: "turn-1".into(),
            status: piko_protocol::TurnStatus::Running,
        },
    );

    let mut turn_usage = piko_protocol::messages::Usage::empty();
    turn_usage.input = 12_200;
    turn_usage.cache_read = 800;
    turn_usage.output = 400;
    turn_usage.total_tokens = 13_400;
    turn_usage.cost.total = 0.05;

    // Turn terminal no longer rolls usage into chrome.
    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Completed {
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        agent_instance_id: "agent-1".into(),
        usage: turn_usage.clone(),
        timestamp: 0,
    }));
    assert!(app.session.last_context_tokens.is_none());
    assert!(app.session.cumulative_usage.is_none());
    assert!(app.session.active_turns.is_empty());

    app.apply_event(Event::Usage(piko_protocol::UsageEvent::Updated {
        session_id: "session-1".into(),
        agent_instance_id: Some("agent-1".into()),
        turn_id: Some("turn-1".into()),
        used: 13_000,
        size: Some(128_000),
        cumulative: Some(turn_usage),
        turn_usage: None,
        timestamp: 0,
    }));

    assert_eq!(app.session.last_context_tokens, Some(13_000));
    assert_eq!(
        app.session.cumulative_usage.as_ref().map(|u| u.cost.total),
        Some(0.05)
    );
}

#[test]
fn model_catalog_resolves_context_window() {
    let mut app = app();
    app.model.active_provider = Some("openai".into());
    app.model.active_model_id = Some("gpt-4o".into());
    app.model.providers = vec![piko_protocol::ProviderInfo {
        provider: "openai".into(),
        has_auth: true,
        auth_methods: vec![piko_protocol::model::ProviderAuthMethod::ApiKey],
        models: vec![piko_protocol::ModelSummary {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            reasoning: false,
            input: vec![],
            context_window: 128_000,
            max_tokens: 16_384,
            thinking_level_map: None,
        }],
    }];

    assert_eq!(app.model.active_context_window(), Some(128_000));
}

#[test]
fn clear_session_view_clears_usage() {
    let mut app = live_app();
    app.session.cumulative_usage = Some(piko_protocol::messages::Usage::empty());
    app.session.last_context_tokens = Some(1000);
    app.clear_session_view();
    assert!(app.session.cumulative_usage.is_none());
    assert!(app.session.last_context_tokens.is_none());
}
