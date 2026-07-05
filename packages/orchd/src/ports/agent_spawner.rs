// ---- Port: AgentSpawner — sub-agent lifecycle abstraction ----
//
// The agent loop calls this trait when it needs to spawn, await, or steer
// sub-agents. The Supervisor implements this trait.

use crate::domain::tasks::task::HostTaskContext;
use async_trait::async_trait;

/// Result reported by a sub-agent after completion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentReport {
    /// Human-readable text from the sub-agent's final output.
    pub text: String,
    /// "completed" | "error" | "cancelled" | "detached"
    pub status: String,
    /// Number of LLM steps taken by the sub-agent.
    pub total_steps: u32,
    /// Task handle for await_task, set when spawn degrades to detached.
    pub task_id: Option<String>,
}

#[async_trait]
pub trait AgentSpawner: Send + Sync {
    /// Synchronous spawn — blocks until the sub-agent completes.
    /// Returns the sub-agent's final report, or None on timeout / not found.
    async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Option<AgentReport>;

    /// Asynchronous spawn — creates a sub-agent driver and returns immediately
    /// with a task_id. Events are published through the task event hub.
    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String;

    /// Poll a detached task for its result, optionally blocking up to
    /// `timeout_ms` milliseconds (polls every 50ms). Returns `None` if
    /// the task hasn't completed within the timeout.
    async fn poll_task(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<AgentReport>;

    /// Send a steering message to a running sub-agent.
    async fn steer_task(&self, task_id: &str, message: &str) -> bool;

    /// List all registered named agents available for spawning.
    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec>;
}

/// No-op spawner that rejects all sub-agent operations.
pub struct NoopAgentSpawner;

#[async_trait]
impl AgentSpawner for NoopAgentSpawner {
    async fn spawn(
        &self,
        _agent_id: &str,
        _prompt: &str,
        _parent_task_id: Option<String>,
        _hc: HostTaskContext,
        _senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Option<AgentReport> {
        None
    }
    async fn spawn_detached(
        &self,
        _agent_id: &str,
        _prompt: &str,
        _parent_task_id: Option<String>,
        _hc: HostTaskContext,
        _senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String {
        String::new()
    }
    async fn poll_task(&self, _task_id: &str, _timeout_ms: Option<u64>) -> Option<AgentReport> {
        None
    }
    async fn steer_task(&self, _task_id: &str, _message: &str) -> bool {
        false
    }
    
    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec> {
        Vec::new()
    }
}
