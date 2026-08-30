use super::*;

fn work(
    agent_id: &str,
    foreground: piko_protocol::AgentForeground,
) -> piko_protocol::AgentWorkSnapshot {
    piko_protocol::AgentWorkSnapshot {
        agent_instance_id: agent_id.into(),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        foreground,
        active_work: matches!(
            foreground,
            piko_protocol::AgentForeground::Running
                | piko_protocol::AgentForeground::Cancelling
                | piko_protocol::AgentForeground::RequiresAction
        )
        .then(|| piko_protocol::ActiveWorkSnapshot {
            root_input_id: "root".into(),
            state: piko_protocol::AgentWorkViewState::Running,
            active_model_step_id: None,
            started_at: 1,
        }),
        pending_steers: Vec::new(),
        queued_inputs: Vec::new(),
        pending_action: None,
    }
}

/// F-51: TUI foreground prefers the host-authored AgentWorkSnapshot.
#[test]
fn agent_foreground_matches_protocol_project_for_busy_states() {
    use piko_protocol::{AgentActivity, AgentForeground};

    let mut app = live_app();
    let agent_id = "agent-fg-1";
    let activity = AgentActivity::Idle;

    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Idle
    );

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Queued));
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Queued
    );

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Running));
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Running
    );

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
    app.approvals.resolve("ap-1");

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Cancelling));
    assert_eq!(
        app.agent_foreground(agent_id, &activity),
        AgentForeground::Cancelling
    );

    app.session.agent_work.clear();
    let cancelling = AgentActivity::Cancelling;
    assert_eq!(
        app.agent_foreground(agent_id, &cancelling),
        AgentForeground::Cancelling
    );
}
