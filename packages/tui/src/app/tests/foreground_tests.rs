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
    use piko_protocol::AgentForeground;

    let mut app = live_app();
    let agent_id = "agent-fg-1";

    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Idle);

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Queued));
    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Queued);

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Running));
    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Running);

    app.approvals
        .push(crate::features::approval::PendingApproval {
            id: "ap-1".into(),
            agent_instance_id: agent_id.into(),
            tool_name: "shell".into(),
            args: json!({}),
            prompt: None,
            selected_idx: 0,
        });
    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Running);
    app.approvals.resolve("ap-1");

    app.session.agent_work.insert(
        agent_id.into(),
        work(agent_id, AgentForeground::RequiresAction),
    );
    assert_eq!(
        app.agent_foreground(agent_id),
        AgentForeground::RequiresAction
    );

    app.session
        .agent_work
        .insert(agent_id.into(), work(agent_id, AgentForeground::Cancelling));
    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Cancelling);

    app.session.agent_work.clear();
    assert_eq!(app.agent_foreground(agent_id), AgentForeground::Idle);
}

#[test]
fn local_approval_event_does_not_change_foreground_or_activity() {
    let mut app = live_app();
    app.agent_panel.active_agent_instance_id = Some("task-1".into());
    app.agent_panel
        .upsert_agent(crate::features::agent_status::AgentEntry {
            agent_id: "main".into(),
            agent_instance_id: "task-1".into(),
            name: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Running,
        });

    app.apply_event(Event::Approval(piko_protocol::ApprovalEvent::Requested {
        session_id: "session-1".into(),
        agent_instance_id: "task-1".into(),
        agent_id: "main".into(),
        approval_id: "approval-1".into(),
        tool_name: "exec".into(),
        tool_args: serde_json::json!({}),
        prompt: None,
    }));

    assert_eq!(
        app.agent_foreground("task-1"),
        piko_protocol::AgentForeground::Idle
    );
    assert_eq!(
        app.agent_panel
            .agents()
            .iter()
            .find(|agent| agent.agent_instance_id == "task-1")
            .map(|agent| agent.activity.clone()),
        Some(piko_protocol::AgentActivity::Running)
    );
}
