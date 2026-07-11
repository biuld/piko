// ---- Domain: work — task-level input-driven execution cycle ----

pub use piko_protocol::agent_runtime::{WorkId, WorkSnapshot};

/// Runtime context for a single work cycle within a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkContext {
    pub work_id: WorkId,
    pub task_id: piko_protocol::agent_runtime::TaskId,
    pub source_turn_id: Option<String>,
}
