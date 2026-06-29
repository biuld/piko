// ---- Domain: task lifecycle ----
//
// Lifecycle transitions for orchestrator-managed tasks.
// These are pure functions that compute new states from events.

use super::task::AgentTaskState;
use super::task::AgentTaskStatus;

/// Transition a task to Started.
pub fn task_started(state: &mut AgentTaskState) {
    state.status = AgentTaskStatus::Running;
}

/// Transition a task to Completed.
pub fn task_completed(
    state: &mut AgentTaskState,
    summary: String,
    artifacts: Option<Vec<super::task::AgentArtifact>>,
) {
    state.status = AgentTaskStatus::Completed;
    state.result = Some(piko_protocol::agents::AgentTaskResult { summary, artifacts });
    state.error = None;
}

/// Transition a task to Failed.
pub fn task_failed(state: &mut AgentTaskState, error: String) {
    state.status = AgentTaskStatus::Failed;
    state.error = Some(error);
}

/// Transition a task to Cancelled.
pub fn task_cancelled(state: &mut AgentTaskState, reason: Option<String>) {
    state.status = AgentTaskStatus::Cancelled;
    state.error = reason;
}
