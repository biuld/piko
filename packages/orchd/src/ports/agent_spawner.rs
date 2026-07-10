// ---- Port: AgentSpawner — delegated agent task lifecycle abstraction ----
//
// The agent loop calls this trait when it needs to spawn, await, or steer
// delegated agent tasks. The Supervisor implements this trait.

use crate::domain::tasks::task::HostTaskContext;
use async_trait::async_trait;

/// Result reported by a delegated agent task after completion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentReport {
    /// Human-readable text from the delegated task's final output.
    pub text: String,
    /// "completed" | "error" | "cancelled" | "detached"
    pub status: String,
    /// Number of LLM steps taken by the delegated task.
    pub total_steps: u32,
    /// Task handle for await_task, set when spawn degrades to detached.
    pub task_id: Option<String>,
}

#[async_trait]
pub trait AgentSpawner: Send + Sync {
    /// Synchronous spawn — blocks until the delegated task completes.
    /// Returns the delegated task's final report, or None on timeout / not found.
    async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Option<AgentReport>;

    /// Asynchronous spawn — creates a delegated task driver and returns immediately
    /// with a task_id. Events are published through the task event hub.
    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String;

    /// Poll a detached task for its result, optionally blocking up to
    /// `timeout_ms` milliseconds (polls every 50ms). Returns `None` if
    /// the task hasn't completed within the timeout.
    async fn poll_task(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<AgentReport>;

    /// Send a steering message to an existing delegated task.
    async fn steer_task(
        &self,
        task_id: &str,
        message: &str,
        source_task_id: Option<String>,
        source_agent_id: Option<String>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> bool;

    /// List all registered named agents available for spawning.
    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec>;
}

/// No-op spawner that rejects all delegated task operations.
pub struct NoopAgentSpawner;

#[async_trait]
impl AgentSpawner for NoopAgentSpawner {
    async fn spawn(
        &self,
        _agent_id: &str,
        _prompt: &str,
        _source_agent_id: Option<String>,
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
        _source_agent_id: Option<String>,
        _parent_task_id: Option<String>,
        _hc: HostTaskContext,
        _senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String {
        String::new()
    }
    async fn poll_task(&self, _task_id: &str, _timeout_ms: Option<u64>) -> Option<AgentReport> {
        None
    }
    async fn steer_task(
        &self,
        _task_id: &str,
        _message: &str,
        _source_task_id: Option<String>,
        _source_agent_id: Option<String>,
        _senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> bool {
        false
    }

    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec> {
        Vec::new()
    }
}
