use super::*;

#[test]
fn agent_tree_hierarchy_and_selection() {
    let vm = derive_agent_tree(&live_state());
    assert_eq!(vm.nodes.len(), 2);
    assert_eq!(vm.nodes[0].name, "Main");
    assert_eq!(vm.nodes[0].depth, 0);
    assert!(vm.nodes[0].selected);
    assert!(vm.nodes[0].has_children);
    assert!(vm.nodes[0].parent_agent_instance_id.is_none());
    assert_eq!(vm.nodes[1].name, "Researcher");
    assert_eq!(vm.nodes[1].depth, 1);
    assert!(!vm.nodes[1].selected);
    assert!(!vm.nodes[1].has_children);
    assert_eq!(
        vm.nodes[1].parent_agent_instance_id.as_deref(),
        Some(vm.nodes[0].agent_instance_id.as_str())
    );
}

#[test]
fn agent_tree_collapse_hides_descendants() {
    use std::collections::HashSet;

    use super::agents::agent_node_visible;

    let vm = derive_agent_tree(&live_state());
    let root = vm.nodes[0].agent_instance_id.clone();
    let mut collapsed = HashSet::new();
    assert!(agent_node_visible(&vm.nodes[1], &vm.nodes, &collapsed));
    collapsed.insert(root);
    assert!(!agent_node_visible(&vm.nodes[1], &vm.nodes, &collapsed));
    assert!(agent_node_visible(&vm.nodes[0], &vm.nodes, &collapsed));
}

#[test]
fn activity_lists_approval_as_actionable() {
    use piko_client_core::state::PendingApproval;

    let mut state = live_state();
    state
        .live_session
        .as_mut()
        .unwrap()
        .pending_approvals
        .push(PendingApproval {
            approval_id: "a1".into(),
            agent_instance_id: "root".into(),
            tool_name: "exec".into(),
            tool_args: serde_json::json!({"cmd": "ls"}),
            prompt: None,
            response_in_flight: false,
        });
    let vm = derive_activity(&state);
    assert!(vm.has_actionable);
    assert!(vm.prefer_expanded);
    assert!(vm.summary.contains("approval"));
    assert!(vm.items.iter().any(|i| i.label.contains("exec")));
}

#[test]
fn timeline_tool_card_running_then_completed() {
    let mut state = live_state();
    let tl = state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .get_mut("root")
        .unwrap();
    tl.apply_committed(
        "tc-1".into(),
        2,
        piko_protocol::Message::ToolCall {
            id: "call-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/tmp/x"}),
            model: None,
            provider: None,
            timestamp: Some(2),
        },
        "turn-2".into(),
    );

    let vm = derive_timeline(&state);
    let tool = vm
        .rows
        .iter()
        .find(|r| r.kind == TimelineRowKind::Tool)
        .unwrap();
    assert_eq!(tool.tool_status, Some(ToolCardStatus::Running));
    assert!(tool.detail.is_some());

    let tl = state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .get_mut("root")
        .unwrap();
    tl.apply_committed(
        "tr-1".into(),
        3,
        piko_protocol::Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("read_file".into()),
            content: vec![piko_protocol::ContentBlock::Text { text: "ok".into() }],
            details: None,
            is_error: Some(false),
            timestamp: Some(3),
        },
        "turn-2".into(),
    );

    let vm = derive_timeline(&state);
    let call_row = vm.rows.iter().find(|r| r.label == "read_file").unwrap();
    assert_eq!(call_row.tool_status, Some(ToolCardStatus::Completed));
    assert_eq!(
        vm.rows
            .iter()
            .filter(|r| r.kind == TimelineRowKind::Tool)
            .count(),
        1,
        "ToolResult must fold into the ToolCall row"
    );
}
