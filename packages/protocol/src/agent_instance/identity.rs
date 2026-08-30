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
/// This is a projection of host-authoritative work facts and pending actions,
/// not a separate orchd-owned state machine. Maps to ACP
/// v2-style session readiness semantics (`idle` / `running` / `requires_action`).
///
/// Runtime-only views may fall back through [`Self::from_activity`]. Product
/// clients use [`Self::project_work`].
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

    /// Project the canonical Agent work view using the shared priority:
    /// requires-action > cancelling > running > queued > idle.
    pub fn project_work(
        active_work: Option<&crate::ActiveWorkSnapshot>,
        pending_action: Option<&crate::PendingActionSummary>,
        has_queued_input: bool,
    ) -> Self {
        if pending_action.is_some() {
            return Self::RequiresAction;
        }
        if let Some(work) = active_work {
            return match work.state {
                crate::AgentWorkViewState::RequiresAction => Self::RequiresAction,
                crate::AgentWorkViewState::Cancelling => Self::Cancelling,
                crate::AgentWorkViewState::Starting | crate::AgentWorkViewState::Running => {
                    Self::Running
                }
                crate::AgentWorkViewState::Completed
                | crate::AgentWorkViewState::Failed
                | crate::AgentWorkViewState::Cancelled => Self::Idle,
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
    use crate::{ActiveWorkSnapshot, AgentWorkViewState, PendingActionSummary};

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
    fn project_work_treats_terminal_work_as_idle() {
        let mut work = ActiveWorkSnapshot {
            root_input_id: "input".into(),
            state: AgentWorkViewState::Completed,
            active_model_step_id: None,
            started_at: 1,
        };
        assert_eq!(
            AgentForeground::project_work(Some(&work), None, true),
            AgentForeground::Idle
        );
        work.state = AgentWorkViewState::Failed;
        assert_eq!(
            AgentForeground::project_work(Some(&work), None, true),
            AgentForeground::Idle
        );
        work.state = AgentWorkViewState::Cancelled;
        assert_eq!(
            AgentForeground::project_work(Some(&work), None, true),
            AgentForeground::Idle
        );
    }

    #[test]
    fn project_work_prioritizes_action_and_cancellation() {
        let mut work = ActiveWorkSnapshot {
            root_input_id: "input".into(),
            state: AgentWorkViewState::Running,
            active_model_step_id: None,
            started_at: 1,
        };
        let action = PendingActionSummary {
            action_id: "action".into(),
            kind: "approval".into(),
            summary: None,
        };
        assert_eq!(
            AgentForeground::project_work(Some(&work), Some(&action), true),
            AgentForeground::RequiresAction
        );
        work.state = AgentWorkViewState::Cancelling;
        assert_eq!(
            AgentForeground::project_work(Some(&work), None, true),
            AgentForeground::Cancelling
        );
    }
}
