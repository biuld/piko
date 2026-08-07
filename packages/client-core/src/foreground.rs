//! Per-AgentInstance foreground work projection (F-22 / D-34).
//!
//! Thin adapter over [`AgentForeground::project`] — the sole priority table
//! lives in `piko-protocol` so TUI and GUI stay aligned.

use piko_protocol::{AgentActivity, AgentForeground, AgentInfo, AgentInstanceId, TurnStatus};

use crate::state::{ActiveTurn, LiveSession, PendingApproval, PendingInteraction};

/// Project foreground work for one agent instance.
///
/// Delegates to [`AgentForeground::project`] (sole mapping). Inputs are only
/// collected here from client-core state shapes.
pub fn agent_foreground(
    agent_instance_id: &str,
    agents: &[AgentInfo],
    active_turns: &[ActiveTurn],
    pending_approvals: &[PendingApproval],
    pending_interactions: &[PendingInteraction],
) -> AgentForeground {
    let blocked = pending_approvals
        .iter()
        .any(|a| a.agent_instance_id == agent_instance_id)
        || pending_interactions
            .iter()
            .any(|i| i.agent_instance_id == agent_instance_id);
    let turn_status = active_turns
        .iter()
        .find(|t| t.agent_instance_id == agent_instance_id)
        .map(|t| t.status);
    let activity = agents
        .iter()
        .find(|a| a.agent_instance_id == agent_instance_id)
        .map(|a| &a.activity);
    AgentForeground::project(blocked, turn_status, activity)
}

/// Project foreground for the session focus (selected agent), or any busy agent.
pub fn focused_or_any_busy(session: &LiveSession) -> AgentForeground {
    if let Some(id) = session.selected_agent.as_deref() {
        let fg = agent_foreground(
            id,
            &session.agents,
            &session.active_turns,
            &session.pending_approvals,
            &session.pending_interactions,
        );
        if fg.is_busy() {
            return fg;
        }
    }
    for agent in &session.agents {
        let fg = agent_foreground(
            &agent.agent_instance_id,
            &session.agents,
            &session.active_turns,
            &session.pending_approvals,
            &session.pending_interactions,
        );
        if fg.is_busy() {
            return fg;
        }
    }
    AgentForeground::Idle
}

