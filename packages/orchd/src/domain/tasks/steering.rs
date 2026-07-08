// ---- Domain: task steering ----
//
// Steering messages allow parent agents or users to inject guidance
// into a running task.

/// A steering message injected into an agent's task by a parent agent or user.
#[derive(Debug, Clone)]
pub struct SteerMessage {
    pub source_task_id: String,
    pub source_agent_id: String,
    pub message: String,
    pub senders: Option<crate::runtime::dispatch::DispatchSenders>,
}
