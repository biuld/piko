use piko_protocol::agent_runtime::{SubmitTaskInput, TaskControlRequest};

/// Internal delivery wrapper for runtime mailbox input.
#[derive(Debug, Clone)]
pub struct TaskInputEnvelope {
    pub input: SubmitTaskInput,
}

/// Unified task mailbox used by the runtime state machine.
#[derive(Debug, Clone)]
pub(crate) enum TaskMailboxMessage {
    Input(TaskInputEnvelope),
    Control(TaskControlRequest),
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
