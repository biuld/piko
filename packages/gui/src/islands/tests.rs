//! Workbench view-model tests (no GPUI).

use piko_client_core::state::{ActiveTurn, LiveSession};
use piko_client_core::{AgentTimeline, ClientState, SessionPhase, TimelineItem};
use piko_protocol::{AgentActivity, AgentInfo, AgentInstanceLifecycle, AgentStatus, TurnStatus};

use super::composer::ActivityItemKind;
use super::timeline::{TimelineRowKind, ToolCardStatus};
use super::*;

fn agent(instance: &str, parent: Option<&str>, name: &str) -> AgentInfo {
    AgentInfo {
        session_id: "s1".into(),
        agent_instance_id: instance.into(),
        agent_id: format!("{instance}-spec"),
        parent_agent_instance_id: parent.map(str::to_string),
        lifecycle: AgentInstanceLifecycle::Open,
        activity: AgentActivity::Idle,
        unread_report_count: 0,
        name: name.into(),
        role: "assistant".into(),
        status: AgentStatus::Idle,
    }
}

fn live_state() -> ClientState {
    let mut timelines = std::collections::HashMap::new();
    let mut tl = AgentTimeline::new();
    tl.apply_committed(
        "msg-1".into(),
        1,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String("hello".into()),
            timestamp: Some(1),
        },
        "turn-1".into(),
    );
    timelines.insert("root".into(), tl);

    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        cwd: "/tmp".into(),
        selected_agent: Some("root".into()),
        agents: vec![
            agent("root", None, "Main"),
            agent("child", Some("root"), "Researcher"),
        ],
        timelines,
        ..Default::default()
    });
    state
}

#[test]
fn timeline_rows_from_committed() {
    let vm = derive_timeline(&live_state());
    assert_eq!(vm.rows.len(), 1);
    assert_eq!(vm.rows[0].body, "hello");
    assert_eq!(vm.rows[0].kind, TimelineRowKind::User);
    assert!(!vm.rows[0].render_markdown);
    assert_eq!(vm.selected_agent_name.as_deref(), Some("Main"));
}

#[test]
fn committed_assistant_marks_markdown_path() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_committed(
        "msg-a".into(),
        2,
        piko_protocol::Message::Assistant {
            content: vec![piko_protocol::ContentBlock::Text {
                text: "**bold** and `code`".into(),
            }],
            api: "chat".into(),
            provider: "test".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(2),
        },
        "turn-1".into(),
    );
    let vm = derive_timeline(&state);
    let row = vm.rows.iter().find(|r| r.id == "msg-a").unwrap();
    assert_eq!(row.kind, TimelineRowKind::Assistant);
    assert!(row.render_markdown);
    assert!(!row.streaming);
    assert!(row.body.contains("**bold**"));
}

#[test]
fn assistant_thinking_uses_style_without_a_redundant_label() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_committed(
        "msg-thinking".into(),
        2,
        piko_protocol::Message::Assistant {
            content: vec![
                piko_protocol::ContentBlock::Thinking {
                    thinking: "consider alternatives".into(),
                    thinking_signature: None,
                },
                piko_protocol::ContentBlock::Text {
                    text: "final answer".into(),
                },
            ],
            api: "chat".into(),
            provider: "test".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(2),
        },
        "turn-1".into(),
    );

    let vm = derive_timeline(&state);
    let row = vm.rows.iter().find(|r| r.id == "msg-thinking").unwrap();
    assert!(row.body.contains("> consider alternatives"));
    assert!(row.body.contains("final answer"));
    assert!(!row.body.to_lowercase().contains("thinking"));
}

#[test]
fn timeline_includes_realtime_draft() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_realtime(
        "draft-1".into(),
        1,
        &piko_protocol::agent_runtime::RealtimeDelta::Text {
            content_index: 0,
            delta: "stream…".into(),
        },
    );
    // Ensure draft exists
    assert!(
        tl.items()
            .iter()
            .any(|i| matches!(i, TimelineItem::RealtimeDraft(_)))
    );

    let vm = derive_timeline(&state);
    let draft = vm
        .rows
        .iter()
        .find(|r| r.streaming && r.body.contains("stream"))
        .unwrap();
    assert!(!draft.render_markdown);
}

#[test]
fn streaming_thinking_uses_the_same_unlabeled_quote_style() {
    let mut state = live_state();
    let session = state.live_session.as_mut().unwrap();
    let tl = session.timelines.get_mut("root").unwrap();
    tl.apply_realtime(
        "draft-thinking".into(),
        1,
        &piko_protocol::agent_runtime::RealtimeDelta::Thinking {
            content_index: 0,
            delta: "working through it".into(),
        },
    );

    let vm = derive_timeline(&state);
    let draft = vm.rows.iter().find(|r| r.id == "draft-thinking").unwrap();
    assert!(draft.render_markdown);
    assert_eq!(draft.body, "> working through it");
    assert!(!draft.body.to_lowercase().contains("thinking"));
}

