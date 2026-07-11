// ---- Domain: work completion report ----

/// Result reported by a delegated agent task after completion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskReport {
    /// Human-readable text from the delegated task's final output.
    pub text: String,
    /// "completed" | "error" | "cancelled" | "detached" | "idle"
    pub status: String,
    /// Number of LLM steps taken by the delegated task.
    pub total_steps: u32,
    /// Task handle for await_task, set when spawn degrades to detached.
    pub task_id: Option<String>,
}
