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
/// **Sole mapping tables** for client projection:
/// - [`Self::from_activity`] — host/runtime [`AgentActivity`] → foreground names
/// - [`Self::project`] — full priority projection (blocked / turn / activity)
///
/// Clients must call these helpers rather than inventing parallel tables.
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

    /// Shared client foreground projection (F-22 / D-34).
    ///
    /// Priority (highest first):
    /// 1. `blocked_on_user` (pending approval or interaction for this agent)
    ///    → [`RequiresAction`](Self::RequiresAction)
    /// 2. Active turn [`TurnStatus`] when present
    /// 3. Host [`AgentActivity`] via [`from_activity`](Self::from_activity)
    /// 4. [`Idle`](Self::Idle)
    ///
    /// TUI, GUI, and client-core must use this function so Queued / Running /
    /// RequiresAction / Cancelling stay aligned.
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
}

#[cfg(test)]
mod foreground_tests {
    use super::*;
    use crate::TurnStatus;

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
}
