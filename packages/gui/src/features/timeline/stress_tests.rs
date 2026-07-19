//! Pure scale harness for timeline VM + follow helper (no GPUI).

use piko_client_core::state::LiveSession;
use piko_client_core::{AgentTimeline, ClientState, SessionPhase};
use piko_protocol::{AgentActivity, AgentInfo, AgentInstanceLifecycle, AgentStatus};

use crate::app::timeline_follow::should_scroll_on_growth;
use crate::features::derive_timeline;

fn agent(instance: &str) -> AgentInfo {
    AgentInfo {
        session_id: "s1".into(),
        agent_instance_id: instance.into(),
        agent_id: format!("{instance}-spec"),
        parent_agent_instance_id: None,
        lifecycle: AgentInstanceLifecycle::Open,
        activity: AgentActivity::Idle,
        unread_report_count: 0,
        name: instance.into(),
        role: "assistant".into(),
        status: AgentStatus::Idle,
    }
}

#[test]
fn stress_timeline_vm_handles_large_committed_set() {
    let mut timelines = std::collections::HashMap::new();
    let mut tl = AgentTimeline::new();
    for i in 0..250 {
        tl.apply_committed(
            format!("m-{i}"),
            i as u64 + 1,
            piko_protocol::Message::User {
                content: piko_protocol::MessageContent::String(format!("line {i}")),
                timestamp: Some(i as i64),
            },
            "t".into(),
        );
    }
    timelines.insert("root".into(), tl);

    let mut state = ClientState::default();
    state.session_phase = SessionPhase::Live;
    state.live_session = Some(LiveSession {
        session_id: "s1".into(),
        cwd: "/tmp".into(),
        selected_agent: Some("root".into()),
        agents: vec![agent("root")],
        timelines,
        ..Default::default()
    });

    let vm = derive_timeline(&state);
    assert_eq!(vm.rows.len(), 250);
    assert!(!should_scroll_on_growth(false, true, false));
    assert!(should_scroll_on_growth(true, true, false));
}
