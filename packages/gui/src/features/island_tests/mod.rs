//! Workbench view-model tests (no GPUI).

use piko_client_core::state::{ActiveTurn, LiveSession};
use piko_client_core::{AgentTimeline, ClientState, SessionPhase, TimelineItem};
use piko_protocol::{AgentActivity, AgentInfo, AgentInstanceLifecycle, AgentStatus, TurnStatus};

use super::composer::ActivityItemKind;
use super::timeline::{
    TimelineRow, TimelineRowKind, ToolCardStatus, VisualSender, group_timeline_rows,
};
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

mod group_tests;
mod streaming_tests;
mod timeline_tests;
mod tool_tests;
mod tree_tests;

fn sample_row(id: &str, kind: TimelineRowKind) -> TimelineRow {
    TimelineRow {
        id: id.into(),
        kind,
        label: String::new(),
        body: String::new(),
        streaming: false,
        render_markdown: false,
        tool_status: None,
        detail: None,
        thinking: None,
        thinking_live: false,
    }
}
