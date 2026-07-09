use crate::runtime::dispatch::DispatchSenders;

/// A steering message injected into an agent's task by a parent agent or user.
/// Carries runtime dispatch senders for inter-task stream linking.
#[derive(Debug, Clone)]
pub struct TaskSteerMessage {
    pub source_task_id: String,
    pub source_agent_id: String,
    pub message: String,
    pub senders: Option<DispatchSenders>,
}

#[derive(Debug, Clone)]
pub enum TaskControlMessage {
    Steer(TaskSteerMessage),
    Close,
    Reopen,
}

/// A tool call entity produced by step dispatch and consumed by the tool executor.
#[derive(Clone, Debug)]
pub struct ToolCallItem {
    pub tool_call_index: u32,
    pub content_index: u32,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
