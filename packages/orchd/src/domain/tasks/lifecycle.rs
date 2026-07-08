// ---- Domain: task lifecycle ----
//
// Lifecycle transitions for orchestrator-managed tasks.
// These are pure functions that compute new states from events.

use super::task::AgentTaskState;
use super::task::AgentTaskStatus;

/// Check if a state transition is valid under the Task state machine.
pub fn can_transition(from: &AgentTaskStatus, to: &AgentTaskStatus) -> bool {
    match from {
        AgentTaskStatus::Queued => matches!(
            to,
            AgentTaskStatus::Running
                | AgentTaskStatus::Closed
                | AgentTaskStatus::Completed
                | AgentTaskStatus::Failed
                | AgentTaskStatus::Cancelled
        ),
        AgentTaskStatus::Running => matches!(
            to,
            AgentTaskStatus::Idle
                | AgentTaskStatus::Closed
                | AgentTaskStatus::Completed
                | AgentTaskStatus::Failed
                | AgentTaskStatus::Cancelled
        ),
        AgentTaskStatus::Idle => matches!(
            to,
            AgentTaskStatus::Running
                | AgentTaskStatus::Closed
                | AgentTaskStatus::Completed
                | AgentTaskStatus::Failed
                | AgentTaskStatus::Cancelled
        ),
        AgentTaskStatus::Closed => matches!(to, AgentTaskStatus::Idle),
        AgentTaskStatus::Completed | AgentTaskStatus::Failed | AgentTaskStatus::Cancelled => false, // Terminal states cannot transition
    }
}

/// Transition a task to Started (Running).
pub fn task_started(state: &mut AgentTaskState) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Running) {
        return Err(format!(
            "Invalid transition from {:?} to Running",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Running;
    Ok(())
}

/// Transition a task to Idle.
pub fn task_idle(state: &mut AgentTaskState) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Idle) {
        return Err(format!(
            "Invalid transition from {:?} to Idle",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Idle;
    Ok(())
}

/// Transition a task to Completed.
pub fn task_completed(
    state: &mut AgentTaskState,
    summary: String,
    artifacts: Option<Vec<super::task::AgentArtifact>>,
) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Completed) {
        return Err(format!(
            "Invalid transition from {:?} to Completed",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Completed;
    state.result = Some(piko_protocol::agents::AgentTaskResult { summary, artifacts });
    state.error = None;
    Ok(())
}

/// Transition a task to Failed.
pub fn task_failed(state: &mut AgentTaskState, error: String) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Failed) {
        return Err(format!(
            "Invalid transition from {:?} to Failed",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Failed;
    state.error = Some(error);
    Ok(())
}

/// Transition a task to Cancelled.
pub fn task_cancelled(state: &mut AgentTaskState, reason: Option<String>) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Cancelled) {
        return Err(format!(
            "Invalid transition from {:?} to Cancelled",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Cancelled;
    state.error = reason;
    Ok(())
}

/// Transition a task to Closed.
pub fn task_closed(state: &mut AgentTaskState) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Closed) {
        return Err(format!(
            "Invalid transition from {:?} to Closed",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Closed;
    Ok(())
}

/// Reopen a closed task to Idle.
pub fn task_reopened(state: &mut AgentTaskState) -> Result<(), String> {
    if !can_transition(&state.status, &AgentTaskStatus::Idle) {
        return Err(format!(
            "Invalid transition from {:?} to Idle",
            state.status
        ));
    }
    state.status = AgentTaskStatus::Idle;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::agents::TaskSource;

    fn mock_task_state(status: AgentTaskStatus) -> AgentTaskState {
        AgentTaskState {
            id: "t1".into(),
            target_agent_id: "main".into(),
            prompt: "test".into(),
            source: TaskSource::User,
            status,
            priority: 0,
            parent_task_id: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn test_valid_transitions() {
        let mut state = mock_task_state(AgentTaskStatus::Queued);
        assert!(task_started(&mut state).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Running);

        assert!(task_idle(&mut state).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Idle);

        assert!(task_started(&mut state).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Running);

        assert!(task_completed(&mut state, "done".into(), None).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Completed);
    }

    #[test]
    fn test_closed_can_reopen_to_idle() {
        let mut state = mock_task_state(AgentTaskStatus::Idle);
        assert!(task_closed(&mut state).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Closed);

        assert!(task_reopened(&mut state).is_ok());
        assert_eq!(state.status, AgentTaskStatus::Idle);
    }

    #[test]
    fn test_invalid_terminal_transitions() {
        let mut state = mock_task_state(AgentTaskStatus::Completed);
        assert!(task_started(&mut state).is_err());
        assert!(task_idle(&mut state).is_err());
        assert!(task_cancelled(&mut state, None).is_err());
    }
}
