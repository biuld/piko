// ---- Domain: agent runtime status ----
//
// AgentStatus represents the lifecycle state of an agent actor.
// It is a pure domain enum, independent of the actor runtime.

/// Lifecycle status of an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    /// No task is currently executing.
    Idle,
    /// A task is actively being processed.
    Running,
    /// A cancellation has been requested; the agent is winding down.
    Cancelling,
}
