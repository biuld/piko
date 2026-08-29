use super::*;
use crate::theme::Theme;
use crate::ui::components::feedback::loading_line;

#[test]
fn loading_until_hydrated_never_uses_fake_main_label() {
    let state = AgentPanelState::default();
    assert!(state.is_loading());

    let line = loading_line(0, &Theme::dark());
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("loading"));
    assert!(!text.contains("main"));
    assert!(!text.contains("Main"));
}

#[test]
fn prepare_for_switch_selects_active_agent() {
    let mut state = AgentPanelState::default();
    state.mark_hydrated();
    state.list = SelectableList::new(vec![
        AgentEntry {
            agent_id: "main".into(),
            agent_instance_id: "a-root".into(),
            name: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        },
        AgentEntry {
            agent_id: "coder".into(),
            agent_instance_id: "a-child".into(),
            name: "coder".into(),
            parent_agent_instance_id: Some("a-root".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        },
    ]);
    state.active_agent_instance_id = Some("a-child".into());
    state.list.selected = 0;
    state.prepare_for_switch();
    assert_eq!(state.list.selected, 1);
    assert_eq!(
        state.selected_agent().map(|a| a.agent_instance_id.as_str()),
        Some("a-child")
    );
}

#[test]
fn filter_select_next_stays_in_matches() {
    let mut state = AgentPanelState::default();
    state.mark_hydrated();
    state.list = SelectableList::new(vec![
        AgentEntry {
            agent_id: "main".into(),
            agent_instance_id: "a1".into(),
            name: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        },
        AgentEntry {
            agent_id: "coder".into(),
            agent_instance_id: "a2".into(),
            name: "coder".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Idle,
        },
    ]);
    state.filter = "cod".into();
    state.reset_selection();
    assert_eq!(state.list.selected, 1);
    state.select_next();
    assert_eq!(state.list.selected, 1);
}

fn agent(id: &str, parent: Option<&str>) -> AgentEntry {
    AgentEntry {
        agent_id: "general".into(),
        agent_instance_id: id.into(),
        name: "General".into(),
        parent_agent_instance_id: parent.map(str::to_owned),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Idle,
        unread_report_count: 0,
        status: piko_protocol::AgentStatus::Idle,
    }
}

#[test]
fn tree_prefixes_use_fixed_three_cell_levels() {
    let agents = vec![
        agent("root", None),
        agent("child", Some("root")),
        agent("grandchild", Some("child")),
        agent("leaf", Some("grandchild")),
    ];

    assert_eq!(
        build_tree_prefixes(&agents),
        vec!["", "└─ ", "   └─ ", "      └─ "]
    );
}

#[test]
fn tree_prefixes_keep_vertical_gutters_for_later_siblings() {
    let agents = vec![
        agent("root", None),
        agent("left", Some("root")),
        agent("left-child", Some("left")),
        agent("right", Some("root")),
    ];

    assert_eq!(
        build_tree_prefixes(&agents),
        vec!["", "├─ ", "│  └─ ", "└─ "]
    );
}
