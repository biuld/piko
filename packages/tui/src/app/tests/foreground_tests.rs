use super::*;

/// F-22: TUI foreground uses the same protocol projection as client-core.
#[test]
fn agent_foreground_matches_protocol_project_for_busy_states() {
    use piko_protocol::{AgentActivity, AgentForeground, TurnStatus};

    let mut app = live_app();
    let agent_id = "agent-fg-1";
    let activity = AgentActivity::Idle;

    // Idle — no turn, no block.
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::project(false, None, Some(&activity))
    );
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Idle
    );

    // Queued turn.
    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
        session_id: "session-1".into(),
        turn_id: "t-q".into(),
        agent_instance_id: agent_id.into(),
        timestamp: 0,
    }));
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Queued
    );
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::project(false, Some(TurnStatus::Queued), Some(&activity))
    );

    // Running turn.
    app.apply_event(Event::TurnLifecycle(piko_protocol::TurnEvent::Started {
        session_id: "session-1".into(),
        turn_id: "t-q".into(),
        agent_instance_id: agent_id.into(),
        timestamp: 0,
    }));
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Running
    );

    // Approval blocks over running.
    app.approvals
        .push(crate::features::approval::PendingApproval {
            id: "ap-1".into(),
            agent_instance_id: agent_id.into(),
            tool_name: "shell".into(),
            args: json!({}),
            prompt: None,
            selected_idx: 0,
        });
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::RequiresAction
    );
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::project(true, Some(TurnStatus::Running), Some(&activity))
    );
    app.approvals.resolve("ap-1");

    // Cancelling via tracked turn status (snapshot-style).
    app.session.active_turns.insert(
        agent_id.into(),
        crate::app::ActiveTurnUi {
            turn_id: "t-c".into(),
            status: TurnStatus::Cancelling,
        },
    );
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Cancelling
    );
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::project(false, Some(TurnStatus::Cancelling), Some(&activity))
    );

    // Activity fallback when no turn.
    app.session.active_turns.clear();
    let cancelling = AgentActivity::Cancelling;
    assert_eq!(
        app.agent_foreground(agent_id, &cancelling),
        AgentForeground::Cancelling
    );
}
