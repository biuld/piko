use serde::{Deserialize, Serialize};

pub type AgentInstanceId = String;
pub type AgentSpecId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstanceIdentity {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub agent_spec_id: AgentSpecId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentInstanceLifecycle {
    Open,
    Closed,
    Terminated,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentActivity {
    Idle,
    Running,
    WaitingForApproval,
    Cancelling,
}

/// Client-facing foreground work state (F-22 / D-34).
///
/// This is a projection of host-authoritative turns, pending prompts, and
/// [`AgentActivity`] — not a separate orchd-owned state machine. Maps to ACP
/// v2-style session readiness semantics (`idle` / `running` / `requires_action`).
///
/// Compatibility mapping tables for the pre-F-51 client projection:
/// - [`Self::from_activity`] — host/runtime [`AgentActivity`] → foreground names
/// - [`Self::project`] — full priority projection (blocked / turn / activity)
///
/// New client work surfaces must use [`Self::project_work`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentForeground {
    Idle,
    /// Follow-up accepted but not yet executing.
    Queued,
    Running,
    /// Blocked on approval or structured user interaction.
    RequiresAction,
    Cancelling,
}

impl AgentForeground {
    pub fn is_busy(self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Sole `AgentActivity` → `AgentForeground` name mapping (F-22).
    ///
    /// | AgentActivity | AgentForeground |
    /// |---|---|
    /// | Idle | Idle |
    /// | Running | Running |
    /// | WaitingForApproval | RequiresAction |
    /// | Cancelling | Cancelling |
    pub fn from_activity(activity: &AgentActivity) -> Self {
        match activity {
            AgentActivity::Idle => Self::Idle,
            AgentActivity::Running => Self::Running,
            AgentActivity::WaitingForApproval => Self::RequiresAction,
            AgentActivity::Cancelling => Self::Cancelling,
        }
    }

    /// Compatibility foreground projection (F-22 / D-34).
    ///
    /// Priority (highest first):
    /// 1. `blocked_on_user` (pending approval or interaction for this agent)
    ///    → [`RequiresAction`](Self::RequiresAction)
    /// 2. Active turn [`TurnStatus`] when present
    /// 3. Host [`AgentActivity`] via [`from_activity`](Self::from_activity)
    /// 4. [`Idle`](Self::Idle)
    ///
    /// Retained for clients that still consume Turn/Activity projections;
    /// F-51 clients use [`Self::project_work`] instead.
    pub fn project(
        blocked_on_user: bool,
        turn_status: Option<crate::TurnStatus>,
        activity: Option<&AgentActivity>,
    ) -> Self {
        if blocked_on_user {
            return Self::RequiresAction;
        }
        if let Some(status) = turn_status {
            return match status {
                crate::TurnStatus::Queued => Self::Queued,
                crate::TurnStatus::Running => Self::Running,
                crate::TurnStatus::WaitingForApproval => Self::RequiresAction,
                crate::TurnStatus::Cancelling => Self::Cancelling,
                crate::TurnStatus::Completed
                | crate::TurnStatus::Failed
                | crate::TurnStatus::Cancelled => Self::Idle,
            };
        }
        activity.map(Self::from_activity).unwrap_or(Self::Idle)
    }

    /// Project the canonical Agent work view using the shared priority:
    /// requires-action > cancelling > running > queued > idle.
    pub fn project_work(
        active_run: Option<&crate::ActiveRunSnapshot>,
        pending_action: Option<&crate::PendingActionSummary>,
        has_queued_input: bool,
    ) -> Self {
        if pending_action.is_some() {
            return Self::RequiresAction;
        }
        if let Some(run) = active_run {
            return match run.state {
                crate::AgentRunViewState::RequiresAction => Self::RequiresAction,
                crate::AgentRunViewState::Cancelling => Self::Cancelling,
                crate::AgentRunViewState::Starting | crate::AgentRunViewState::Running => {
                    Self::Running
                }
                crate::AgentRunViewState::Completed
                | crate::AgentRunViewState::Failed
                | crate::AgentRunViewState::Cancelled => Self::Idle,
            };
        }
        if has_queued_input {
            Self::Queued
        } else {
            Self::Idle
        }
    }
}

#[cfg(test)]
mod foreground_tests {
    use super::*;
    use crate::{ActiveRunSnapshot, AgentRunViewState, PendingActionSummary, TurnStatus};

    #[test]
    fn from_activity_is_sole_activity_table() {
        assert_eq!(
            AgentForeground::from_activity(&AgentActivity::Idle),
            AgentForeground::Idle
        );
        assert_eq!(
            AgentForeground::from_activity(&AgentActivity::Running),
            AgentForeground::Running
        );
        assert_eq!(
            AgentForeground::from_activity(&AgentActivity::WaitingForApproval),
            AgentForeground::RequiresAction
        );
        assert_eq!(
            AgentForeground::from_activity(&AgentActivity::Cancelling),
            AgentForeground::Cancelling
        );
    }

    #[test]
    fn project_priority_blocked_over_running_turn() {
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
    fn project_turn_statuses() {
        assert_eq!(
            AgentForeground::project(false, Some(TurnStatus::Queued), None),
            AgentForeground::Queued
        );
        assert_eq!(
            AgentForeground::project(false, Some(TurnStatus::Running), None),
            AgentForeground::Running
        );
        assert_eq!(
            AgentForeground::project(false, Some(TurnStatus::WaitingForApproval), None),
            AgentForeground::RequiresAction
        );
        assert_eq!(
            AgentForeground::project(false, Some(TurnStatus::Cancelling), None),
            AgentForeground::Cancelling
        );
        assert_eq!(
            AgentForeground::project(false, Some(TurnStatus::Completed), None),
            AgentForeground::Idle
        );
    }

    #[test]
    fn project_falls_back_to_activity() {
        assert_eq!(
            AgentForeground::project(false, None, Some(&AgentActivity::Cancelling)),
            AgentForeground::Cancelling
        );
        assert_eq!(
            AgentForeground::project(false, None, None),
            AgentForeground::Idle
        );
    }

    #[test]
    fn project_work_treats_terminal_runs_as_idle() {
        let mut run = ActiveRunSnapshot {
            run_id: "run".into(),
            root_input_id: "input".into(),
            user_turn_id: None,
            state: AgentRunViewState::Completed,
            active_model_step_id: None,
            started_at: 1,
        };
        assert_eq!(
            AgentForeground::project_work(Some(&run), None, true),
            AgentForeground::Idle
        );
        run.state = AgentRunViewState::Failed;
        assert_eq!(
            AgentForeground::project_work(Some(&run), None, true),
            AgentForeground::Idle
        );
        run.state = AgentRunViewState::Cancelled;
        assert_eq!(
            AgentForeground::project_work(Some(&run), None, true),
            AgentForeground::Idle
        );
    }

    #[test]
    fn project_work_prioritizes_action_and_cancellation() {
        let mut run = ActiveRunSnapshot {
            run_id: "run".into(),
            root_input_id: "input".into(),
            user_turn_id: None,
            state: AgentRunViewState::Running,
            active_model_step_id: None,
            started_at: 1,
        };
        let action = PendingActionSummary {
            action_id: "action".into(),
            kind: "approval".into(),
            summary: None,
        };
        assert_eq!(
            AgentForeground::project_work(Some(&run), Some(&action), true),
            AgentForeground::RequiresAction
        );
        run.state = AgentRunViewState::Cancelling;
        assert_eq!(
            AgentForeground::project_work(Some(&run), None, true),
            AgentForeground::Cancelling
        );
    }
}
