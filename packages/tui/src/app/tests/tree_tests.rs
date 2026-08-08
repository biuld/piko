use super::*;

#[test]
fn tree_summary_prompt_does_not_trigger_when_selected_user_targets_current_leaf() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("current", Some("root"), "current"),
        user_tree_entry("future-branch-user", Some("current"), "future branch"),
    ];
    app.tree.load(&entries, Some("current"));

    assert!(!app.tree_navigation_needs_summary("future-branch-user"));
}

#[test]
fn tree_summary_prompt_triggers_when_selected_user_targets_sibling_branch_parent() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("fork", Some("root"), "fork"),
        user_tree_entry("active-leaf", Some("fork"), "active"),
        user_tree_entry("sibling-user", Some("fork"), "sibling"),
    ];
    app.tree.load(&entries, Some("active-leaf"));

    assert!(app.tree_navigation_needs_summary("sibling-user"));
}

#[test]
fn tree_summary_prompt_triggers_when_root_user_abandons_current_branch() {
    let mut app = app();
    let entries = vec![
        user_tree_entry("root", None, "root"),
        user_tree_entry("active-leaf", Some("root"), "active"),
    ];
    app.tree.load(&entries, Some("active-leaf"));

    assert!(app.tree_navigation_needs_summary("root"));
}

#[test]
fn submit_without_session_returns_session_create_effect() {
    let mut app = app();
    app.editor.restore_text("hello");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(app.session.initializing);
    assert_eq!(app.session.pending_turn_text.as_deref(), Some("hello"));
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::SessionCreate { cwd, .. })
            if cwd == "/tmp/piko-test"
    ));
}

#[test]
fn submit_with_session_waits_for_server_committed_user_message() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-root".into());
    app.editor.restore_text("hello");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::ChatSubmit { text, .. }) if text == "hello"
    ));
    assert!(app.timeline.message_ids().is_empty());
}

#[test]
fn submit_targets_the_viewed_child_agent() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel
        .agents
        .push(crate::features::agent_status::AgentEntry {
            agent_id: "coder".into(),
            agent_instance_id: "agent-child".into(),
            name: "Coder".into(),
            parent_agent_instance_id: Some("agent-root".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        });
    app.agent_panel.active_agent_instance_id = Some("agent-child".into());
    app.editor.restore_text("follow up");

    let effects = app.dispatch(EditorAction::Submit.into());

    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::ChatSubmit {
            session_id,
            target_agent_instance_id: agent_instance_id,
            text,
            ..
        }) if session_id == "session-1"
            && agent_instance_id == "agent-child"
            && text == "follow up"
    ));
}

#[test]
fn agent_run_lifecycle_does_not_synthesize_agent_activity() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel
        .agents
        .push(crate::features::agent_status::AgentEntry {
            agent_id: "coder".into(),
            agent_instance_id: "agent-child".into(),
            name: "Coder".into(),
            parent_agent_instance_id: Some("agent-root".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        });

    app.apply_event(Event::AgentRunLifecycle(
        piko_protocol::AgentRunEvent::Started {
            session_id: "session-1".into(),
            run_id: "run-1".into(),
            agent_instance_id: "agent-child".into(),
            timestamp: 1,
        },
    ));

    let agent = &app.agent_panel.agents[0];
    assert_eq!(agent.activity, piko_protocol::AgentActivity::Idle);
    assert_eq!(agent.status, piko_protocol::AgentStatus::Idle);
    assert!(app.session.active_turns.is_empty());
}
