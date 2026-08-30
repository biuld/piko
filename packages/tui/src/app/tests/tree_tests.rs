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
    assert_eq!(
        app.session.pending_turn_content,
        Some(piko_protocol::MessageContent::String("hello".into()))
    );
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
        Effect::Send(piko_protocol::Command::AgentInputSubmit { input, .. })
            if input.content == piko_protocol::MessageContent::String("hello".into())
    ));
    assert!(app.timeline().message_ids().is_empty());
}

#[test]
fn submit_targets_the_viewed_child_agent() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel
        .list
        .items
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
        Effect::Send(piko_protocol::Command::AgentInputSubmit { input, .. })
            if input.session_id == "session-1"
                && input.agent_instance_id == "agent-child"
                && input.content == piko_protocol::MessageContent::String("follow up".into())
                && input.delivery == piko_protocol::AgentInputDelivery::FollowUp
    ));
}