#[test]
fn activity_show_stop_when_running() {
    let mut state = live_state();
    state
        .live_session
        .as_mut()
        .unwrap()
        .active_turns
        .push(ActiveTurn {
            turn_id: "t1".into(),
            agent_instance_id: "root".into(),
            status: TurnStatus::Running,
        });
    let vm = derive_activity(&state);
    assert!(vm.show_stop);
    assert!(vm.summary.contains("running"));
}

#[test]
fn composer_can_send_when_live_with_agent() {
    let vm = derive_composer(&live_state());
    assert!(vm.can_send);
    assert_eq!(vm.target_label, "Main");
    assert!(!vm.show_stop);
}

#[test]
fn composer_idle_without_session() {
    let vm = derive_composer(&ClientState::default());
    assert!(vm.can_send);
}

#[test]
fn agent_tree_hierarchy_and_selection() {
    let vm = derive_agent_tree(&live_state());
    assert_eq!(vm.nodes.len(), 2);
    assert_eq!(vm.nodes[0].name, "Main");
    assert_eq!(vm.nodes[0].depth, 0);
    assert!(vm.nodes[0].selected);
    assert!(vm.nodes[0].has_children);
    assert_eq!(vm.nodes[1].name, "Researcher");
    assert_eq!(vm.nodes[1].depth, 1);
    assert!(!vm.nodes[1].selected);
    assert!(!vm.nodes[1].has_children);
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
    let call_row = vm
        .rows
        .iter()
        .find(|r| r.label.contains("tool read_file"))
        .unwrap();
    assert_eq!(call_row.tool_status, Some(ToolCardStatus::Completed));
}

#[test]
fn timeline_tool_card_failed_on_error_result() {
    let mut state = live_state();
    let tl = state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .get_mut("root")
        .unwrap();
    tl.apply_committed(
        "tc-err".into(),
        2,
        piko_protocol::Message::ToolCall {
            id: "call-err".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
            model: None,
            provider: None,
            timestamp: Some(2),
        },
        "turn-e".into(),
    );
    tl.apply_committed(
        "tr-err".into(),
        3,
        piko_protocol::Message::ToolResult {
            tool_call_id: "call-err".into(),
            tool_name: Some("bash".into()),
            content: vec![piko_protocol::ContentBlock::Text {
                text: "boom".into(),
            }],
            details: None,
            is_error: Some(true),
            timestamp: Some(3),
        },
        "turn-e".into(),
    );

    let vm = derive_timeline(&state);
    let call_row = vm
        .rows
        .iter()
        .find(|r| r.label.contains("tool bash"))
        .unwrap();
    assert_eq!(call_row.tool_status, Some(ToolCardStatus::Failed));
    assert!(call_row.body.contains("failed"));

    let activity = derive_activity(&state);
    assert!(
        activity
            .items
            .iter()
            .any(|i| i.kind == ActivityItemKind::ToolFailed)
    );
}

#[test]
fn activity_and_composer_show_stop_when_queued() {
    let mut state = live_state();
    state
        .live_session
        .as_mut()
        .unwrap()
        .active_turns
        .push(ActiveTurn {
            turn_id: "tq".into(),
            agent_instance_id: "root".into(),
            status: TurnStatus::Queued,
        });
    let activity = derive_activity(&state);
    assert!(activity.show_stop);
    assert!(
        activity.summary.to_lowercase().contains("queued")
            || activity
                .items
                .iter()
                .any(|i| i.kind == ActivityItemKind::TurnQueued)
    );

    let composer = derive_composer(&state);
    assert!(composer.show_stop);
}

#[test]
fn activity_projects_core_queue_tool_and_turn_failure() {
    let mut state = live_state();
    let live = state.live_session.as_mut().unwrap();
    live.queue.next_turn_count = 2;
    live.turn_failures
        .push(piko_client_core::state::TurnFailure {
            turn_id: "failed-turn".into(),
            agent_instance_id: "root".into(),
            error: "provider unavailable".into(),
        });
    let mut timeline = AgentTimeline::default();
    timeline.apply_tool_started(
        "call-live".into(),
        "exec".into(),
        serde_json::json!({"cmd": "true"}),
        None,
    );
    live.timelines.insert("root".into(), timeline);

    let vm = derive_activity(&state);
    assert!(vm.items.iter().any(|item| item.id == "host-queue"));
    assert!(
        vm.items
            .iter()
            .any(|item| item.id == "turn-fail-failed-turn")
    );
    assert!(vm.items.iter().any(|item| item.id == "tool-call-live"));
}

#[test]
fn timeline_renders_authoritative_tool_projection() {
    let mut state = live_state();
    let mut timeline = AgentTimeline::default();
    timeline.apply_tool_started(
        "call-1".into(),
        "exec".into(),
        serde_json::json!({"cmd": "true"}),
        None,
    );
    timeline.apply_tool_ended(
        "call-1".into(),
        "exec".into(),
        serde_json::json!({"exit": 0}),
        false,
    );
    state
        .live_session
        .as_mut()
        .unwrap()
        .timelines
        .insert("root".into(), timeline);
    let vm = derive_timeline(&state);
    assert_eq!(vm.rows[0].tool_status, Some(ToolCardStatus::Completed));
    assert!(vm.rows[0].detail.as_deref().unwrap().contains("Arguments"));
}
