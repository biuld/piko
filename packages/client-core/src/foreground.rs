//! Per-AgentInstance foreground work projection (F-22 / F-51).
//!
//! Prefer the host-authored [`AgentWorkSnapshot`]. Local pending
//! approvals/interactions still upgrade the view to RequiresAction
//! before the next snapshot arrives.

use piko_protocol::{AgentActivity, AgentForeground, AgentInfo, AgentInstanceId};

use crate::state::{LiveSession, PendingApproval, PendingInteraction};

/// Project foreground work for one agent instance.
pub fn agent_foreground(agent_instance_id: &str, session: &LiveSession) -> AgentForeground {
    let blocked = session
        .pending_approvals
        .iter()
        .any(|a| a.agent_instance_id == agent_instance_id)
        || session
            .pending_interactions
            .iter()
            .any(|i| i.agent_instance_id == agent_instance_id);
    if blocked {
        return AgentForeground::RequiresAction;
    }
    if let Some(work) = session.agent_work.get(agent_instance_id) {
        return work.foreground;
    }
    let activity = session
        .agents
        .iter()
        .find(|a| a.agent_instance_id == agent_instance_id)
        .map(|a| &a.activity);
    activity
        .map(AgentForeground::from_activity)
        .unwrap_or(AgentForeground::Idle)
}

/// Project foreground for the session focus (selected agent), or any busy agent.
pub fn focused_or_any_busy(session: &LiveSession) -> AgentForeground {
    if let Some(id) = session.selected_agent.as_deref() {
        let fg = agent_foreground(id, session);
        if fg.is_busy() {
            return fg;
        }
    }
    for agent in &session.agents {
        let fg = agent_foreground(&agent.agent_instance_id, session);
        if fg.is_busy() {
            return fg;
        }
    }
    AgentForeground::Idle
}

/// Keep agent activity consistent with pending prompts and active work.
pub(crate) fn refresh_prompt_blocking(
    agents: &mut [AgentInfo],
    agent_work: &std::collections::HashMap<String, piko_protocol::AgentWorkSnapshot>,
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

    for agent in agents
        .iter_mut()
        .filter(|a| a.agent_instance_id == *agent_instance_id)
    {
        if blocked {
            agent.activity = AgentActivity::WaitingForApproval;
        } else {
            let has_work = agent_work
                .get(agent_instance_id)
                .is_some_and(|work| work.active_work.is_some());
            agent.activity = if has_work {
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
    use crate::state::{LiveSession, PendingApproval, PendingInteraction};
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

    fn work(id: &str, foreground: AgentForeground) -> piko_protocol::AgentWorkSnapshot {
        piko_protocol::AgentWorkSnapshot {
            agent_instance_id: id.into(),
            lifecycle: AgentInstanceLifecycle::Open,
            foreground,
            active_work: matches!(
                foreground,
                AgentForeground::Running
                    | AgentForeground::Cancelling
                    | AgentForeground::RequiresAction
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

    fn session(
        agents: Vec<AgentInfo>,
        work_items: Vec<piko_protocol::AgentWorkSnapshot>,
        approvals: Vec<PendingApproval>,
    ) -> LiveSession {
        LiveSession {
            agents,
            agent_work: work_items
                .into_iter()
                .map(|item| (item.agent_instance_id.clone(), item))
                .collect(),
            pending_approvals: approvals,
            ..Default::default()
        }
    }

    #[test]
    fn approval_forces_requires_action() {
        let session = session(
            vec![agent("a1", AgentActivity::Running)],
            vec![work("a1", AgentForeground::Running)],
            vec![PendingApproval {
                approval_id: "ap".into(),
                agent_instance_id: "a1".into(),
                tool_name: "shell".into(),
                tool_args: serde_json::json!({}),
                prompt: None,
                response_in_flight: false,
            }],
        );
        assert_eq!(
            agent_foreground("a1", &session),
            AgentForeground::RequiresAction
        );
    }

    #[test]
    fn idle_without_work_or_activity() {
        assert_eq!(
            agent_foreground("missing", &LiveSession::default()),
            AgentForeground::Idle
        );
    }

    #[test]
    fn activity_fallback_when_no_work_snapshot() {
        let session = session(
            vec![agent("a1", AgentActivity::Running)],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(agent_foreground("a1", &session), AgentForeground::Running);
    }

    #[test]
    fn work_snapshot_drives_queued_running_cancelling() {
        let agents = vec![agent("a1", AgentActivity::Idle)];
        for expected in [
            AgentForeground::Queued,
            AgentForeground::Running,
            AgentForeground::Cancelling,
        ] {
            let session = session(agents.clone(), vec![work("a1", expected)], Vec::new());
            assert_eq!(agent_foreground("a1", &session), expected);
        }
    }

    #[test]
    fn refresh_prompt_blocking_updates_activity_from_work() {
        let mut agents = vec![agent("a1", AgentActivity::Running)];
        let work_map = std::collections::HashMap::from([(
            "a1".to_string(),
            work("a1", AgentForeground::Running),
        )]);
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
            &work_map,
            &approvals,
            &[] as &[PendingInteraction],
            &"a1".into(),
        );
        assert_eq!(agents[0].activity, AgentActivity::WaitingForApproval);
        refresh_prompt_blocking(&mut agents, &work_map, &[], &[], &"a1".into());
        assert_eq!(agents[0].activity, AgentActivity::Running);
    }
}