/// Keep turn + agent activity projections consistent with pending prompts.
pub(crate) fn refresh_prompt_blocking(
    agents: &mut [AgentInfo],
    active_turns: &mut [ActiveTurn],
    pending_approvals: &[PendingApproval],
    pending_interactions: &[PendingInteraction],
    agent_instance_id: &AgentInstanceId,
) {
    let blocked = pending_approvals
        .iter()
        .any(|a| a.agent_instance_id == *agent_instance_id)
        || pending_interactions
            .iter()
            .any(|i| i.agent_instance_id == *agent_instance_id);

    for turn in active_turns
        .iter_mut()
        .filter(|t| t.agent_instance_id == *agent_instance_id)
    {
        if blocked {
            if matches!(
                turn.status,
                TurnStatus::Running | TurnStatus::WaitingForApproval | TurnStatus::Cancelling
            ) {
                turn.status = TurnStatus::WaitingForApproval;
            }
        } else if turn.status == TurnStatus::WaitingForApproval {
            turn.status = TurnStatus::Running;
        }
    }

    for agent in agents
        .iter_mut()
        .filter(|a| a.agent_instance_id == *agent_instance_id)
    {
        if blocked {
            agent.activity = AgentActivity::WaitingForApproval;
        } else {
            let has_turn = active_turns
                .iter()
                .any(|t| t.agent_instance_id == *agent_instance_id);
            agent.activity = if has_turn {
                AgentActivity::Running
            } else if matches!(
                agent.activity,
                AgentActivity::WaitingForApproval | AgentActivity::Cancelling
            ) {
                AgentActivity::Idle
            } else {
                agent.activity.clone()
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{PendingApproval, PendingInteraction};
    use piko_protocol::{AgentInstanceLifecycle, AgentStatus};

    fn agent(id: &str, activity: AgentActivity) -> AgentInfo {
        AgentInfo {
            session_id: "s".into(),
            agent_instance_id: id.into(),
            agent_id: id.into(),
            parent_agent_instance_id: None,
            lifecycle: AgentInstanceLifecycle::Open,
            activity,
            unread_report_count: 0,
            name: id.into(),
            role: "assistant".into(),
            status: AgentStatus::Idle,
        }
    }

    #[test]
    fn approval_forces_requires_action() {
        let agents = vec![agent("a1", AgentActivity::Running)];
        let turns = vec![ActiveTurn {
            turn_id: "t1".into(),
            agent_instance_id: "a1".into(),
            status: TurnStatus::Running,
        }];
        let approvals = vec![PendingApproval {
            approval_id: "ap".into(),
            agent_instance_id: "a1".into(),
            tool_name: "shell".into(),
            tool_args: serde_json::json!({}),
            prompt: None,
            response_in_flight: false,
        }];
        assert_eq!(
            agent_foreground("a1", &agents, &turns, &approvals, &[]),
            AgentForeground::RequiresAction
        );
        // Same inputs via the protocol sole path.
        assert_eq!(
            AgentForeground::project(
                true,
                Some(TurnStatus::Running),
                Some(&AgentActivity::Running)
            ),
            AgentForeground::RequiresAction
        );
    }

    #[test]
    fn idle_without_turn_or_activity() {
        assert_eq!(
            agent_foreground("missing", &[], &[], &[], &[]),
            AgentForeground::Idle
        );
    }

    #[test]
    fn activity_fallback_when_no_turn() {
        let agents = vec![agent("a1", AgentActivity::Running)];
        assert_eq!(
            agent_foreground("a1", &agents, &[], &[], &[]),
            AgentForeground::Running
        );
    }

    #[test]
    fn queued_running_cancelling_parity_with_protocol_project() {
        let agents = vec![agent("a1", AgentActivity::Idle)];
        for (status, expected) in [
            (TurnStatus::Queued, AgentForeground::Queued),
            (TurnStatus::Running, AgentForeground::Running),
            (TurnStatus::Cancelling, AgentForeground::Cancelling),
            (
                TurnStatus::WaitingForApproval,
                AgentForeground::RequiresAction,
            ),
        ] {
            let turns = vec![ActiveTurn {
                turn_id: "t1".into(),
                agent_instance_id: "a1".into(),
                status,
            }];
            let via_core = agent_foreground("a1", &agents, &turns, &[], &[]);
            let via_protocol =
                AgentForeground::project(false, Some(status), Some(&AgentActivity::Idle));
            assert_eq!(via_core, expected, "status={status:?}");
            assert_eq!(via_core, via_protocol, "status={status:?}");
        }
    }

    #[test]
    fn refresh_prompt_blocking_updates_turn_and_activity() {
        let mut agents = vec![agent("a1", AgentActivity::Running)];
        let mut turns = vec![ActiveTurn {
            turn_id: "t1".into(),
            agent_instance_id: "a1".into(),
            status: TurnStatus::Running,
        }];
        let approvals = vec![PendingApproval {
            approval_id: "ap".into(),
            agent_instance_id: "a1".into(),
            tool_name: "shell".into(),
            tool_args: serde_json::json!({}),
            prompt: None,
            response_in_flight: false,
        }];
        refresh_prompt_blocking(
            &mut agents,
            &mut turns,
            &approvals,
            &[] as &[PendingInteraction],
            &"a1".into(),
        );
        assert_eq!(turns[0].status, TurnStatus::WaitingForApproval);
        assert_eq!(agents[0].activity, AgentActivity::WaitingForApproval);
        refresh_prompt_blocking(&mut agents, &mut turns, &[], &[], &"a1".into());
        assert_eq!(turns[0].status, TurnStatus::Running);
        assert_eq!(agents[0].activity, AgentActivity::Running);
    }
}
